//! Entraînement des jeunes (extension propre au portage Rust, absente du C++).
//!
//! Les tireurs présents sont répartis en groupes (8 par défaut). Chaque
//! groupe devient une compétition autonome : des poules (2 de 4 par défaut),
//! puis un tableau où toutes les places sont tirées. Il y a donc autant de
//! vainqueurs que de groupes, et avec 2 poules de 4 chacun tire 3 matchs de
//! poule et 3 matchs de tableau.
//!
//! - Groupes « par division » : les meilleurs au classement dans le groupe 1,
//!   les suivants dans le groupe 2… ; « au classement » : serpentin, chaque
//!   groupe a un niveau équivalent.
//! - Poules « en serpentin » (1-4-5-8 / 2-3-6-7) ou « par niveau » (les
//!   meilleurs du groupe dans la poule 1).
//! - Tableau croisé : 1er de la poule 1 contre 4e de la poule 2, 2e contre
//!   3e… (sortie des poules ordonnée par place puis par numéro de poule).

use std::any::Any;

use crate::player::{PlayerRef, PlayerStore};
use crate::rounds::table::common::{self, codes};
use crate::stage::{RankEntry, Stage, StageBase, StageContext, StageKind};
use crate::xml::XmlWriter;

/// Taille de poule permise.
pub const MIN_POOL_SIZE: u32 = 3;
pub const MAX_POOL_SIZE: u32 = 6;

/// Constitution des groupes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupMode {
    /// Les meilleurs dans le premier groupe.
    Division,
    /// Serpentin : groupes de niveau équivalent.
    Ranking,
}

/// Constitution des poules d'un groupe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PoolMode {
    /// 1-4-5-8 / 2-3-6-7.
    Snake,
    /// Les meilleurs du groupe dans la poule 1.
    Level,
}

/// Réglages de la formule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct YouthSettings {
    pub group_size: u32,
    pub pools_per_group: u32,
    pub group_mode: GroupMode,
    pub pool_mode: PoolMode,
}

impl Default for YouthSettings {
    fn default() -> Self {
        YouthSettings { group_size: 8, pools_per_group: 2, group_mode: GroupMode::Division, pool_mode: PoolMode::Snake }
    }
}

/// Un groupe : ses tireurs (dans l'ordre du classement d'entrée) et la
/// taille de chacune de ses poules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub players: Vec<PlayerRef>,
    pub pool_sizes: Vec<u32>,
}

/// Une répartition proposée pour `n` tireurs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Proposal {
    pub group_size: u32,
    pub pools_per_group: u32,
    pub nb_groups: u32,
    /// Plus petit et plus grand groupe.
    pub smallest: u32,
    pub largest: u32,
    /// Matchs par tireur : poule + tableau (pour le groupe le plus grand).
    pub bouts_per_fencer: u32,
    /// Pénalité (0 : idéal). Sert à trier les propositions.
    pub penalty: u32,
}

/// Tailles des groupes : `nb_groups` groupes aussi égaux que possible, les
/// plus grands en premier.
pub fn group_sizes(n: u32, nb_groups: u32) -> Vec<u32> {
    let k = nb_groups.max(1).min(n.max(1));
    (0..k).map(|i| n / k + u32::from(i < n % k)).collect()
}

/// Nombre de groupes pour une taille de groupe visée.
pub fn nb_groups(n: u32, group_size: u32) -> u32 {
    if n == 0 {
        return 0;
    }
    let g = group_size.max(1);
    ((n as f64 / g as f64).round() as u32).max(1)
}

/// Tailles des poules d'un groupe (les plus grandes en premier).
pub fn pool_sizes(group: u32, pools: u32) -> Vec<u32> {
    let p = pools.max(1).min(group.max(1));
    (0..p).map(|i| group / p + u32::from(i < group % p)).collect()
}

/// Matchs par tireur dans le plus grand groupe : poule, puis tableau où
/// toutes les places sont tirées (log2 du tableau, exemptions comprises).
pub fn bouts_per_fencer(group: u32, pools: u32) -> u32 {
    let pool = pool_sizes(group, pools).first().copied().unwrap_or(1);
    let table = group.max(1).next_power_of_two().trailing_zeros();
    pool.saturating_sub(1) + table
}

fn penalty(sizes: &[u32], pools: u32, wanted: &YouthSettings) -> Option<u32> {
    let mut penalty = 0;
    for &g in sizes {
        let ps = pool_sizes(g, pools);
        if ps.iter().any(|s| *s < MIN_POOL_SIZE || *s > MAX_POOL_SIZE) {
            return None;
        }
        // Exemptions du tableau : des tireurs sans adversaire au premier tour.
        penalty += (g.next_power_of_two() - g) * 3;
        // Poules de tailles différentes dans un même groupe.
        penalty += (ps[0] - ps[ps.len() - 1]) * 2;
        // Écart à la taille de poule visée.
        let target = (wanted.group_size / wanted.pools_per_group.max(1)).max(MIN_POOL_SIZE);
        penalty += ps.iter().map(|s| s.abs_diff(target)).sum::<u32>();
    }
    if let (Some(min), Some(max)) = (sizes.iter().min(), sizes.iter().max()) {
        penalty += (max - min) * 2;
    }
    penalty += wanted.group_size.abs_diff(sizes.iter().max().copied().unwrap_or(0));
    Some(penalty)
}

/// Répartitions possibles pour `n` tireurs, la meilleure en premier : des
/// groupes de 4 à 16 tireurs, des poules de 3 à 6, des tableaux aussi pleins
/// que possible et proches des réglages demandés.
pub fn proposals(n: u32, wanted: &YouthSettings) -> Vec<Proposal> {
    let mut out: Vec<Proposal> = Vec::new();
    if n < MIN_POOL_SIZE {
        return out;
    }
    for nb in 1..=n / MIN_POOL_SIZE {
        let sizes = group_sizes(n, nb);
        let largest = sizes[0];
        let smallest = *sizes.last().unwrap_or(&largest);
        if largest > 16 || smallest < 4.min(n) {
            continue;
        }
        for pools in 1..=4 {
            let Some(p) = penalty(&sizes, pools, wanted) else { continue };
            let proposal = Proposal {
                group_size: largest,
                pools_per_group: pools,
                nb_groups: nb,
                smallest,
                largest,
                bouts_per_fencer: bouts_per_fencer(largest, pools),
                penalty: p,
            };
            if !out.iter().any(|o| o.nb_groups == nb && o.pools_per_group == pools) {
                out.push(proposal);
            }
        }
    }
    out.sort_by_key(|p| (p.penalty, p.nb_groups, p.pools_per_group));
    out
}

/// Groupes formés à partir des tireurs dans l'ordre du classement d'entrée.
pub fn make_groups(players: &[PlayerRef], settings: &YouthSettings) -> Vec<Group> {
    let n = players.len() as u32;
    if n == 0 {
        return Vec::new();
    }
    let sizes = group_sizes(n, nb_groups(n, settings.group_size));
    let k = sizes.len();
    let mut groups: Vec<Vec<PlayerRef>> = vec![Vec::new(); k];
    match settings.group_mode {
        GroupMode::Division => {
            let mut it = players.iter().copied();
            for (g, size) in sizes.iter().enumerate() {
                groups[g].extend(it.by_ref().take(*size as usize));
            }
        }
        GroupMode::Ranking => {
            // Serpentin : 1, 2, …, k, k, …, 2, 1, 1, 2…
            let mut g = 0usize;
            let mut forward = true;
            for p in players {
                while groups[g].len() as u32 >= sizes[g] {
                    g = (g + 1) % k;
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
    }
    groups
        .into_iter()
        .map(|players| {
            let size = players.len() as u32;
            Group { players, pool_sizes: pool_sizes(size, settings.pools_per_group) }
        })
        .collect()
}

/// Phase « Entraînement des jeunes ».
pub struct YouthTraining {
    base: StageBase,
    pub settings: YouthSettings,
}

impl YouthTraining {
    pub fn new(id: u32) -> Self {
        let mut base = StageBase::new(StageKind::YouthTraining, id);
        base.filter = codes(&["stage_start_rank", "name", "first_name", "club", "ranking"]);
        YouthTraining { base, settings: YouthSettings::default() }
    }

    /// Groupes avec les réglages courants.
    pub fn groups(&self) -> Vec<Group> {
        make_groups(&self.base.attendees, &self.settings)
    }

    pub fn proposals(&self) -> Vec<Proposal> {
        proposals(self.base.attendees.len() as u32, &self.settings)
    }

    /// Applique une proposition (taille des groupes et nombre de poules).
    pub fn apply(&mut self, p: &Proposal) {
        self.settings.group_size = p.group_size;
        self.settings.pools_per_group = p.pools_per_group;
    }
}

impl Stage for YouthTraining {
    fn base(&self) -> &StageBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut StageBase {
        &mut self.base
    }

    fn set_attendees(&mut self, ctx: &mut StageContext, attendees: Vec<PlayerRef>) {
        common::retrieve_attendees(&mut self.base, ctx, attendees);
    }

    fn load(&mut self, ctx: &mut StageContext, node: roxmltree::Node) -> Result<(), crate::Error> {
        common::load_configuration(&mut self.base, node);
        let n = |name: &str, def: u32| node.attribute(name).and_then(|v| v.parse::<u32>().ok()).filter(|v| *v > 0).unwrap_or(def);
        let d = YouthSettings::default();
        self.settings = YouthSettings {
            group_size: n("TailleGroupe", d.group_size),
            pools_per_group: n("PoulesParGroupe", d.pools_per_group),
            group_mode: if node.attribute("Groupes") == Some("Classement") { GroupMode::Ranking } else { GroupMode::Division },
            pool_mode: if node.attribute("Poules") == Some("Niveau") { PoolMode::Level } else { PoolMode::Snake },
        };
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
        w.attr("Groupes", if self.settings.group_mode == GroupMode::Ranking { "Classement" } else { "Division" });
        w.attr("Poules", if self.settings.pool_mode == PoolMode::Level { "Niveau" } else { "Serpentin" });
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

/// Les tireurs présents d'un magasin, utile aux tests.
pub fn attending(players: &PlayerStore, refs: &[PlayerRef]) -> Vec<PlayerRef> {
    refs.iter().copied().filter(|r| players.get(*r).map(|p| p.attending()).unwrap_or(false)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_proposal_is_groups_of_eight_with_two_pools() {
        let best = proposals(24, &YouthSettings::default())[0];
        assert_eq!((best.nb_groups, best.group_size, best.pools_per_group), (3, 8, 2));
        assert_eq!(best.bouts_per_fencer, 6);
    }

    #[test]
    fn division_and_snake_groups() {
        let players: Vec<PlayerRef> = (1..=16).collect();
        let s = YouthSettings::default();
        let g = make_groups(&players, &s);
        assert_eq!(g[0].players, (1..=8).collect::<Vec<_>>());
        assert_eq!(g[1].players, (9..=16).collect::<Vec<_>>());
        let g = make_groups(&players, &YouthSettings { group_mode: GroupMode::Ranking, ..s });
        assert_eq!(g[0].players, vec![1, 4, 5, 8, 9, 12, 13, 16]);
        assert_eq!(g[1].players, vec![2, 3, 6, 7, 10, 11, 14, 15]);
    }

    #[test]
    fn uneven_counts_stay_balanced() {
        let g = make_groups(&(1..=21).collect::<Vec<_>>(), &YouthSettings::default());
        assert_eq!(g.iter().map(|g| g.players.len()).collect::<Vec<_>>(), vec![7, 7, 7]);
        assert_eq!(g[0].pool_sizes, vec![4, 3]);
    }
}
