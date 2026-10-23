//! Entraînement des jeunes dans une seule compétition : groupes, poules par
//! groupe, tableaux croisés par groupe, retour en arrière.

use std::collections::HashMap;

use bp_core::contest::{players::new_player, Contest};
use bp_core::player::{AttrId, PlayerClass, PlayerRef};
use bp_core::rounds::pool::PoolSupervisorStage;
use bp_core::rounds::table::supervisor::{FencedPlaces, TableSupervisor};
use bp_core::rounds::youth::{GroupMode, YouthTraining};
use bp_core::stage::StageKind;

/// 16 tireurs présents classés de 1 à 16 ; formule appel → groupes →
/// poules → tableaux → classement.
fn contest(group_mode: GroupMode) -> Contest {
    let mut c = Contest::new(None, false);
    c.create_default_schedule(false);
    for i in 0..16 {
        let mut p = new_player(PlayerClass::Fencer);
        p.set_str(&AttrId::global("name"), &format!("JEUNE{:02}", i + 1));
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

fn next(c: &mut Contest) {
    c.with_schedule(|s, ctx| {
        s.lock_current(ctx);
        s.enter_next(ctx);
    });
}

fn rankings(c: &Contest) -> HashMap<PlayerRef, u32> {
    c.players.iter().map(|p| (p.reference, p.attr("ranking").unwrap().as_uint())).collect()
}

fn pool_stage(c: &Contest) -> usize {
    (0..c.schedule.len()).find(|i| c.schedule.stage(*i).unwrap().kind() == StageKind::PoolSupervisor).unwrap()
}

#[test]
fn groups_have_their_own_pools_and_crossed_tables_in_one_contest() {
    let mut c = contest(GroupMode::Division);
    next(&mut c); // groupes → composition des poules
    let rank = rankings(&c);
    let pools: Vec<(String, Vec<u32>)> = {
        let stage = c.schedule.stage(pool_stage(&c)).unwrap().as_any().downcast_ref::<PoolSupervisorStage>().unwrap();
        let round = stage.round();
        round
            .pools()
            .iter()
            .map(|p| {
                let mut r: Vec<u32> = p.fencers().iter().map(|f| rank[f]).collect();
                r.sort();
                (p.name().to_string(), r)
            })
            .collect()
    };
    assert_eq!(pools.len(), 4);
    assert_eq!(pools[0].1, vec![1, 4, 5, 8]);
    assert_eq!(pools[1].1, vec![2, 3, 6, 7]);
    assert_eq!(pools[2].1, vec![9, 12, 13, 16]);
    assert_eq!(pools[3].1, vec![10, 11, 14, 15]);
    assert!(pools[2].0.contains('2'), "{}", pools[2].0);

    next(&mut c); // composition → poules
    let index = pool_stage(&c);
    c.with_schedule(|s, ctx| {
        let stage = s.stage_mut(index).unwrap().as_any_mut().downcast_mut::<PoolSupervisorStage>().unwrap();
        let mut round = stage.round();
        for p in round.pools_mut() {
            for m in 0..p.matches().len() {
                let a = p.matches()[m].bout.opponent(0).unwrap();
                let b = p.matches()[m].bout.opponent(1).unwrap();
                let (w, l) = if rank[&a] < rank[&b] { (a, b) } else { (b, a) };
                p.set_score(ctx.players, m, w, 5, true);
                p.set_score(ctx.players, m, l, 2, false);
            }
        }
        round.refresh_classification(ctx.players);
    });

    next(&mut c); // poules → tableaux
    let table = c.schedule.current().unwrap().as_any().downcast_ref::<TableSupervisor>().unwrap();
    assert_eq!(table.fenced_places_mode(), FencedPlaces::AllPlaces);
    let roots: Vec<_> = table.table_sets().iter().filter(|s| s.parent().is_none()).collect();
    assert_eq!(roots.len(), 2);
    for (g, set) in roots.iter().enumerate() {
        let mut pairs: Vec<(u32, u32)> = set
            .nodes()
            .iter()
            .filter_map(|n| {
                let m = n.bout.as_ref()?;
                let (a, b) = (m.opponent(0)?, m.opponent(1)?);
                let (x, y) = (rank[&a], rank[&b]);
                Some((x.min(y), x.max(y)))
            })
            .collect();
        pairs.sort();
        let o = 8 * g as u32;
        // 1er P1 / 4e P2, 2e P1 / 3e P2, 3e P1 / 2e P2, 4e P1 / 1er P2.
        assert_eq!(pairs, vec![(o + 1, o + 7), (o + 2, o + 8), (o + 3, o + 5), (o + 4, o + 6)]);
        assert_eq!(set.group(), Some(g as u32 + 1));
    }

    // Retour en arrière jusqu'à la création des groupes : tout est défait.
    c.with_schedule(|s, ctx| {
        for _ in 0..3 {
            s.go_back(ctx);
        }
    });
    assert_eq!(c.schedule.current().unwrap().kind(), StageKind::YouthTraining);
    assert!(c.players.iter().all(|p| p.attr("youth_group").is_none()));
}

#[test]
fn ranking_mode_makes_groups_of_equal_level() {
    let mut c = contest(GroupMode::Ranking);
    next(&mut c);
    let rank = rankings(&c);
    let mut group2: Vec<u32> = c
        .players
        .iter()
        .filter(|p| p.attr("youth_group").map(|v| v.as_uint()) == Some(2))
        .map(|p| rank[&p.reference])
        .collect();
    group2.sort();
    assert_eq!(group2, vec![2, 3, 6, 7, 10, 11, 14, 15]);
}

#[test]
fn locked_groups_survive_a_save_and_reload() {
    let mut c = contest(GroupMode::Division);
    next(&mut c);
    let xml = c.to_xml();
    let mut loaded = Contest::new(None, false);
    loaded.load_xml_str(&xml).unwrap();
    let stage = loaded.schedule.stage(pool_stage(&loaded)).unwrap().as_any().downcast_ref::<PoolSupervisorStage>().unwrap();
    let round = stage.round();
    assert_eq!(round.pools().len(), 4);
    assert!(round.pools()[3].name().contains('2'));
}

#[test]
fn each_group_has_its_ranking_and_points() {
    use bp_core::rounds::youth::{self, PointsMode};
    let mut c = contest(GroupMode::Division);
    c.with_schedule(|s, _| {
        let youth = s.current_mut().unwrap().as_any_mut().downcast_mut::<YouthTraining>().unwrap();
        youth.settings.points = PointsMode::ByPlace(vec![10, 8, 6, 5]);
    });
    next(&mut c); // groupes → composition
    next(&mut c); // composition → poules
    let rank = rankings(&c);
    let index = pool_stage(&c);
    c.with_schedule(|s, ctx| {
        let stage = s.stage_mut(index).unwrap().as_any_mut().downcast_mut::<PoolSupervisorStage>().unwrap();
        let mut round = stage.round();
        for p in round.pools_mut() {
            for m in 0..p.matches().len() {
                let a = p.matches()[m].bout.opponent(0).unwrap();
                let b = p.matches()[m].bout.opponent(1).unwrap();
                let (w, l) = if rank[&a] < rank[&b] { (a, b) } else { (b, a) };
                p.set_score(ctx.players, m, w, 5, true);
                p.set_score(ctx.players, m, l, 2, false);
            }
        }
        round.refresh_classification(ctx.players);
    });
    next(&mut c); // poules → tableaux
    // Le mieux classé gagne chaque match de tableau, jusqu'au bout.
    loop {
        let mut changed = false;
        c.with_schedule(|s, ctx| {
            let i = s.current_index();
            let table = s.stage_mut(i).unwrap().as_any_mut().downcast_mut::<TableSupervisor>().unwrap();
            let todo: Vec<(usize, usize, PlayerRef, PlayerRef)> = table
                .table_sets()
                .iter()
                .enumerate()
                .flat_map(|(si, set)| {
                    set.nodes().iter().enumerate().filter_map(move |(ni, n)| {
                        let m = n.bout.as_ref()?;
                        if m.winner().is_some() {
                            return None;
                        }
                        Some((si, ni, m.opponent(0)?, m.opponent(1)?))
                    })
                })
                .collect();
            for (si, ni, a, b) in todo {
                let (w, l) = if rank[&a] < rank[&b] { (a, b) } else { (b, a) };
                table.set_score(ctx, si, ni, w, "V");
                table.set_score(ctx, si, ni, l, "2");
                changed = true;
            }
        });
        if !changed {
            break;
        }
    }
    let results = youth::results(&c.schedule, &c.players).unwrap();
    let summary: Vec<(u32, Option<u32>, u32, Option<u32>)> =
        results.iter().map(|r| (r.group, r.place, rank[&r.player], r.points)).collect();
    // Chaque groupe a son propre classement : 1er, 2e… avec ses points.
    assert_eq!(summary[0], (1, Some(1), 1, Some(10)));
    assert_eq!(summary[1], (1, Some(2), 2, Some(8)));
    assert_eq!(summary[8], (2, Some(1), 9, Some(10)));
    assert_eq!(summary[15], (2, Some(8), 16, Some(5)));
    // Le premier a gagné ses 3 matchs de poule et ses 3 de tableau.
    assert_eq!(results[0].victories, 6);

    // Compétition terminée, enregistrée puis rouverte : mêmes classements.
    c.with_schedule(|s, ctx| {
        s.lock_current(ctx);
        s.enter_next(ctx);
    });
    let xml = c.to_xml();
    let mut reopened = Contest::new(None, false);
    reopened.load_xml_str(&xml).unwrap();
    let again = youth::results(&reopened.schedule, &reopened.players).unwrap();
    let names = |c: &Contest, r: &[youth::YouthResult]| -> Vec<(String, u32, Option<u32>, u32, Option<u32>)> {
        r.iter().map(|x| (c.players.get(x.player).unwrap().name(), x.group, x.place, x.victories, x.points)).collect()
    };
    assert_eq!(names(&reopened, &again), names(&c, &results));
    assert_eq!(reopened.schedule.current().unwrap().kind(), StageKind::GeneralClassification);
    // L'export HTML contient le tableau de chaque groupe.
    let html = reopened.dump_to_html();
    assert!(html.contains("Group 1 · Place #1") && html.contains("Group 2 · Place #1"));
}

#[test]
fn pools_of_a_group_on_one_piste_are_interleaved_and_numbered_once() {
    use bp_core::group_sheets::group_dispatch;
    use bp_core::player::PlayerStore;
    let mut c = contest(GroupMode::Division);
    next(&mut c); // groupes → composition des poules
    next(&mut c); // composition validée → poules (les matchs existent)
    let index = pool_stage(&c);
    let stage = c.schedule.stage(index).unwrap().as_any().downcast_ref::<PoolSupervisorStage>().unwrap();
    let shared = stage.shared_round();
    let mut round = shared.lock().unwrap();
    let group = round.pools()[0].group().unwrap();
    let mut players = c.players.clone();
    for pool in round.pools_mut().iter_mut().filter(|p| p.group() == Some(group)) {
        pool.add_referee(200);
        pool.add_referee(100);
        pool.set_roadmap_pistes(&mut players, &[1], None);
    }
    let referees = PlayerStore::new();
    let bouts = group_dispatch(&round, group, &players, &referees);
    let pools: Vec<usize> = round.pools().iter().enumerate().filter(|(_, p)| p.group() == Some(group)).map(|(i, _)| i).collect();
    let total: usize = pools.iter().map(|i| round.pools()[*i].matches().len()).sum();
    assert_eq!(bouts.len(), total);
    // M1, M2… une seule fois pour tout le groupe
    assert_eq!(bouts.iter().map(|b| b.number).collect::<Vec<_>>(), (1..=total).collect::<Vec<_>>());
    // Poule 1, poule 2, poule 1… et les arbitres alternent à chaque match
    assert_eq!(bouts[0].pool, pools[0]);
    assert_eq!(bouts[1].pool, pools[1]);
    assert_eq!(bouts[2].pool, pools[0]);
    for pair in bouts.windows(2) {
        assert_ne!(pair[0].referee, pair[1].referee);
        assert_eq!(pair[0].piste, 1);
    }
}

#[test]
fn group_sheets_are_filled_with_pool_and_table_results() {
    let mut c = contest(GroupMode::Division);
    next(&mut c); // groupes → composition
    let rank = rankings(&c);
    next(&mut c); // composition → poules
    let index = pool_stage(&c);
    // Avant tout résultat : feuilles vierges.
    let blank = c.youth_group_sheets(index).unwrap();
    let first_row = &blank["groups"][0]["pools"][0]["rows"][0];
    assert!(first_row["cells"].as_array().unwrap().iter().all(|x| x == ""));
    assert!(first_row["dashboard"].is_null());
    c.with_schedule(|s, ctx| {
        let stage = s.stage_mut(index).unwrap().as_any_mut().downcast_mut::<PoolSupervisorStage>().unwrap();
        let mut round = stage.round();
        for p in round.pools_mut() {
            for m in 0..p.matches().len() {
                let a = p.matches()[m].bout.opponent(0).unwrap();
                let b = p.matches()[m].bout.opponent(1).unwrap();
                let (w, l) = if rank[&a] < rank[&b] { (a, b) } else { (b, a) };
                p.set_score(ctx.players, m, w, 5, true);
                p.set_score(ctx.players, m, l, 2, false);
            }
        }
        round.refresh_classification(ctx.players);
    });
    // Poules remplies : scores et résultats.
    let filled = c.youth_group_sheets(index).unwrap();
    let pool = &filled["groups"][0]["pools"][0];
    assert!(pool["rows"][0]["cells"].as_array().unwrap().iter().any(|x| x != ""));
    assert!(pool["rows"][0]["dashboard"].is_array());
    assert!(pool["matches"][0]["opponents"][0]["score"].as_str().is_some_and(|s| !s.is_empty()));

    next(&mut c); // poules → tableaux
    // Premier match du tableau du groupe 1 joué.
    c.with_schedule(|s, ctx| {
        let i = s.current_index();
        let sup = s.stage_mut(i).unwrap().as_any_mut().downcast_mut::<TableSupervisor>().unwrap();
        let set = sup.table_sets().iter().position(|t| t.group() == Some(1)).unwrap();
        let node = sup.table_sets()[set].nodes().iter().position(|n| n.bout.as_ref().is_some_and(|m| m.opponent(0).is_some() && m.opponent(1).is_some())).unwrap();
        let m = sup.table_sets()[set].nodes()[node].bout.clone().unwrap();
        sup.set_score(ctx, set, node, m.opponent(0).unwrap(), "V");
        sup.set_score(ctx, set, node, m.opponent(1).unwrap(), "3");
    });
    let sheets = c.youth_group_sheets(index).unwrap();
    let items = sheets["groups"][0]["bracket"]["items"].as_array().unwrap().clone();
    let texts: Vec<String> = items.iter().filter_map(|i| i["text"].as_str().map(str::to_string)).collect();
    assert!(texts.iter().any(|t| t.starts_with("JEUNE")), "noms du vrai tableau");
    assert!(texts.iter().any(|t| t == "V"), "score du match joué");
    assert!(texts.iter().any(|t| t == "3"));
}
