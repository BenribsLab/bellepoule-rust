//! Répartition des matchs sur plusieurs pistes, indépendante des scores.
use bp_core::{
    contest::players::new_player,
    player::{AttrId, Owner, PlayerClass, PlayerStore},
    rounds::pool::pool::Pool,
    xml::XmlWriter,
};

fn pool(n: usize) -> (Pool, PlayerStore) {
    let mut players = PlayerStore::new();
    let mut pool = Pool::new(5, 0, 1, "Tireur", 42, Owner::Stage(1));
    for i in 0..n {
        let mut player = new_player(PlayerClass::Fencer);
        player.set_str(&AttrId::global("name"), &format!("TIREUR {i}"));
        player.set_uint(&AttrId::global("attending"), 1);
        let r = players.add(player);
        pool.add_fencer(&mut players, r, Some(i + 1));
    }
    pool.create_matches(&players, None);
    (pool, players)
}

#[test]
fn every_bout_has_one_piste_and_a_fixed_referee_without_parallel_fencers() {
    for n in [3, 4, 6, 7, 8] {
        for count in 1..=4 {
            let (mut pool, mut players) = pool(n);
            for i in (0..count).rev() {
                pool.add_referee(100 + i);
            }
            let pistes: Vec<_> = (1..=count).collect();
            pool.set_roadmap_pistes(&mut players, &pistes, None);
            let plan = pool.match_assignments();
            assert_eq!(plan.len(), n * (n - 1) / 2);
            let mut bouts = std::collections::BTreeSet::new();
            let mut fencers = std::collections::BTreeSet::new();
            let mut resources = std::collections::BTreeSet::new();
            let mut loads = vec![0; count as usize];
            for assignment in &plan {
                assert!(bouts.insert(assignment.match_index));
                assert!(resources.insert((assignment.wave, assignment.piste)));
                assert_eq!(assignment.referee, Some(99 + assignment.piste));
                for k in 0..2 {
                    let player = pool.matches()[assignment.match_index]
                        .bout
                        .opponent(k)
                        .unwrap();
                    assert!(fencers.insert((assignment.wave, player)));
                }
                loads[assignment.piste as usize - 1] += 1;
            }
            assert!(loads.iter().max().unwrap() - loads.iter().min().unwrap() <= 1);
            assert_eq!(pool.match_assignments(), plan, "répartition reproductible");
            let fencer = pool.matches()[0].bout.opponent(0).unwrap();
            pool.set_score_str(&mut players, 0, fencer, "V");
            assert_eq!(
                pool.match_assignments(),
                plan,
                "saisir les scores ne réaffecte pas les matchs"
            );
        }
    }
}

#[test]
fn pistes_and_their_referees_survive_xml_and_printing_keeps_match_indices() {
    let (mut pool, mut players) = pool(6);
    let mut referees = PlayerStore::new();
    for (id, name) in [(100, "ARBITRE A"), (200, "ARBITRE B")] {
        let mut player = new_player(PlayerClass::Referee);
        player.set_ref(id);
        player.set_str(&AttrId::global("name"), name);
        referees.add(player);
    }
    pool.add_referee(200);
    pool.add_referee(100);
    pool.set_roadmap_pistes(&mut players, &[3, 12], None);
    let original = pool.match_assignments();
    let mut writer = XmlWriter::new();
    pool.save(&players, &mut writer);
    let xml = writer.finish();
    let doc = bp_core::xml::parse(&xml).unwrap();
    let (mut loaded, _) = self::pool(6);
    loaded.load_header(doc.root_element());
    for r in pool.referees().iter().rev() {
        loaded.add_referee(*r);
    }
    assert_eq!(loaded.strips(), vec![3, 12]);
    assert_eq!(loaded.piste_referees(), &[(3, 100), (12, 200)]);
    assert_eq!(loaded.match_assignments(), original);
    let sheet = loaded.sheet(&players, &referees, true);
    for m in &sheet.matches {
        let assignment = m.assignment.as_ref().unwrap();
        assert_eq!(m.match_index, assignment.match_index);
        assert!(m
            .resource
            .as_ref()
            .unwrap()
            .contains(if assignment.piste == 3 {
                "ARBITRE A"
            } else {
                "ARBITRE B"
            }));
        assert_eq!(
            m.opponents[0].as_ref().unwrap().player,
            loaded.matches()[m.match_index].bout.opponent(0).unwrap()
        );
    }
}

#[test]
fn several_referees_on_one_piste_alternate_bout_after_bout() {
    let (mut pool, mut players) = pool(5);
    pool.add_referee(200);
    pool.add_referee(100);
    pool.set_roadmap_pistes(&mut players, &[4], None);
    let plan = pool.match_assignments();
    let referees: Vec<_> = plan.iter().map(|a| a.referee.unwrap()).collect();
    for pair in referees.windows(2) {
        assert_ne!(pair[0], pair[1], "les arbitres alternent : {referees:?}");
    }
}

#[test]
fn extra_referees_are_shared_between_pistes() {
    let (mut pool, mut players) = pool(6);
    for r in [400, 300, 200, 100] {
        pool.add_referee(r);
    }
    pool.set_roadmap_pistes(&mut players, &[1, 2], None);
    let by_piste = pool.referees_by_piste();
    assert_eq!(by_piste.len(), 2);
    assert!(by_piste.iter().all(|(_, refs)| refs.len() == 2));
    for a in pool.match_assignments() {
        let (_, refs) = by_piste.iter().find(|(p, _)| *p == a.piste).unwrap();
        assert!(refs.contains(&a.referee.unwrap()));
    }
}

#[test]
fn a_referee_avoids_fencers_of_his_club_when_another_is_free() {
    let (mut pool, mut players) = pool(4);
    // Le premier tireur est du club de l'arbitre 100.
    let first = pool.matches()[0].bout.opponent(0).unwrap();
    players.get_mut(first).unwrap().set_str(&AttrId::global("club"), "YERRES");
    let mut referees = PlayerStore::new();
    for (id, club) in [(100, "YERRES"), (200, "MELUN")] {
        let mut player = new_player(PlayerClass::Referee);
        player.set_ref(id);
        player.set_str(&AttrId::global("club"), club);
        referees.add(player);
    }
    pool.add_referee(200);
    pool.add_referee(100);
    pool.set_roadmap_pistes(&mut players, &[1], None);
    for a in pool.match_assignments_with_clubs(&players, &referees) {
        let m = &pool.matches()[a.match_index].bout;
        if (0..2).any(|k| m.opponent(k) == Some(first)) {
            assert_eq!(a.referee, Some(200));
        }
    }
}
