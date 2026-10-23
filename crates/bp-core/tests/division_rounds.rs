//! Poules de division : répartition de départ, poules par division,
//! montées et descentes, classement final D1 puis D2.

use bp_core::contest::{players::new_player, Contest};
use bp_core::player::{AttrId, PlayerClass, PlayerRef};
use bp_core::rounds::divisions::{DivisionRound, Move};
use bp_core::rounds::pool::PoolSupervisorStage;
use bp_core::stage::StageKind;

/// 12 tireurs classés 1 à 12 ; formule : appel, division, poules,
/// division, poules, classement général.
fn ladder() -> (Contest, Vec<PlayerRef>) {
    let mut c = Contest::new(None, false);
    c.create_default_schedule(true);
    let mut refs = Vec::new();
    for i in 0..12u32 {
        let mut p = new_player(PlayerClass::Fencer);
        p.set_str(&AttrId::global("name"), &format!("TIREUR{i:02}"));
        p.set_str(&AttrId::global("club"), &format!("CLUB{}", i % 4));
        p.set_uint(&AttrId::global("ranking"), i + 1);
        let r = c.add_fencer(p).unwrap();
        c.with_schedule(|s, ctx| s.checkin_mut().unwrap().set_attending(ctx, r, true));
        refs.push(r);
    }
    c.with_schedule(|s, ctx| {
        // Les phases déjà présentes après l'appel (hors classement général) sont retirées.
        while s.len() > 2 {
            s.remove_stage(ctx, 1);
        }
        let first = s.add_kind(ctx, StageKind::DivisionRound, Some(0));
        s.add_kind(ctx, StageKind::DivisionRound, Some(first + 2));
    });
    (c, refs)
}

fn kinds(c: &Contest) -> Vec<StageKind> {
    c.schedule.stages().map(|s| s.kind()).collect()
}

fn next(c: &mut Contest) {
    c.with_schedule(|s, ctx| {
        s.lock_current(ctx);
        s.enter_next(ctx);
    });
}

fn division(c: &Contest, index: usize) -> &DivisionRound {
    c.schedule.stage(index).unwrap().as_any().downcast_ref::<DivisionRound>().unwrap()
}

#[test]
fn promotions_and_relegations_between_two_rounds() {
    let (mut c, refs) = ladder();
    assert_eq!(
        kinds(&c),
        vec![
            StageKind::Checkin,
            StageKind::DivisionRound,
            StageKind::PoolAllocator,
            StageKind::PoolSupervisor,
            StageKind::DivisionRound,
            StageKind::PoolAllocator,
            StageKind::PoolSupervisor,
            StageKind::GeneralClassification,
        ]
    );

    // Répartition de départ : les 6 meilleurs en D1.
    next(&mut c);
    let d = division(&c, 1);
    assert_eq!(d.members(1), refs[..6].to_vec());
    assert_eq!(d.members(2), refs[6..].to_vec());
    assert!(refs.iter().all(|r| d.movement(*r) == Move::New));

    // Poules : une par division, nommées D1 et D2.
    next(&mut c);
    {
        let alloc = c.schedule.stage(2).unwrap().as_any().downcast_ref::<bp_core::rounds::pool::PoolAllocatorStage>().unwrap();
        let round = alloc.round();
        let names: Vec<String> = round.pools().iter().map(|p| p.name().to_string()).collect();
        assert_eq!(names.len(), 2);
        assert!(names[0].starts_with("D1") && names[1].starts_with("D2"), "{names:?}");
        assert!(round.pools()[0].fencers().iter().all(|f| refs[..6].contains(f)));
    }
    next(&mut c);
    c.with_schedule(|s, ctx| {
        s.stage_mut(3).unwrap().as_any_mut().downcast_mut::<PoolSupervisorStage>().unwrap().stuff(ctx);
    });

    // Fin du tour : les 2 derniers de D1 descendent, les 2 premiers de D2 montent.
    let order: Vec<PlayerRef> = c.with_schedule(|s, ctx| s.stage(3).unwrap().output(ctx));
    next(&mut c);
    let d = division(&c, 4);
    let d1: Vec<PlayerRef> = order.iter().copied().filter(|r| refs[..6].contains(r)).collect();
    let d2: Vec<PlayerRef> = order.iter().copied().filter(|r| refs[6..].contains(r)).collect();
    for r in &d1[4..] {
        assert_eq!((d.division_of(*r), d.movement(*r)), (2, Move::Down));
    }
    for r in &d2[..2] {
        assert_eq!((d.division_of(*r), d.movement(*r)), (1, Move::Up));
    }
    for r in d1[..4].iter().chain(&d2[2..]) {
        assert_eq!(d.movement(*r), Move::Stays);
    }
    assert_eq!(d.members(1).len(), 6, "montées = descentes : les divisions gardent leur taille");

    // Correction à la main, puis relecture du fichier.
    c.with_schedule(|s, _| {
        s.stage_mut(4).unwrap().as_any_mut().downcast_mut::<DivisionRound>().unwrap().set_division(d1[0], 2);
    });
    let mut again = Contest::new(None, false);
    again.load_xml_str(&c.to_xml()).unwrap();
    let d = division(&again, 4);
    assert_eq!(d.division_of(d1[0]), 2);
    assert_eq!(d.movement(d1[0]), Move::Down);
    assert_eq!(d.members(1).len(), 5);

    // Deuxième tour joué : le classement général met la D1 d'abord.
    next(&mut c);
    next(&mut c);
    c.with_schedule(|s, ctx| {
        s.stage_mut(6).unwrap().as_any_mut().downcast_mut::<PoolSupervisorStage>().unwrap().stuff(ctx);
    });
    next(&mut c);
    let final_d1 = division(&c, 4).members(1);
    let ranking: Vec<PlayerRef> = c.with_schedule(|s, ctx| s.stage(7).unwrap().classification(ctx).iter().map(|e| e.player).collect());
    assert_eq!(ranking.len(), 12);
    let top: Vec<PlayerRef> = ranking[..final_d1.len()].to_vec();
    assert!(top.iter().all(|r| final_d1.contains(r)), "la D1 d'abord");
}
