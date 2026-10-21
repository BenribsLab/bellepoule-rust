//! Entraînement des jeunes : groupes, poules imposées et tableau croisé.

use bp_core::contest::{players::new_player, Contest};
use bp_core::player::{AttrId, PlayerClass, PlayerRef};
use bp_core::rounds::pool::PoolSupervisorStage;
use bp_core::rounds::table::supervisor::{FencedPlaces, TableSupervisor};
use bp_core::rounds::youth::{GroupMode, YouthTraining};
use bp_core::stage::{Stage, StageKind};
use bp_core::tournament::{user_config::UserConfig, Tournament};

fn ranking(c: &Contest, r: PlayerRef) -> u32 {
    c.players.get(r).unwrap().attr("ranking").unwrap().as_uint()
}

/// 16 tireurs présents, classés de 1 à 16, formule appel → entraînement.
fn source(group_mode: GroupMode) -> Contest {
    let mut c = Contest::new(None, false);
    c.create_default_schedule(true);
    for i in 0..16 {
        let mut p = new_player(PlayerClass::Fencer);
        p.set_str(&AttrId::global("name"), &format!("JEUNE{i:02}"));
        p.set_uint(&AttrId::global("ranking"), i + 1);
        let r = c.add_fencer(p).unwrap();
        c.with_schedule(|s, ctx| {
            s.checkin_mut().unwrap().set_attending(ctx, r, true);
        });
    }
    c.with_schedule(|s, ctx| {
        s.add_kind(ctx, StageKind::YouthTraining, Some(0));
        s.lock_current(ctx);
        s.enter_next(ctx);
        let youth = s.current_mut().unwrap().as_any_mut().downcast_mut::<YouthTraining>().unwrap();
        youth.settings.group_mode = group_mode;
    });
    c
}

#[test]
fn each_group_fences_two_pools_then_a_crossed_table_with_all_places() {
    let mut t = Tournament::new(UserConfig::new());
    let id = t.manage(source(GroupMode::Division));
    let groups = t.create_youth_groups(id).unwrap();
    assert_eq!(groups.len(), 2);

    let g = t.contest_mut(groups[0]).unwrap();
    let mut rankings: Vec<u32> = g.players.iter().map(|p| p.attr("ranking").unwrap().as_uint()).collect();
    rankings.sort();
    assert_eq!(rankings, (1..=8).collect::<Vec<_>>());

    // Appel → composition des poules : 2 poules de 4 en serpentin.
    g.with_schedule(|s, ctx| {
        s.lock_current(ctx);
        s.enter_next(ctx);
    });
    let pool_index = (0..g.schedule.len()).find(|i| g.schedule.stage(*i).unwrap().kind() == StageKind::PoolSupervisor).unwrap();
    let pools: Vec<Vec<u32>> = {
        let stage = g.schedule.stage(pool_index).unwrap().as_any().downcast_ref::<PoolSupervisorStage>().unwrap();
        let round = stage.round();
        round.pools().iter().map(|p| {
            let mut r: Vec<u32> = p.fencers().iter().map(|f| ranking(g, *f)).collect();
            r.sort();
            r
        }).collect()
    };
    assert_eq!(pools, vec![vec![1, 4, 5, 8], vec![2, 3, 6, 7]]);

    // Composition → poules ; le mieux classé gagne chaque match.
    g.with_schedule(|s, ctx| {
        s.lock_current(ctx);
        s.enter_next(ctx);
    });
    let rank_of: std::collections::HashMap<PlayerRef, u32> = g.players.iter().map(|p| (p.reference, p.attr("ranking").unwrap().as_uint())).collect();
    g.with_schedule(|s, ctx| {
        let stage = s.stage_mut(pool_index).unwrap().as_any_mut().downcast_mut::<PoolSupervisorStage>().unwrap();
        let mut round = stage.round();
        for p in round.pools_mut() {
            for m in 0..p.matches().len() {
                let a = p.matches()[m].bout.opponent(0).unwrap();
                let b = p.matches()[m].bout.opponent(1).unwrap();
                let (w, l) = if rank_of[&a] < rank_of[&b] { (a, b) } else { (b, a) };
                p.set_score(ctx.players, m, w, 5, true);
                p.set_score(ctx.players, m, l, 2, false);
            }
        }
        round.refresh_classification(ctx.players);
    });

    // Poules → tableau : 1er P1 / 4e P2, 2e P1 / 3e P2, 3e P1 / 2e P2, 4e P1 / 1er P2.
    g.with_schedule(|s, ctx| {
        s.lock_current(ctx);
        s.enter_next(ctx);
    });
    let table = g.schedule.current().unwrap().as_any().downcast_ref::<TableSupervisor>().unwrap();
    assert_eq!(table.fenced_places_mode(), FencedPlaces::AllPlaces);
    let set = &table.table_sets()[0];
    let mut pairs: Vec<(u32, u32)> = set
        .nodes()
        .iter()
        .filter_map(|n| {
            let m = n.bout.as_ref()?;
            let (a, b) = (m.opponent(0)?, m.opponent(1)?);
            let (x, y) = (rank_of[&a], rank_of[&b]);
            Some((x.min(y), x.max(y)))
        })
        .collect();
    pairs.sort();
    // P1 = 1,4,5,8 (places 1..4) ; P2 = 2,3,6,7.
    assert_eq!(pairs, vec![(1, 7), (2, 8), (3, 5), (4, 6)]);
}

#[test]
fn ranking_mode_makes_groups_of_equal_level() {
    let mut t = Tournament::new(UserConfig::new());
    let id = t.manage(source(GroupMode::Ranking));
    let groups = t.create_youth_groups(id).unwrap();
    let g = t.contest(groups[1]).unwrap();
    let mut names: Vec<String> = g.players.iter().map(|p| p.name()).collect();
    names.sort();
    // Serpentin : 2, 3, 6, 7, 10, 11, 14, 15.
    assert_eq!(names, ["JEUNE01", "JEUNE02", "JEUNE05", "JEUNE06", "JEUNE09", "JEUNE10", "JEUNE13", "JEUNE14"]);
}
