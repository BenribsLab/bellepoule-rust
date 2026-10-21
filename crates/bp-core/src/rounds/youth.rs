//! Entraînement des jeunes (extension propre au portage Rust, absente du C++).
//!
//! Les tireurs présents sont répartis en groupes (8 par défaut). Chaque
//! groupe devient une compétition autonome : des poules (2 par défaut), puis
//! un tableau croisé (1er de la poule 1 contre dernier de la poule 2…) où
//! l'on tire toutes les places, la 3e place seulement, ou aucune.
//!
//! Les réglages par défaut (préférences de la phase) : taille des groupes,
//! poules par groupe, constitution des groupes (par division, au classement
//! en serpentin ou au hasard, avec le décalage par club des poules),
//! constitution des poules d'un groupe, places tirées. Sur l'écran de la
//! phase, chaque groupe peut ensuite changer de taille et de nombre de poules.

use std::any::Any;

use crate::player::{PlayerRef, PlayerStore};
use crate::rounds::pool::neo_swapper::NeoSwapper;
use crate::rounds::pool::pool::random_compare;
use crate::rounds::table::common::{self, codes};
use crate::rounds::table::supervisor::FencedPlaces;
use crate::stage::{RankEntry, Stage, StageBase, StageContext, StageKind};
use crate::xml::XmlWriter;

/// Taille de poule permise dans un groupe.
pub const MIN_POOL_SIZE: u32 = 3;
pub const MAX_POOL_SIZE: u32 = 8;

/// Constitution des groupes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupMode {
    /// Groupes de niveau : les meilleurs dans le groupe 1.
    Division,
    /// Serpentin sur le classement, avec décalage : groupes de même niveau.
    Ranking,
    /// Au hasard, avec décalage.
    Random,
}

/// Constitution des poules d'un groupe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PoolMode {
    /// 1-4-5-8 / 2-3-6-7, avec décalage.
    Snake,
    /// Les meilleurs du groupe dans la poule 1.
    Level,
}

/// Réglages par défaut de la phase.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct YouthSettings {
    pub group_size: u32,
    pub pools_per_group: u32,
    pub group_mode: GroupMode,
    pub pool_mode: PoolMode,
    /// Critères de décalage (codes : `club`, `league`…), comme les poules.
    pub swapping: Vec<String>,
    /// Places tirées dans le tableau de chaque groupe.
    pub fenced_places: FencedPlaces,
}

impl Default for YouthSettings {
    fn default() -> Self {
        YouthSettings {
            group_size: 8,
            pools_per_group: 2,
            group_mode: GroupMode::Division,
            pool_mode: PoolMode::Snake,
            swapping: vec!["club".to_string()],
            fenced_places: FencedPlaces::AllPlaces,
        }
    }
}

/// Composition d'un groupe : nombre de tireurs et de poules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GroupLayout {
    pub size: u32,
    pub nb_pools: u32,
}

/// Un groupe : ses tireurs (dans l'ordre du classement d'entrée) et sa
/// composition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub players: Vec<PlayerRef>,
    pub layout: GroupLayout,
}

impl Group {
    pub fn pool_sizes(&self) -> Vec<u32> {
        pool_sizes(self.layout.size, self.layout.nb_pools)
    }
}

/// Une formule de poules possible pour un groupe (`Allocator::Configuration`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoolChoice {
    pub nb_pools: u32,
    pub smallest: u32,
    pub largest: u32,
}

/// Tailles des poules d'un groupe (les plus grandes en premier).
pub fn pool_sizes(group: u32, pools: u32) -> Vec<u32> {
    let p = pools.max(1).min(group.max(1));
    (0..p).map(|i| group / p + u32::from(i < group % p)).collect()
}

/// Formules de poules possibles pour un groupe de `size` tireurs.
pub fn pool_choices(size: u32) -> Vec<PoolChoice> {
    (1..=size.max(1))
        .filter_map(|p| {
            let s = pool_sizes(size, p);
            let (largest, smallest) = (s[0], *s.last().unwrap_or(&s[0]));
            (smallest >= MIN_POOL_SIZE.min(size) && largest <= MAX_POOL_SIZE).then_some(PoolChoice { nb_pools: p, smallest, largest })
        })
        .collect()
}

/// Meilleure formule de poules pour un groupe : le nombre de poules demandé
/// s'il donne des poules de 3 à 8, sinon des poules au plus près de la
/// taille visée (`group_size / pools_per_group`).
pub fn best_pools(size: u32, settings: &YouthSettings) -> u32 {
    let choices = pool_choices(size);
    if choices.iter().any(|c| c.nb_pools == settings.pools_per_group) {
        return settings.pools_per_group;
    }
    let target = (settings.group_size / settings.pools_per_group.max(1)).max(MIN_POOL_SIZE);
    choices
        .iter()
        .min_by_key(|c| (c.largest.abs_diff(target) + c.smallest.abs_diff(target), c.nb_pools))
        .map(|c| c.nb_pools)
        .unwrap_or(1)
}

/// Composition par défaut : autant de groupes que la taille visée le
/// permet ; les tireurs en surplus vont dans les derniers groupes
/// (17 tireurs → 8 + 9).
pub fn default_layout(n: u32, settings: &YouthSettings) -> Vec<GroupLayout> {
    if n == 0 {
        return Vec::new();
    }
    let k = ((n / settings.group_size.max(1)).max(1)).min(n);
    let base = n / k;
    let extra = n % k;
    (0..k)
        .map(|i| {
            let size = base + u32::from(i >= k - extra);
            GroupLayout { size, nb_pools: best_pools(size, settings) }
        })
        .collect()
}

/// Matchs par tireur dans un groupe : poule, puis tableau (toutes les places
/// tirées : log2 du tableau).
pub fn bouts_per_fencer(layout: &GroupLayout, fenced: FencedPlaces) -> u32 {
    let pool = pool_sizes(layout.size, layout.nb_pools).first().copied().unwrap_or(1);
    let rounds = layout.size.max(1).next_power_of_two().trailing_zeros();
    let table = match fenced {
        FencedPlaces::AllPlaces => rounds,
        // Au moins un match ; au plus tous les tours pour le vainqueur.
        _ => rounds.min(1).max(1),
    };
    pool.saturating_sub(1) + table
}

/// Ordre au hasard reproductible.
fn shuffled(players: &[PlayerRef], seed: u32) -> Vec<PlayerRef> {
    let mut list = players.to_vec();
    crate::sort::glib_sort_by(&mut list, |a, b| random_compare(*a, *b, seed));
    list
}

/// Serpentin en respectant les tailles demandées.
fn snake_fill(groups: &mut [Vec<PlayerRef>], order: &[PlayerRef], layout: &[GroupLayout]) {
    let k = groups.len();
    let (mut g, mut forward) = (0usize, true);
    for p in order {
        let mut guard = 0;
        while groups[g].len() as u32 >= layout[g].size && guard < k {
            g = (g + 1) % k;
            guard += 1;
        }
        groups[g].push(*p);
        if k > 1 {
            if forward {
                if g + 1 == k {
                    forward = false;
                } else {
                    g += 1;
                }
            } else if g == 0 {
                forward = true;
            } else {
                g -= 1;
            }
        }
    }
}

/// Répartit `players` (dans l'ordre du classement) selon `layout`.
pub fn make_groups(
    store: &PlayerStore,
    players: &[PlayerRef],
    layout: &[GroupLayout],
    settings: &YouthSettings,
    owner: crate::player::Owner,
    seed: u32,
) -> Vec<Group> {
    let k = layout.len();
    let mut groups: Vec<Vec<PlayerRef>> = vec![Vec::new(); k];
    if k == 0 {
        return Vec::new();
    }
    let balanced = {
        let max = layout.iter().map(|l| l.size).max().unwrap_or(0);
        let min = layout.iter().map(|l| l.size).min().unwrap_or(0);
        max - min <= 1 && layout.iter().map(|l| l.size as usize).sum::<usize>() == players.len()
    };
    match settings.group_mode {
        GroupMode::Division => {
            let mut it = players.iter().copied();
            for (g, l) in layout.iter().enumerate() {
                groups[g].extend(it.by_ref().take(l.size as usize));
            }
            // Tireurs en trop (tailles modifiées à la main) : dernier groupe.
            groups[k - 1].extend(it);
        }
        GroupMode::Ranking | GroupMode::Random => {
            let order = if settings.group_mode == GroupMode::Random { shuffled(players, seed) } else { players.to_vec() };
            let mut done = false;
            if balanced {
                // Même serpentin et même décalage que les poules.
                let mut swapper = NeoSwapper::new();
                swapper.configure(k, &settings.swapping, owner);
                let mut result = swapper.swap(store, &order);
                if result.iter().map(Vec::len).sum::<usize>() == order.len() {
                    // Le décalage remplit d'abord les premiers groupes : on range
                    // les groupes obtenus selon les tailles demandées.
                    for (g, l) in layout.iter().enumerate() {
                        if let Some(i) = result.iter().position(|r| r.len() == l.size as usize) {
                            groups[g] = result.remove(i);
                        }
                    }
                    for (g, rest) in result.into_iter().enumerate() {
                        groups[(g + k - 1) % k].extend(rest);
                    }
                    done = true;
                }
            }
            if !done {
                snake_fill(&mut groups, &order, layout);
            }
            // Dans chaque groupe, les tireurs restent dans l'ordre du classement.
            for group in groups.iter_mut() {
                group.sort_by_key(|p| players.iter().position(|x| x == p).unwrap_or(usize::MAX));
            }
        }
    }
    groups
        .into_iter()
        .zip(layout.iter())
        .map(|(players, l)| {
            let size = players.len() as u32;
            let nb_pools = if size == l.size { l.nb_pools } else { best_pools(size, settings) };
            Group { players, layout: GroupLayout { size, nb_pools } }
        })
        .collect()
}

/// Phase « Entraînement des jeunes ».
pub struct YouthTraining {
    base: StageBase,
    pub settings: YouthSettings,
    /// Composition modifiée sur l'écran de la phase (sinon : par défaut).
    layout: Option<Vec<GroupLayout>>,
}

impl YouthTraining {
    pub fn new(id: u32) -> Self {
        let mut base = StageBase::new(StageKind::YouthTraining, id);
        base.filter = codes(&["stage_start_rank", "name", "first_name", "club", "ranking"]);
        YouthTraining { base, settings: YouthSettings::default(), layout: None }
    }

    /// Composition courante (modifiée ou par défaut).
    pub fn layout(&self) -> Vec<GroupLayout> {
        let n = self.base.attendees.len() as u32;
        match &self.layout {
            Some(l) if l.iter().map(|g| g.size).sum::<u32>() == n => l.clone(),
            _ => default_layout(n, &self.settings),
        }
    }

    pub fn is_customized(&self) -> bool {
        self.layout.is_some()
    }

    /// Revient à la composition par défaut.
    pub fn reset_layout(&mut self) {
        self.layout = None;
    }

    /// Change la taille du groupe `g` ; la différence est reportée sur le
    /// groupe suivant (le précédent pour le dernier).
    pub fn set_group_size(&mut self, g: usize, size: u32) {
        let mut l = self.layout();
        if g >= l.len() || l.len() < 2 {
            return;
        }
        let other = if g + 1 < l.len() { g + 1 } else { g - 1 };
        let total = l[g].size + l[other].size;
        let size = size.clamp(MIN_POOL_SIZE, total - MIN_POOL_SIZE);
        l[g].size = size;
        l[other].size = total - size;
        for i in [g, other] {
            if !pool_choices(l[i].size).iter().any(|c| c.nb_pools == l[i].nb_pools) {
                l[i].nb_pools = best_pools(l[i].size, &self.settings);
            }
        }
        self.layout = Some(l);
    }

    /// Nombre de poules du groupe `g`.
    pub fn set_group_pools(&mut self, g: usize, nb_pools: u32) {
        let mut l = self.layout();
        if let Some(group) = l.get_mut(g) {
            group.nb_pools = nb_pools.max(1);
            self.layout = Some(l);
        }
    }

    /// Ajoute (ou retire) un groupe, en revenant à des tailles équilibrées.
    pub fn set_group_count(&mut self, count: u32) {
        let n = self.base.attendees.len() as u32;
        let k = count.clamp(1, (n / MIN_POOL_SIZE).max(1));
        let base = n / k;
        let extra = n % k;
        let l = (0..k)
            .map(|i| {
                let size = base + u32::from(i >= k - extra);
                GroupLayout { size, nb_pools: best_pools(size, &self.settings) }
            })
            .collect();
        self.layout = Some(l);
    }

    /// Groupes avec la composition courante.
    pub fn groups(&self, players: &PlayerStore) -> Vec<Group> {
        make_groups(players, &self.base.attendees, &self.layout(), &self.settings, self.base.owner(), self.base.rand_seed)
    }
}

fn layout_image(l: &[GroupLayout]) -> String {
    l.iter().map(|g| format!("{}x{}", g.size, g.nb_pools)).collect::<Vec<_>>().join(",")
}

fn parse_layout(s: &str) -> Option<Vec<GroupLayout>> {
    s.split(',')
        .map(|g| {
            let (size, pools) = g.split_once('x')?;
            Some(GroupLayout { size: size.trim().parse().ok()?, nb_pools: pools.trim().parse().ok()? })
        })
        .collect()
}

impl Stage for YouthTraining {
    fn base(&self) -> &StageBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut StageBase {
        &mut self.base
    }

    fn set_attendees(&mut self, ctx: &mut StageContext, attendees: Vec<PlayerRef>) {
        if attendees.len() != self.base.attendees.len() {
            self.layout = None;
        }
        common::retrieve_attendees(&mut self.base, ctx, attendees);
    }

    fn load(&mut self, ctx: &mut StageContext, node: roxmltree::Node) -> Result<(), crate::Error> {
        common::load_configuration(&mut self.base, node);
        let n = |name: &str, def: u32| node.attribute(name).and_then(|v| v.parse::<u32>().ok()).filter(|v| *v > 0).unwrap_or(def);
        let d = YouthSettings::default();
        self.settings = YouthSettings {
            group_size: n("TailleGroupe", d.group_size),
            pools_per_group: n("PoulesParGroupe", d.pools_per_group),
            group_mode: match node.attribute("Groupes") {
                Some("Classement") => GroupMode::Ranking,
                Some("Hasard") => GroupMode::Random,
                _ => GroupMode::Division,
            },
            pool_mode: if node.attribute("Poules") == Some("Niveau") { PoolMode::Level } else { PoolMode::Snake },
            swapping: node
                .attribute("Decalage")
                .map(|v| v.split('/').filter(|s| !s.is_empty()).map(str::to_string).collect())
                .unwrap_or(d.swapping),
            fenced_places: node
                .attribute("PlacesTirees")
                .map(|v| FencedPlaces::from_value(crate::attribute::parse_leading_int(v).max(0) as u32))
                .unwrap_or(d.fenced_places),
        };
        self.layout = node.attribute("Composition").and_then(parse_layout);
        let attendees = self.base.attendees.clone();
        common::retrieve_attendees(&mut self.base, ctx, attendees);
        Ok(())
    }

    fn save(&self, _ctx: &StageContext, w: &mut XmlWriter) {
        w.start(StageKind::YouthTraining.xml_name());
        self.save_configuration(w);
        w.end();
    }

    fn save_configuration(&self, w: &mut XmlWriter) {
        common::save_configuration(&self.base, w, false);
        w.attr_fmt("TailleGroupe", self.settings.group_size);
        w.attr_fmt("PoulesParGroupe", self.settings.pools_per_group);
        w.attr(
            "Groupes",
            match self.settings.group_mode {
                GroupMode::Division => "Division",
                GroupMode::Ranking => "Classement",
                GroupMode::Random => "Hasard",
            },
        );
        w.attr("Poules", if self.settings.pool_mode == PoolMode::Level { "Niveau" } else { "Serpentin" });
        w.attr("Decalage", &format!("/{}/", self.settings.swapping.join("/")));
        w.attr_fmt("PlacesTirees", self.settings.fenced_places.value());
        if let Some(l) = &self.layout {
            w.attr("Composition", &layout_image(l));
        }
    }

    fn is_over(&self, _ctx: &StageContext) -> bool {
        true
    }

    fn classification(&self, ctx: &StageContext) -> Vec<RankEntry> {
        let list = self.base.attendees.clone();
        let rank = |r: PlayerRef| list.iter().position(|x| *x == r).map(|i| i as u32 + 1).unwrap_or(0);
        common::rank_entries(ctx.players, self.base.owner(), &self.base.attendees, &rank)
    }

    fn output(&self, _ctx: &StageContext) -> Vec<PlayerRef> {
        self.base.attendees.clone()
    }

    fn remap_players(&mut self, map: &std::collections::HashMap<PlayerRef, PlayerRef>) {
        for r in self.base.attendees.iter_mut() {
            if let Some(n) = map.get(r) {
                *r = *n;
            }
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// Fabrique des phases de ce module.
pub fn new_stage(_kind: StageKind, id: u32) -> Box<dyn Stage> {
    Box::new(YouthTraining::new(id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player::Owner;

    fn sizes(l: &[GroupLayout]) -> Vec<(u32, u32)> {
        l.iter().map(|g| (g.size, g.nb_pools)).collect()
    }

    #[test]
    fn surplus_goes_to_the_last_groups() {
        let s = YouthSettings::default();
        assert_eq!(sizes(&default_layout(16, &s)), vec![(8, 2), (8, 2)]);
        assert_eq!(sizes(&default_layout(17, &s)), vec![(8, 2), (9, 2)]);
        assert_eq!(sizes(&default_layout(23, &s)), vec![(11, 2), (12, 2)]);
        assert_eq!(pool_sizes(9, 2), vec![5, 4]);
    }

    #[test]
    fn division_groups_follow_the_ranking() {
        let players: Vec<PlayerRef> = (1..=17).collect();
        let s = YouthSettings::default();
        let g = make_groups(&PlayerStore::default(), &players, &default_layout(17, &s), &s, Owner::Stage(1), 0);
        assert_eq!(g[0].players, (1..=8).collect::<Vec<_>>());
        assert_eq!(g[1].players, (9..=17).collect::<Vec<_>>());
    }

    #[test]
    fn ranking_groups_are_snaked() {
        let players: Vec<PlayerRef> = (1..=16).collect();
        let s = YouthSettings { group_mode: GroupMode::Ranking, swapping: Vec::new(), ..YouthSettings::default() };
        let g = make_groups(&PlayerStore::default(), &players, &default_layout(16, &s), &s, Owner::Stage(1), 0);
        assert_eq!(g[0].players, vec![1, 4, 5, 8, 9, 12, 13, 16]);
        assert_eq!(g[1].players, vec![2, 3, 6, 7, 10, 11, 14, 15]);
    }

    #[test]
    fn group_sizes_can_be_changed_by_hand() {
        let mut y = YouthTraining::new(1);
        y.base.attendees = (1..=17).collect();
        y.set_group_size(0, 9);
        assert_eq!(sizes(&y.layout()), vec![(9, 2), (8, 2)]);
        y.set_group_pools(0, 3);
        assert_eq!(sizes(&y.layout()), vec![(9, 3), (8, 2)]);
        y.reset_layout();
        assert_eq!(sizes(&y.layout()), vec![(8, 2), (9, 2)]);
    }
}
