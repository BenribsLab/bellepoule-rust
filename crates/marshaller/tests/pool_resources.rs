//! Ressources de poule : réservation atomique, feuille de route et relecture.
use bp_core::{
    contest::{players::new_player, Contest},
    player::{AttrId, PlayerClass},
    rounds::pool::PoolAllocatorStage,
};
use bp_net::message::Message;
use marshaller::{hall::Hall, model::NetCommand, slot::JobKey};

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

fn fixture() -> (Contest, Hall, Vec<JobKey>, Vec<u32>) {
    let mut c = Contest::new_with_default_stages(None);
    for i in 0..12 {
        let mut p = new_player(PlayerClass::Fencer);
        p.set_str(&AttrId::global("name"), &format!("TIREUR {i}"));
        p.set_uint(&AttrId::global("ranking"), i + 1);
        let r = c.add_fencer(p).unwrap();
        c.with_schedule(|s, ctx| {
            s.checkin_mut().unwrap().set_attending(ctx, r, true);
        });
    }
    let mut referees = Vec::new();
    for i in 0..2 {
        let mut p = new_player(PlayerClass::Referee);
        p.set_ref(10000 + i);
        p.set_str(&AttrId::global("name"), &format!("ARBITRE {i}"));
        p.set_uint(&AttrId::global("attending"), 1);
        referees.push(c.referees.add(p));
    }
    c.with_schedule(|s, ctx| {
        s.lock_current(ctx);
        s.enter_next(ctx);
    });
    let mut hall = Hall::new(None);
    hall.default_layout();
    send(&c, &mut hall);
    for r in &referees {
        hall.set_referee_attending(*r, true);
    }
    let jobs: Vec<_> = hall.jobs.keys().copied().collect();
    assert!(jobs.len() >= 2);
    let piste = hall.pistes[0].key;
    hall.drop_job(jobs[0], piste);
    hall.take_outbox();
    (c, hall, jobs, referees)
}

#[test]
fn common_reservation_prevents_conflicts_and_releases_all_resources() {
    let (_, mut hall, jobs, referees) = fixture();
    let pistes: Vec<_> = hall.pistes.iter().take(3).map(|p| p.key).collect();
    assert!(hall
        .set_pool_resources(jobs[0], &pistes[..2], &[referees[0], referees[0]])
        .is_err());
    hall.set_pool_resources(jobs[0], &pistes[..2], &referees)
        .unwrap();
    let sk = hall.jobs[&jobs[0]].slot.unwrap();
    assert_eq!(hall.slots[&sk].pistes(), pistes[..2]);
    for p in &pistes[..2] {
        assert_eq!(hall.piste(*p).unwrap().slots, vec![sk]);
    }
    for r in &referees {
        assert_eq!(hall.referees.get(*r).unwrap().slots, vec![sk]);
    }
    assert!(!hall.job_drop_allowed(jobs[1], pistes[1]));
    hall.drop_job(jobs[1], pistes[2]);
    let before = hall.slots[&sk].clone();
    assert!(hall
        .set_pool_resources(jobs[0], &[pistes[0], pistes[2]], &referees)
        .is_err());
    assert_eq!(hall.slots[&sk].pistes(), before.pistes());
    assert_eq!(hall.slots[&sk].referees, before.referees);
    hall.set_pool_resources(jobs[0], &pistes[..1], &referees[..1])
        .unwrap();
    assert!(hall.piste(pistes[1]).unwrap().slots.is_empty());
    assert!(hall.referees.get(referees[1]).unwrap().slots.is_empty());
    hall.set_pool_resources(jobs[0], &pistes[..2], &referees)
        .unwrap();
    hall.slot_cancel(sk);
    for p in &pistes[..2] {
        assert!(hall.piste(*p).unwrap().slots.is_empty());
    }
    for r in &referees {
        assert!(hall.referees.get(*r).unwrap().slots.is_empty());
    }
}

#[test]
fn paired_pistes_reach_bellepoule_and_survive_reopening_both_models() {
    let (mut c, mut hall, jobs, referees) = fixture();
    let pistes: Vec<_> = hall.pistes.iter().take(2).map(|p| p.key).collect();
    hall.set_pool_resources(jobs[0], &pistes, &referees)
        .unwrap();
    hall.renumber_piste(pistes[1], 12).unwrap();
    let roadmap = hall
        .take_outbox()
        .into_iter()
        .filter_map(|cmd| match cmd {
            NetCommand::Spread(m)
                if m.is("BellePoule2D::Roadmap")
                    && m.get_uint("source") == hall.jobs[&jobs[0]].netid =>
            {
                Some(m)
            }
            _ => None,
        })
        .next_back()
        .unwrap();
    assert_eq!(roadmap.get_list("pistes"), vec![1, 12]);
    assert_eq!(roadmap.get_uint("piste"), 1);
    assert!(c.apply_pool_roadmap(
        roadmap.get_uint("source"),
        1,
        &roadmap.get_list("pistes"),
        roadmap.get_str("start_date").as_deref(),
        roadmap.get_str("start_time").as_deref(),
        &roadmap.get_list("referees")
    ));
    let xml = c.to_xml();
    let mut loaded = Contest::new(None, false);
    loaded.load_xml_str(&xml).unwrap();
    let allocator = loaded
        .schedule
        .stages()
        .find_map(|s| s.as_any().downcast_ref::<PoolAllocatorStage>())
        .unwrap();
    {
        let round = allocator.round();
        let pool = round
            .pools()
            .iter()
            .find(|p| p.net_id == roadmap.get_uint("source"))
            .unwrap();
        assert_eq!(pool.strips(), vec![1, 12]);
        assert_eq!(
            pool.piste_referees(),
            &[(1, referees[0]), (12, referees[1])]
        );
        assert_eq!(pool.referees(), referees);
        assert!(pool.match_assignments().iter().all(|a| a.referee.is_some()));
    }
    let mut restored = Hall::new(None);
    restored.default_layout();
    send(&loaded, &mut restored);
    let job = restored
        .jobs
        .values()
        .find(|j| j.netid == roadmap.get_uint("source"))
        .unwrap();
    let slot = &restored.slots[&job.slot.unwrap()];
    assert_eq!(
        slot.pistes()
            .iter()
            .map(|key| restored.piste(*key).unwrap().id)
            .collect::<Vec<_>>(),
        vec![1, 12]
    );
    assert_eq!(slot.referees, referees);
}

#[test]
fn moving_a_pool_keeps_secondary_pistes_and_referee_pairing() {
    let (_, mut hall, jobs, referees) = fixture();
    let pistes: Vec<_> = hall.pistes.iter().take(3).map(|p| p.key).collect();
    hall.set_pool_resources(jobs[0], &pistes[..2], &referees)
        .unwrap();
    hall.start_moving_job(jobs[0]);
    assert!(hall.piste(pistes[0]).unwrap().slots.is_empty());
    assert!(hall.piste(pistes[1]).unwrap().slots.is_empty());
    hall.select_piste(pistes[2]);
    let slot = &hall.slots[&hall.jobs[&jobs[0]].slot.unwrap()];
    assert_eq!(slot.pistes(), vec![pistes[2], pistes[1]]);
    assert_eq!(slot.referees, referees);
    let sk = hall.jobs[&jobs[0]].slot.unwrap();
    assert_eq!(hall.piste(pistes[1]).unwrap().slots, vec![sk]);
    assert!(hall.floating.is_none());
}

#[test]
fn a_scheduled_pool_can_be_dragged_to_a_free_second_piste() {
    let (_, mut hall, jobs, referees) = fixture();
    let pistes: Vec<_> = hall.pistes.iter().take(3).map(|p| p.key).collect();
    hall.set_pool_resources(jobs[0], &pistes[..1], &referees[..1])
        .unwrap();
    let job = &hall.jobs[&jobs[0]];
    let (c, b, original_slot) = (job.competition, job.batch, job.slot);
    assert!(hall.batch_pending_selected(c, b, &jobs[..1]).is_empty());
    assert_eq!(hall.batch_draggable_jobs(c, b, &jobs[..1]), jobs[..1]);
    assert!(!hall.job_drop_allowed(jobs[0], pistes[0]));
    assert!(hall.job_drop_allowed(jobs[0], pistes[1]));
    hall.piste_mut(pistes[1]).unwrap().blocked = true;
    assert!(!hall.job_drop_allowed(jobs[0], pistes[1]));
    hall.piste_mut(pistes[1]).unwrap().blocked = false;
    hall.drop_job(jobs[1], pistes[2]);
    // Le conflit est vérifié à l'horaire de la poule, même si le curseur change.
    let start = hall.slots[&original_slot.unwrap()].start;
    hall.timeline
        .set_cursor_time(marshaller::time::add(start, 180 * marshaller::time::MINUTE));
    assert!(hall.job_drop_allowed(jobs[0], pistes[1]));
    assert!(!hall.job_drop_allowed(jobs[0], pistes[2]));
    // Sans validation du dialogue, aucun second créneau ne doit être créé.
    hall.drop_job(jobs[0], pistes[1]);
    assert_eq!(hall.jobs[&jobs[0]].slot, original_slot);
    assert!(hall.piste(pistes[1]).unwrap().slots.is_empty());
    hall.set_pool_resources(jobs[0], &pistes[..2], &referees)
        .unwrap();
    assert_eq!(hall.jobs[&jobs[0]].slot, original_slot);
    assert_eq!(hall.slots[&original_slot.unwrap()].pistes(), pistes[..2]);
}

#[test]
fn referee_choices_exclude_other_reservations_at_the_pool_time() {
    let (_, mut hall, jobs, referees) = fixture();
    let pistes: Vec<_> = hall.pistes.iter().take(2).map(|p| p.key).collect();
    hall.set_pool_resources(jobs[0], &pistes[..1], &referees[..1])
        .unwrap();
    assert!(
        hall.pool_referee_available(jobs[0], referees[0]),
        "sa propre affectation reste visible"
    );
    assert!(hall.pool_referee_available(jobs[0], referees[1]));
    hall.drop_job(jobs[1], pistes[1]);
    hall.set_pool_resources(jobs[1], &pistes[1..], &referees[1..])
        .unwrap();
    assert!(!hall.pool_referee_available(jobs[0], referees[1]));
    assert!(hall.pool_referee_available(jobs[1], referees[1]));
    let first = hall.slots[&hall.jobs[&jobs[0]].slot.unwrap()].clone();
    hall.timeline.set_cursor_time(marshaller::time::add(
        first.start,
        180 * marshaller::time::MINUTE,
    ));
    assert!(
        !hall.pool_referee_available(jobs[0], referees[1]),
        "l'horaire de la poule prime sur le curseur"
    );
    assert!(hall
        .set_pool_resources(jobs[0], &pistes[..1], &referees[1..])
        .is_err());
    let second = hall.jobs[&jobs[1]].slot.unwrap();
    // Un arbitre redevient proposé dès que les créneaux ne se chevauchent plus.
    let end = marshaller::time::add(first.start, first.duration);
    hall.slots.get_mut(&second).unwrap().start = end;
    assert!(hall.pool_referee_available(jobs[0], referees[1]));
    hall.slots.get_mut(&second).unwrap().start =
        marshaller::time::add(end, -marshaller::time::MINUTE);
    assert!(
        !hall.pool_referee_available(jobs[0], referees[1]),
        "un chevauchement partiel suffit"
    );
    hall.slot_cancel(second);
    assert!(hall.pool_referee_available(jobs[0], referees[1]));
    hall.set_referee_attending(referees[1], false);
    assert!(
        !hall.pool_referee_available(jobs[0], referees[1]),
        "les absents ne sont pas proposés"
    );
}
