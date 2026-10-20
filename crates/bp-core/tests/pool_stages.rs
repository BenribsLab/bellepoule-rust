//! Tour de poules : chargement de vrais fichiers et déroulé complet d'une
//! compétition créée de zéro.

use std::path::PathBuf;

use bp_core::contest::players::new_player;
use bp_core::contest::Contest;
use bp_core::player::{AttrId, PlayerClass};
use bp_core::rounds::pool::{PoolAllocatorStage, PoolSupervisorStage};
use bp_core::rounds::table::TableSupervisor;
use rand::SeedableRng;
use bp_core::stage::StageKind;

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
}
