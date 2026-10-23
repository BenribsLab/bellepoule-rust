//! Tour de poules : chargement de vrais fichiers et déroulé complet d'une
//! compétition créée de zéro.

use std::path::PathBuf;

use bp_core::contest::players::new_player;
use bp_core::contest::Contest;
use bp_core::player::{AttrId, PlayerClass};
use bp_core::rounds::pool::{PoolAllocatorStage, PoolSupervisorStage};
use bp_core::rounds::table::TableSupervisor;
use rand::SeedableRng;
use bp_core::stage::{Stage, StageKind};

fn data(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests-data").join(name)
}

fn kinds(c: &Contest) -> Vec<StageKind> {
    c.schedule.stages().map(|s| s.kind()).collect()
}

#[test]
fn loads_pools_from_real_files() {
    for (file, nb_pools) in [("melun.cotcot", 13usize), ("engarde.cotcot", 0)] {
        let mut c = Contest::new(None, false);
        c.load_file(&data(file)).unwrap_or_else(|e| panic!("{file} : {e}"));
        let k = kinds(&c);
        assert!(k.contains(&StageKind::PoolAllocator) && k.contains(&StageKind::PoolSupervisor), "{file} : {k:?}");
        let alloc = c.schedule.stages().find_map(|s| s.as_any().downcast_ref::<PoolAllocatorStage>()).unwrap();
        let round = alloc.round();
        if nb_pools > 0 {
            assert_eq!(round.nb_pools(), nb_pools, "{file}");
            let in_pools: usize = round.pools().iter().map(|p| p.nb_players()).sum();
            assert!(in_pools > 0, "{file} : poules vides");
            assert!(round.pools().iter().any(|p| p.has_matches()), "{file} : aucun match chargé");
        }
    }
}

/// Un ancien fichier est normalisé (renumérotation des tireurs) à la première
/// lecture ; ensuite l'enregistrement est stable.
#[test]
fn saving_pools_is_stable_once_normalized() {
    let mut a = Contest::new(None, false);
    a.load_file(&data("melun.cotcot")).unwrap();
    let normalized = a.to_xml();
    let mut b = Contest::new(None, false);
    b.load_xml_str(&normalized).unwrap();
    let second = b.to_xml();
    let mut c = Contest::new(None, false);
    c.load_xml_str(&second).unwrap();
    assert_eq!(second, c.to_xml(), "l'enregistrement des poules n'est pas stable");
    // Mêmes poules et mêmes matchs de poules (les tableaux sont vérifiés à part).
    let before_tables = |x: &str| x[..x.find("<PhaseDeTableaux").unwrap_or(x.len())].to_string();
    let pools = |x: &str| before_tables(x).matches("<Poule ").count();
    let matches = |x: &str| before_tables(x).matches("<Match ").count();
    assert_eq!(pools(&normalized), 13 + 17 + 16);
    assert_eq!(pools(&normalized), pools(&second));
    assert_eq!(matches(&normalized), 852, "matchs de poules du fichier d'origine");
    assert_eq!(matches(&normalized), matches(&second));
}

/// Compétition de 20 tireurs : du pointage jusqu'au classement du tour.
#[test]
fn full_pool_round() {
    let mut c = Contest::new_with_default_stages(None);
    for i in 0..20u32 {
        let mut p = new_player(PlayerClass::Fencer);
        p.set_str(&AttrId::global("name"), &format!("TIREUR{i:02}"));
        p.set_uint(&AttrId::global("ranking"), i + 1);
        let r = c.add_fencer(p).unwrap();
        c.with_schedule(|s, ctx| {
            s.checkin_mut().unwrap().set_attending(ctx, r, true);
        });
    }
    assert_eq!(
        kinds(&c),
        vec![
            StageKind::Checkin,
            StageKind::PoolAllocator,
            StageKind::PoolSupervisor,
            StageKind::TableSupervisor,
            StageKind::GeneralClassification
        ]
    );

    // Pointage → composition des poules
    c.with_schedule(|s, ctx| {
        s.lock_current(ctx);
        s.enter_next(ctx);
    });
    assert_eq!(c.schedule.current_index(), 1);
    let nb_pools = {
        let alloc = c.schedule.stage(1).unwrap().as_any().downcast_ref::<PoolAllocatorStage>().unwrap();
        let round = alloc.round();
        assert!(round.nb_pools() >= 2, "20 tireurs doivent donner plusieurs poules");
        let total: usize = round.pools().iter().map(|p| p.nb_players()).sum();
        assert_eq!(total, 20);
        round.nb_pools()
    };
    let over = c.with_schedule(|s, ctx| s.sensitivity(ctx).next);
    assert!(over, "la composition équilibrée doit permettre de continuer");

    // Composition → saisie des scores
    c.with_schedule(|s, ctx| {
        s.lock_current(ctx);
        s.enter_next(ctx);
    });
    assert_eq!(c.schedule.current_index(), 2);
    let not_over = c.with_schedule(|s, ctx| s.sensitivity(ctx).next);
    assert!(!not_over, "aucun score saisi : le tour n'est pas terminé");

    c.with_schedule(|s, ctx| {
        let sup = s.stage_mut(2).unwrap().as_any_mut().downcast_mut::<PoolSupervisorStage>().unwrap();
        sup.stuff(ctx);
    });
    let over = c.with_schedule(|s, ctx| s.sensitivity(ctx).next);
    assert!(over, "les scores aléatoires doivent terminer le tour");

    // Verrouillage : classement et qualifiés
    let exceedance = c.with_schedule(|s, ctx| s.lock_current(ctx));
    let (classified, qualified) = c.with_schedule(|s, ctx| {
        let st = s.stage(2).unwrap();
        (st.classification(ctx).len(), st.output(ctx).len())
    });
    assert_eq!(classified, 20);
    assert!(qualified > 0 && qualified <= 20);
    assert_eq!(exceedance, 0);

    // Les rangs sont une suite croissante, sans trou au départ.
    let ranks: Vec<u32> = c.with_schedule(|s, ctx| s.stage(2).unwrap().classification(ctx).iter().map(|e| e.rank).collect());
    assert_eq!(ranks[0], 1);
    assert!(ranks.windows(2).all(|w| w[0] <= w[1]));

    // Enregistrement et relecture au milieu de la compétition
    c.with_schedule(|s, ctx| s.enter_next(ctx));
    let xml = c.to_xml();
    let mut again = Contest::new(None, false);
    again.load_xml_str(&xml).unwrap();
    assert_eq!(kinds(&c), kinds(&again));
    assert_eq!(xml, again.to_xml(), "relecture instable");
    assert!(xml.contains(&format!("NbDePoules=\"{nb_pools}\"")));
}

#[test]
fn going_back_unlocks_the_previous_stage() {
    let mut c = Contest::new_with_default_stages(None);
    for i in 0..12u32 {
        let mut p = new_player(PlayerClass::Fencer);
        p.set_str(&AttrId::global("name"), &format!("T{i:02}"));
        let r = c.add_fencer(p).unwrap();
        c.with_schedule(|s, ctx| {
            s.checkin_mut().unwrap().set_attending(ctx, r, true);
        });
    }
    c.with_schedule(|s, ctx| {
        s.lock_current(ctx);
        s.enter_next(ctx);
        s.lock_current(ctx);
        s.enter_next(ctx);
        assert_eq!(s.current_index(), 2);
        s.go_back(ctx);
        assert_eq!(s.current_index(), 1);
        assert!(!s.stage(1).unwrap().base().locked, "l'allocateur doit être déverrouillé");
    });
    let alloc = c.schedule.stage(1).unwrap().as_any().downcast_ref::<PoolAllocatorStage>().unwrap();
    assert!(alloc.round().pools().iter().all(|p| !p.has_matches()), "les matchs doivent être supprimés");
}

/// Compétition complète : pointage, poules, tableaux, classement général.
#[test]
fn whole_competition_gives_a_final_ranking() {
    let mut c = Contest::new_with_default_stages(None);
    let n = 24u32;
    for i in 0..n {
        let mut p = new_player(PlayerClass::Fencer);
        p.set_str(&AttrId::global("name"), &format!("TIREUR{i:02}"));
        p.set_uint(&AttrId::global("ranking"), i + 1);
        let r = c.add_fencer(p).unwrap();
        c.with_schedule(|s, ctx| {
            s.checkin_mut().unwrap().set_attending(ctx, r, true);
        });
    }
    // pointage → composition → poules
    for _ in 0..2 {
        c.with_schedule(|s, ctx| {
            s.lock_current(ctx);
            s.enter_next(ctx);
        });
    }
    c.with_schedule(|s, ctx| {
        s.stage_mut(2).unwrap().as_any_mut().downcast_mut::<PoolSupervisorStage>().unwrap().stuff(ctx);
        assert!(s.sensitivity(ctx).next);
        s.lock_current(ctx);
        s.enter_next(ctx);
    });
    assert_eq!(c.schedule.current_index(), 3, "on doit être aux tableaux");

    // tableaux : tous les matchs sont tirés au sort
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(7);
    c.with_schedule(|s, ctx| {
        let t = s.stage_mut(3).unwrap().as_any_mut().downcast_mut::<TableSupervisor>().unwrap();
        let set = t.visible_table_sets()[0];
        t.select_table_set(Some(set));
        // Un tour du tableau est joué à chaque appel : on passe au suivant ensuite.
        for _ in 0..10 {
            t.stuff(ctx, &mut rng);
            t.display_next(set);
        }
    });
    let over = c.with_schedule(|s, ctx| s.sensitivity(ctx).next);
    assert!(over, "le tableau doit être terminé");
    c.with_schedule(|s, ctx| {
        s.lock_current(ctx);
        s.enter_next(ctx);
    });
    assert_eq!(c.schedule.current_index(), 4, "on doit être au classement général");
    assert!(c.schedule.is_over());

    // Classement final : chaque présent a un rang, le premier est unique.
    let ranks: Vec<u32> = c.with_schedule(|s, ctx| s.stage(4).unwrap().classification(ctx).iter().map(|e| e.rank).collect());
    assert_eq!(ranks.len(), n as usize);
    assert_eq!(ranks[0], 1);
    assert!(ranks.windows(2).all(|w| w[0] <= w[1]), "classement trié : {ranks:?}");
    assert!(ranks.iter().all(|r| *r >= 1 && *r <= n), "rangs hors bornes : {ranks:?}");
    assert_eq!(ranks.iter().filter(|r| **r == 1).count(), 1, "un seul vainqueur");

    // Enregistrer puis rouvrir donne la même compétition, le même classement.
    let names = |c: &mut Contest| -> Vec<(String, u32)> {
        c.with_schedule(|s, ctx| {
            let st = s.stage(4).unwrap();
            st.classification(ctx)
                .iter()
                .map(|e| (ctx.players.get(e.player).unwrap().name(), e.rank))
                .collect()
        })
    };
    let before = names(&mut c);
    let xml = c.to_xml();
    let mut again = Contest::new(None, false);
    again.load_xml_str(&xml).unwrap();
    assert_eq!(names(&mut again), before, "le classement final change après relecture");
    assert_eq!(xml, again.to_xml(), "relecture instable");
    let tables = &xml[xml.find("<PhaseDeTableaux").unwrap()..];
    assert!(tables.contains("<SuiteDeTableaux"), "les tableaux doivent être enregistrés");
}

/// Un tableau à moitié joué garde ses scores à la relecture.
#[test]
fn half_played_table_survives_reload() {
    let mut c = Contest::new_with_default_stages(None);
    for i in 0..20u32 {
        let mut p = new_player(PlayerClass::Fencer);
        p.set_str(&AttrId::global("name"), &format!("TIREUR{i:02}"));
        p.set_uint(&AttrId::global("ranking"), i + 1);
        let r = c.add_fencer(p).unwrap();
        c.with_schedule(|s, ctx| {
            s.checkin_mut().unwrap().set_attending(ctx, r, true);
        });
    }
    for _ in 0..2 {
        c.with_schedule(|s, ctx| {
            s.lock_current(ctx);
            s.enter_next(ctx);
        });
    }
    c.with_schedule(|s, ctx| {
        s.stage_mut(2).unwrap().as_any_mut().downcast_mut::<PoolSupervisorStage>().unwrap().stuff(ctx);
        s.lock_current(ctx);
        s.enter_next(ctx);
    });
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(3);
    c.with_schedule(|s, ctx| {
        let t = s.stage_mut(3).unwrap().as_any_mut().downcast_mut::<TableSupervisor>().unwrap();
        let set = t.visible_table_sets()[0];
        t.select_table_set(Some(set));
        t.stuff(ctx, &mut rng); // un seul tour
    });

    // Résumé : pour chaque match, les deux tireurs et leurs scores.
    let summary = |c: &Contest| -> Vec<String> {
        let t = c.schedule.stage(3).unwrap().as_any().downcast_ref::<TableSupervisor>().unwrap();
        let set = t.table_set(0);
        set.nodes()
            .iter()
            .filter_map(|n| n.bout.as_ref())
            .map(|b| {
                format!(
                    "{}:{:?}/{:?}:{}-{}",
                    b.number,
                    b.opponents[0].fencer,
                    b.opponents[1].fencer,
                    b.opponents[0].score.image(10),
                    b.opponents[1].score.image(10)
                )
            })
            .collect()
    };
    let before = summary(&c);
    assert!(before.iter().any(|m| !m.ends_with(":-")), "des scores doivent être saisis");
    let xml = c.to_xml();
    let mut again = Contest::new(None, false);
    again.load_xml_str(&xml).unwrap();
    assert_eq!(summary(&again), before, "les scores du tableau changent après relecture");
    assert_eq!(xml, again.to_xml());
}

/// Joue la compétition avec une option de « places tirées » et renvoie les rangs finaux.
fn play_with_fenced_places(mode: bp_core::rounds::table::FencedPlaces, fencers: u32) -> Vec<u32> {
    let mut c = Contest::new_with_default_stages(None);
    for i in 0..fencers {
        let mut p = new_player(PlayerClass::Fencer);
        p.set_str(&AttrId::global("name"), &format!("TIREUR{i:02}"));
        p.set_uint(&AttrId::global("ranking"), i + 1);
        let r = c.add_fencer(p).unwrap();
        c.with_schedule(|s, ctx| {
            s.checkin_mut().unwrap().set_attending(ctx, r, true);
        });
    }
    c.with_schedule(|s, _ctx| {
        s.stage_mut(3).unwrap().as_any_mut().downcast_mut::<TableSupervisor>().unwrap().set_fenced_places(mode);
    });
    for _ in 0..2 {
        c.with_schedule(|s, ctx| {
            s.lock_current(ctx);
            s.enter_next(ctx);
        });
    }
    c.with_schedule(|s, ctx| {
        s.stage_mut(2).unwrap().as_any_mut().downcast_mut::<PoolSupervisorStage>().unwrap().stuff(ctx);
        s.lock_current(ctx);
        s.enter_next(ctx);
    });
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(11);
    // Les suites des battus se remplissent au fil des tours : plusieurs passes.
    for _ in 0..6 {
        c.with_schedule(|s, ctx| {
            let t = s.stage_mut(3).unwrap().as_any_mut().downcast_mut::<TableSupervisor>().unwrap();
            for set in t.visible_table_sets() {
                t.select_table_set(Some(set));
                for _ in 0..10 {
                    t.stuff(ctx, &mut rng);
                    t.display_next(set);
                }
            }
        });
    }
    assert!(c.with_schedule(|s, ctx| s.sensitivity(ctx).next), "{mode:?} : tableaux non terminés");
    c.with_schedule(|s, ctx| {
        s.lock_current(ctx);
        s.enter_next(ctx);
    });
    c.with_schedule(|s, ctx| s.stage(4).unwrap().classification(ctx).iter().map(|e| e.rank).collect())
}

#[test]
fn every_fenced_places_option_gives_a_final_ranking() {
    use bp_core::rounds::table::FencedPlaces::*;
    for n in [8u32, 13, 20, 33] {
        for mode in [None, ThirdPlaces, AllPlaces] {
            let ranks = play_with_fenced_places(mode, n);
            assert_eq!(ranks.len(), n as usize, "{mode:?} {n}");
            assert_eq!(ranks[0], 1, "{mode:?} {n}");
            assert!(ranks.windows(2).all(|w| w[0] <= w[1]), "{mode:?} {n} : {ranks:?}");
            assert_eq!(ranks.iter().filter(|r| **r == 1).count(), 1, "{mode:?} {n} : {ranks:?}");
            if mode == AllPlaces {
                // Toutes les places sont tirées : chaque tireur a un rang distinct.
                let mut d = ranks.clone();
                d.dedup();
                assert_eq!(d.len(), ranks.len(), "{mode:?} {n} : rangs identiques {ranks:?}");
            }
        }
    }
}

/// Quota dépassé par des ex æquo : un barrage départage, puis la compétition continue.
#[test]
fn barrage_breaks_ties_at_the_quota() {
    use bp_core::rounds::barrage::Barrage;
    let n = 16u32;
    let mut found = None;
    'search: for quota in (2..n).rev() {
        let mut c = Contest::new_with_default_stages(None);
        for i in 0..n {
            let mut p = new_player(PlayerClass::Fencer);
            p.set_str(&AttrId::global("name"), &format!("TIREUR{i:02}"));
            p.set_uint(&AttrId::global("ranking"), i + 1);
            let r = c.add_fencer(p).unwrap();
            c.with_schedule(|s, ctx| {
                s.checkin_mut().unwrap().set_attending(ctx, r, true);
            });
        }
        for _ in 0..2 {
            c.with_schedule(|s, ctx| {
                s.lock_current(ctx);
                s.enter_next(ctx);
            });
        }
        c.with_schedule(|s, ctx| {
            let sup = s.stage_mut(2).unwrap().as_any_mut().downcast_mut::<PoolSupervisorStage>().unwrap();
            sup.round().set_nb_qualified(Some(quota));
            sup.stuff(ctx);
        });
        let exceedance = c.with_schedule(|s, ctx| s.lock_current(ctx));
        if exceedance > 0 {
            found = Some((c, exceedance));
            break 'search;
        }
    }
    let (mut c, exceedance) = found.expect("aucun quota ne provoque d'ex æquo avec ces scores");

    c.with_schedule(|s, ctx| {
        s.add_barrage(ctx);
        s.enter_next(ctx);
    });
    let kinds = kinds(&c);
    assert_eq!(kinds[3], StageKind::Barrage, "{kinds:?}");
    let ties = c.schedule.stage(3).unwrap().as_any().downcast_ref::<Barrage>().unwrap().ties_count();
    assert_eq!(ties, exceedance, "le barrage doit porter sur les ex æquo");
    {
        let b = c.schedule.stage(3).unwrap().as_any().downcast_ref::<Barrage>().unwrap();
        assert_eq!(b.max_score(), 5, "barrage en 5 touches après les poules");
        assert!(!b.matches().is_empty(), "matchs de barrage créés");
        assert!(b.matches().iter().all(|m| m.max_score == 5), "{:?}", b.matches().iter().map(|m| m.max_score).collect::<Vec<_>>());
    }
    {
        let xml = c.to_xml();
        let mut r = Contest::new(None, false);
        r.load_xml_str(&xml).unwrap();
        let b = r.schedule.stage(3).unwrap().as_any().downcast_ref::<Barrage>().unwrap();
        assert_eq!(b.max_score(), 5, "barrage rechargé : 5 touches");
        assert!(b.matches().iter().all(|m| m.max_score == 5), "{:?}", b.matches().iter().map(|m| m.max_score).collect::<Vec<_>>());
        assert_eq!(b.matches().len(), c.schedule.stage(3).unwrap().as_any().downcast_ref::<Barrage>().unwrap().matches().len());
    }
    // Promouvoir le premier du barrage seulement.
    c.with_schedule(|s, ctx| {
        let b = s.stage_mut(3).unwrap().as_any_mut().downcast_mut::<Barrage>().unwrap();
        let players: Vec<_> = b.barrage_players().to_vec();
        for (i, r) in players.iter().enumerate() {
            b.set_promoted(ctx, *r, i == 0);
        }
    });
    // Un seul promu : le quota est respecté exactement.
    let b = c.schedule.stage(3).unwrap().as_any().downcast_ref::<Barrage>().unwrap();
    assert_eq!(b.promoted_count(&c.players), 1);
    // Le barrage disparaît si l'on revient en arrière.
    c.with_schedule(|s, ctx| s.go_back(ctx));
    assert!(!kinds_of(&c).contains(&StageKind::Barrage));
}

fn kinds_of(c: &Contest) -> Vec<StageKind> {
    kinds(c)
}

/// Point de scission : les tireurs exportés ne continuent pas dans la phase suivante.
#[test]
fn splitting_removes_exported_fencers() {
    use bp_core::rounds::splitting::Splitting;
    let n = 16u32;
    let mut c = Contest::new_with_default_stages(None);
    for i in 0..n {
        let mut p = new_player(PlayerClass::Fencer);
        p.set_str(&AttrId::global("name"), &format!("TIREUR{i:02}"));
        p.set_uint(&AttrId::global("ranking"), i + 1);
        let r = c.add_fencer(p).unwrap();
        c.with_schedule(|s, ctx| {
            s.checkin_mut().unwrap().set_attending(ctx, r, true);
        });
    }
    for _ in 0..2 {
        c.with_schedule(|s, ctx| {
            s.lock_current(ctx);
            s.enter_next(ctx);
        });
    }
    c.with_schedule(|s, ctx| {
        s.stage_mut(2).unwrap().as_any_mut().downcast_mut::<PoolSupervisorStage>().unwrap().stuff(ctx);
        s.lock_current(ctx);
        // Point de scission inséré après le tour de poules.
        let i = s.add_kind(ctx, StageKind::Splitting, Some(2));
        assert_eq!(i, 3);
        s.enter_next(ctx);
    });
    assert_eq!(kinds(&c)[3], StageKind::Splitting);

    let (exported, remaining) = c.with_schedule(|s, ctx| {
        let sp = s.stage_mut(3).unwrap().as_any_mut().downcast_mut::<Splitting>().unwrap();
        let first_two: Vec<_> = sp.base().attendees.iter().take(2).copied().collect();
        for r in &first_two {
            sp.set_exported(ctx, *r, true);
        }
        (first_two.len(), sp.remaining(ctx.players).len())
    });
    assert_eq!(exported, 2);
    assert_eq!(remaining, n as usize - 2);
    c.with_schedule(|s, ctx| {
        s.lock_current(ctx);
        s.enter_next(ctx);
    });
    let tables_attendees = c.schedule.stage(4).unwrap().base().attendees.len();
    assert_eq!(tables_attendees, n as usize - 2, "les exportés ne passent pas aux tableaux");
}

#[test]
fn double_defeat_ends_a_pool_bout_without_a_victory() {
    let mut c = Contest::new_with_default_stages(None);
    for i in 0..5u32 {
        let mut p = new_player(PlayerClass::Fencer);
        p.set_str(&AttrId::global("name"), &format!("TIREUR{i:02}"));
        p.set_uint(&AttrId::global("ranking"), i + 1);
        let r = c.add_fencer(p).unwrap();
        c.with_schedule(|s, ctx| s.checkin_mut().unwrap().set_attending(ctx, r, true));
    }
    for _ in 0..2 {
        c.with_schedule(|s, ctx| {
            s.lock_current(ctx);
            s.enter_next(ctx);
        });
    }
    // Tous les matchs joués au hasard, puis le premier en double défaite 2-2.
    let (a, b) = c.with_schedule(|s, ctx| {
        let sup = s.stage_mut(2).unwrap().as_any_mut().downcast_mut::<PoolSupervisorStage>().unwrap();
        sup.stuff(ctx);
        let mut round = sup.round();
        let pool = &mut round.pools_mut()[0];
        let bout = &pool.matches()[0].bout;
        let (a, b) = (bout.opponent(0).unwrap(), bout.opponent(1).unwrap());
        pool.set_score_str(ctx.players, 0, a, "2");
        pool.set_score_str(ctx.players, 0, b, "2");
        (a, b)
    });
    let check = |c: &Contest| {
        let sup = c.schedule.stage(2).unwrap().as_any().downcast_ref::<PoolSupervisorStage>().unwrap();
        let round = sup.round();
        let bout = &round.pools()[0].matches()[0].bout;
        assert!(bout.is_double_defeat() && bout.is_over());
        assert_eq!(bout.winner(), None);
        assert_eq!((bout.opponent(0), bout.opponent(1)), (Some(a), Some(b)));
    };
    check(&c);
    let over = c.with_schedule(|s, ctx| s.sensitivity(ctx).next);
    assert!(over, "une double défaite termine le match : la poule est finie");
    let mut again = Contest::new(None, false);
    again.load_xml_str(&c.to_xml()).unwrap();
    check(&again);
}
