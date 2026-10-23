//! Entraînement des jeunes : placement d'un groupe entier et arbitres du
//! même club que les tireurs.
use bp_core::{
    contest::{players::new_player, Contest},
    player::{AttrId, PlayerClass},
    rounds::youth::{GroupMode, YouthTraining},
    stage::StageKind,
};
use bp_net::message::Message;
use marshaller::hall::Hall;

fn send(c: &Contest, hall: &mut Hall) {
    for parcel in c.parcels() {
        let mut m = Message::new(parcel.name);
        m.set_net_id(parcel.net_id);
        for (key, value) in parcel.fields {
            m.set_str(&key, &value);
        }
        m.set_fitness(1);
        hall.on_message(&m);
    }
}

fn youth() -> (Contest, Hall, Vec<u32>) {
    let mut c = Contest::new_with_default_stages(None);
    for i in 0..16 {
        let mut p = new_player(PlayerClass::Fencer);
        p.set_str(&AttrId::global("name"), &format!("JEUNE{:02}", i + 1));
        p.set_str(&AttrId::global("club"), if i == 0 { "YERRES" } else { "MELUN" });
        p.set_uint(&AttrId::global("ranking"), i + 1);
        let r = c.add_fencer(p).unwrap();
        c.with_schedule(|s, ctx| {
            s.checkin_mut().unwrap().set_attending(ctx, r, true);
        });
    }
    let mut referees = Vec::new();
    for (i, club) in ["YERRES", "BRUNOY", "MONTGERON"].iter().enumerate() {
        let mut p = new_player(PlayerClass::Referee);
        p.set_ref(10000 + i as u32);
        p.set_str(&AttrId::global("name"), &format!("ARBITRE {i}"));
        p.set_str(&AttrId::global("club"), club);
        p.set_uint(&AttrId::global("attending"), 1);
        referees.push(c.referees.add(p));
    }
    c.with_schedule(|s, ctx| {
        s.add_kind(ctx, StageKind::YouthTraining, Some(0));
        s.lock_current(ctx);
        s.enter_next(ctx);
        let youth = s.current_mut().unwrap().as_any_mut().downcast_mut::<YouthTraining>().unwrap();
        youth.settings.group_mode = GroupMode::Division;
        s.lock_current(ctx);
        s.enter_next(ctx);
    });
    let mut hall = Hall::new(None);
    hall.default_layout();
    send(&c, &mut hall);
    for r in &referees {
        hall.set_referee_attending(*r, true);
    }
    hall.take_outbox();
    (c, hall, referees)
}

fn batch_of(hall: &Hall) -> (u32, u32) {
    let j = hall.jobs.values().find(|j| j.is_pool).unwrap();
    (j.competition, j.batch)
}

#[test]
fn a_group_on_one_piste_shares_one_slot_with_its_referees() {
    let (_, mut hall, referees) = youth();
    let (c, b) = batch_of(&hall);
    let groups = hall.batch_groups(c, b);
    assert!(!groups.is_empty(), "les poules connaissent leur groupe");
    let pools = hall.group_pools(c, b, groups[0]);
    assert!(pools.len() >= 2);
    let piste = hall.pistes[0].key;
    hall.place_group(c, b, groups[0], &[piste], &referees[1..]).unwrap();
    let slots: Vec<_> = pools.iter().map(|j| hall.jobs[j].slot.unwrap()).collect();
    assert!(slots.iter().all(|s| *s == slots[0]), "un seul créneau pour le groupe");
    let slot = &hall.slots[&slots[0]];
    assert_eq!(slot.pistes(), vec![piste]);
    assert_eq!(slot.referees, referees[1..].to_vec());
    assert_eq!(slot.jobs.len(), pools.len());
    // Un autre groupe ne peut pas prendre la même piste en même temps.
    if groups.len() > 1 {
        assert!(hall.place_group(c, b, groups[1], &[piste], &[]).is_err());
    }
}

#[test]
fn a_group_on_several_pistes_is_shorter() {
    let (_, mut hall, _) = youth();
    let (c, b) = batch_of(&hall);
    let group = hall.batch_groups(c, b)[0];
    let pools = hall.group_pools(c, b, group);
    let one = hall.pistes[0].key;
    hall.place_group(c, b, group, &[one], &[]).unwrap();
    let long = hall.slots[&hall.jobs[&pools[0]].slot.unwrap()].duration;
    let two: Vec<_> = hall.pistes.iter().take(2).map(|p| p.key).collect();
    hall.place_group(c, b, group, &two, &[]).unwrap();
    let slot = &hall.slots[&hall.jobs[&pools[0]].slot.unwrap()];
    assert_eq!(slot.pistes(), two);
    assert!(slot.duration < long);
}

#[test]
fn same_club_referee_is_reported() {
    let (_, hall, referees) = youth();
    let (c, b) = batch_of(&hall);
    let all: Vec<_> = hall.batch_groups(c, b).into_iter().flat_map(|g| hall.group_club_conflicts(c, b, g, &referees)).collect();
    assert!(all.iter().any(|x| x.referee == referees[0] && x.club == "YERRES"));
    assert!(all.iter().all(|x| x.referee == referees[0]));
}
