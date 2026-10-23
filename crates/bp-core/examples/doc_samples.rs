//! Compétitions d'exemple pour les captures de la documentation (noms
//! fictifs) : `cargo run -p bp-core --example doc_samples -- <dossier>`.
//!
//! - `individuel.cotcot` : fleuret M15, poules jouées, tableau commencé ;
//! - `jeunes.cotcot` : entraînement des jeunes, groupes et poules en cours ;
//! - `equipes.cotcot` : épreuve par équipes en relais, une rencontre commencée.

use std::collections::HashMap;
use std::path::PathBuf;

use bp_core::contest::{players::new_player, Contest};
use bp_core::player::{AttrId, PlayerClass, PlayerRef};
use bp_core::relay::{RelayFormat, RelayLog};
use bp_core::rounds::pool::PoolSupervisorStage;
use bp_core::rounds::table::supervisor::TableSupervisor;
use bp_core::rounds::youth::{GroupMode, YouthTraining};
use bp_core::stage::StageKind;

const NAMES: [&str; 32] = [
    "MARTIN", "BERNARD", "THOMAS", "PETIT", "ROBERT", "RICHARD", "DURAND", "DUBOIS", "MOREAU", "LAURENT", "SIMON", "MICHEL",
    "LEFEBVRE", "LEROY", "ROUX", "DAVID", "BERTRAND", "MOREL", "FOURNIER", "GIRARD", "BONNET", "DUPONT", "LAMBERT", "FONTAINE",
    "ROUSSEAU", "VINCENT", "MULLER", "LEFEVRE", "FAURE", "ANDRE", "MERCIER", "BLANC",
];
const FIRST: [&str; 32] = [
    "Lina", "Hugo", "Emma", "Nathan", "Jade", "Louis", "Chloé", "Gabriel", "Léa", "Arthur", "Alice", "Jules",
    "Rose", "Lucas", "Anna", "Adam", "Inès", "Raphaël", "Zoé", "Paul", "Camille", "Noé", "Mila", "Sacha",
    "Lou", "Tom", "Léna", "Ethan", "Nina", "Théo", "Manon", "Marius",
];
const CLUBS: [&str; 6] = ["YERRES CE", "MELUN AE", "BRUNOY AE", "MONTGERON ESC", "VIGNEUX CS", "CRETEIL US"];

fn fencer(i: usize) -> bp_core::player::Player {
    let mut p = new_player(PlayerClass::Fencer);
    p.set_str(&AttrId::global("name"), NAMES[i % 32]);
    p.set_str(&AttrId::global("first_name"), FIRST[i % 32]);
    p.set_str(&AttrId::global("club"), CLUBS[i % CLUBS.len()]);
    p.set_str(&AttrId::global("league"), "ILE-DE-FRANCE");
    p.set_str(&AttrId::global("country"), "FRA");
    p.set_uint(&AttrId::global("ranking"), i as u32 + 1);
    p.set_uint(&AttrId::global("attending"), 1);
    p
}

fn base(name: &str, category: usize) -> Contest {
    let mut c = Contest::new_with_default_stages(None);
    c.name = name.into();
    c.organizer = "Cercle d'escrime de Yerres".into();
    c.location = "Gymnase Léo Lagrange, Yerres".into();
    c.weapon = &bp_core::weapon::WEAPONS[2];
    c.category.set_index(category);
    c.gender = 2;
    (c.day, c.month, c.year) = (18, 10, 2026);
    c
}

fn next(c: &mut Contest) {
    c.with_schedule(|s, ctx| {
        s.lock_current(ctx);
        s.enter_next(ctx);
    });
}

fn rankings(c: &Contest) -> HashMap<PlayerRef, u32> {
    c.players.iter().map(|p| (p.reference, p.attr("ranking").map(|v| v.as_uint()).unwrap_or(99))).collect()
}

/// Scores des poules : le mieux classé gagne le plus souvent ; `ratio` des matchs joués.
fn play_pools(c: &mut Contest, ratio: f64) {
    let rank = rankings(c);
    let index = (0..c.schedule.len()).find(|i| c.schedule.stage(*i).is_some_and(|s| s.kind() == StageKind::PoolSupervisor)).unwrap();
    c.with_schedule(|s, ctx| {
        let stage = s.stage_mut(index).unwrap().as_any_mut().downcast_mut::<PoolSupervisorStage>().unwrap();
        let mut round = stage.round();
        for p in round.pools_mut() {
            let n = p.matches().len();
            let played = ((n as f64) * ratio).round() as usize;
            for m in 0..played {
                let a = p.matches()[m].bout.opponent(0).unwrap();
                let b = p.matches()[m].bout.opponent(1).unwrap();
                let upset = (m * 7 + a as usize) % 5 == 0;
                let (w, l) = if (rank[&a] < rank[&b]) != upset { (a, b) } else { (b, a) };
                p.set_score(ctx.players, m, w, 5, true);
                p.set_score(ctx.players, m, l, ((m + l as usize) % 5) as u32, false);
            }
        }
        round.refresh_classification(ctx.players);
    });
}

fn save(c: &mut Contest, dir: &std::path::Path, name: &str) {
    let path = dir.join(name);
    c.save_to(&path).expect("enregistrement");
    println!("{}", path.display());
}

fn individual(dir: &std::path::Path) {
    let mut c = base("Challenge d'automne", 5);
    for i in 0..24 {
        let p = fencer(i);
        let r = c.add_fencer(p).unwrap();
        c.with_schedule(|s, ctx| {
            s.checkin_mut().unwrap().set_attending(ctx, r, true);
        });
    }
    // Arbitres de l'organisation
    c.referees.set_weapon(c.weapon);
    for (i, (name, first, club)) in [
        ("GARNIER", "Sophie", "YERRES CE"), ("CHEVALIER", "Marc", "MELUN AE"), ("PERRIN", "Julie", "BRUNOY AE"),
        ("ROBIN", "Antoine", "MONTGERON ESC"), ("CLEMENT", "Claire", "VIGNEUX CS"), ("GAUTHIER", "Pierre", "CRETEIL US"),
    ].iter().enumerate() {
        let mut r = new_player(PlayerClass::Referee);
        r.set_ref(20000 + i as u32);
        r.set_str(&AttrId::global("name"), name);
        r.set_str(&AttrId::global("first_name"), first);
        r.set_str(&AttrId::global("club"), club);
        r.set_str(&AttrId::global("weapon"), "F");
        r.set_uint(&AttrId::global("attending"), 1);
        c.referees.add(r);
    }
    next(&mut c); // appel → composition
    next(&mut c); // composition → poules
    play_pools(&mut c, 1.0);
    next(&mut c); // poules → tableau
    // Premier tour du tableau en partie joué.
    c.with_schedule(|s, ctx| {
        let i = s.current_index();
        let sup = s.stage_mut(i).unwrap().as_any_mut().downcast_mut::<TableSupervisor>().unwrap();
        let ready: Vec<(usize, PlayerRef, PlayerRef)> = sup.table_sets()[0]
            .nodes()
            .iter()
            .enumerate()
            .filter_map(|(n, node)| {
                let m = node.bout.as_ref()?;
                Some((n, m.opponent(0)?, m.opponent(1)?))
            })
            .take(6)
            .collect();
        for (k, (node, a, b)) in ready.into_iter().enumerate() {
            sup.set_score(ctx, 0, node, a, "V");
            sup.set_score(ctx, 0, node, b, &((k * 3 + 6) % 15).to_string());
        }
    });
    save(&mut c, dir, "individuel.cotcot");
}

fn youth(dir: &std::path::Path) {
    let mut c = base("Entraînement des jeunes d'octobre", 3);
    for i in 0..16 {
        let r = c.add_fencer(fencer(i)).unwrap();
        c.with_schedule(|s, ctx| {
            s.checkin_mut().unwrap().set_attending(ctx, r, true);
        });
    }
    c.with_schedule(|s, ctx| {
        s.add_kind(ctx, StageKind::YouthTraining, Some(0));
        s.lock_current(ctx);
        s.enter_next(ctx);
        let y = s.current_mut().unwrap().as_any_mut().downcast_mut::<YouthTraining>().unwrap();
        y.settings.group_mode = GroupMode::Division;
    });
    next(&mut c); // groupes → composition
    next(&mut c); // composition → poules
    play_pools(&mut c, 0.6);
    save(&mut c, dir, "jeunes.cotcot");
}

fn teams(dir: &std::path::Path) {
    let mut c = Contest::new(None, false);
    c.set_team_event(true);
    c.create_default_schedule(true);
    c.name = "Coupe des clubs par équipes".into();
    c.organizer = "Comité départemental".into();
    c.location = "Halle des sports, Melun".into();
    c.weapon = &bp_core::weapon::WEAPONS[1];
    c.category.set_index(9);
    (c.day, c.month, c.year) = (25, 10, 2026);
    c.set_team_size(3, 1);
    let teams = ["YERRES 1", "YERRES 2", "MELUN", "BRUNOY", "MONTGERON", "VIGNEUX", "CRETEIL", "SAVIGNY"];
    let mut k = 0;
    for team in teams {
        for _ in 0..4 {
            let mut p = fencer(k);
            p.set_str(&AttrId::global("team"), team);
            k += 1;
            c.with_schedule(|s, ctx| {
                s.checkin_mut().unwrap().add(ctx, p);
            });
        }
    }
    // Appel : tous les tireurs présents (les équipes le deviennent).
    let team_refs: Vec<PlayerRef> = c.players.iter().filter(|p| p.class == PlayerClass::Fencer).map(|p| p.reference).collect();
    for t in team_refs {
        c.with_schedule(|s, ctx| {
            s.checkin_mut().unwrap().set_attending(ctx, t, true);
        });
    }
    // Tableau en relais de 5 points, 3 contre 3.
    let table = (0..c.schedule.len()).find(|i| c.schedule.stage(*i).is_some_and(|s| s.kind() == StageKind::TableSupervisor)).unwrap();
    let mut timing = c.schedule.stage(table).unwrap().bout_timing();
    timing.relay = RelayFormat::default();
    timing.period_seconds = 180;
    let _ = c.set_stage_timing(table, timing);
    next(&mut c); // appel → tableau
    // Une rencontre commencée : composition et trois relais.
    let i = c.schedule.current_index();
    let names: Vec<String> = c.players.iter().filter(|p| p.class == PlayerClass::Fencer).take(8).map(|p| format!("{} {}", p.name(), p.str_attr("first_name").unwrap_or_default())).collect();
    c.with_schedule(|s, _| {
        let sup = s.stage_mut(i).unwrap().as_any_mut().downcast_mut::<TableSupervisor>().unwrap();
        let node = sup.table_sets()[0].nodes().iter().position(|n| n.bout.as_ref().is_some_and(|m| m.opponent(0).is_some() && m.opponent(1).is_some())).unwrap();
        if let Some(m) = sup.table_set_mut(0).bout_mut(node) {
            m.relay = Some(RelayLog {
                revision: 1,
                format: RelayFormat::default(),
                top: 0,
                lineup: vec![names[0].clone(), names[1].clone(), names[2].clone(), names[4].clone(), names[5].clone(), names[6].clone()],
                relays: vec![[5, 3], [8, 10], [15, 12]],
                winner: None,
            });
        }
    });
    save(&mut c, dir, "equipes.cotcot");
}

fn main() {
    let dir = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| ".".into()));
    std::fs::create_dir_all(&dir).expect("dossier");
    individual(&dir);
    youth(&dir);
    teams(&dir);
}
