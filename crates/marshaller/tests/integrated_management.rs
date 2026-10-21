//! Organisation intégrée : les deux fonctions n'imposent pas l'autre ressource.
use bp_core::{
    contest::{players::new_player, Contest},
    player::{AttrId, PlayerClass},
    rounds::pool::PoolAllocatorStage,
};
use bp_net::Message;
use marshaller::{hall::Hall, model::NetCommand};

fn send(c: &Contest, hall: &mut Hall) {
    for p in c.parcels() {
        let mut m = Message::new(p.name);
        m.set_net_id(p.net_id);
        m.set_fitness(1);
        for (k, v) in p.fields {
            m.set_str(&k, &v);
        }
        hall.on_message(&m);
    }
}

fn fixture() -> (Contest, Hall, u32, u32) {
    let mut c = Contest::new_with_default_stages(None);
    for i in 0..12 {
        let mut p = new_player(PlayerClass::Fencer);
        p.set_str(&AttrId::global("name"), &format!("TIREUR{i}"));
        p.set_uint(&AttrId::global("ranking"), i + 1);
        let r = c.add_fencer(p).unwrap();
        c.with_schedule(|s, ctx| {
            s.checkin_mut().unwrap().set_attending(ctx, r, true);
        });
    }
    let mut r = new_player(PlayerClass::Referee);
    r.set_ref(10000);
    r.set_str(&AttrId::global("name"), "ARBITRE");
    r.set_uint(&AttrId::global("attending"), 1);
    let reference = c.referees.add(r);
    c.with_schedule(|s, ctx| {
        s.lock_current(ctx);
        s.enter_next(ctx);
    });
    let mut hall = Hall::new(None);
    send(&c, &mut hall);
    hall.set_referee_attending(reference, true);
    let job = *hall.jobs.keys().next().unwrap();
    hall.take_outbox();
    (c, hall, job, reference)
}

fn apply(c: &mut Contest, hall: &mut Hall, source: u32) {
    let m = hall
        .take_outbox()
        .into_iter()
        .filter_map(|cmd| match cmd {
            NetCommand::Spread(m)
                if m.is("BellePoule2D::Roadmap") && m.get_uint("source") == source =>
            {
                Some(m)
            }
            _ => None,
        })
        .next_back()
        .unwrap();
    assert!(c.apply_pool_roadmap(
        source,
        m.fitness(),
        &m.get_list("pistes"),
        m.get_str("start_date").as_deref(),
        m.get_str("start_time").as_deref(),
        &m.get_list("referees")
    ));
}

#[test]
fn referee_only_never_creates_a_physical_piste_and_survives_reload() {
    let (mut c, mut hall, job, reference) = fixture();
    hall.set_job_referees(job, &[reference]).unwrap();
    assert!(hall.pistes.is_empty());
    let slot = &hall.slots[&hall.jobs[&job].slot.unwrap()];
    assert!(slot.pistes().is_empty());
    assert_eq!(slot.referees, vec![reference]);
    let other = *hall.jobs.keys().find(|key| **key != job).unwrap();
    assert!(!hall.pool_referee_available(other, reference));
    let source = hall.jobs[&job].netid;
    apply(&mut c, &mut hall, source);
    let xml = c.to_xml();
    let mut loaded = Contest::new(None, false);
    loaded.load_xml_str(&xml).unwrap();
    let stage = loaded
        .schedule
        .stages()
        .find_map(|s| s.as_any().downcast_ref::<PoolAllocatorStage>())
        .unwrap();
    {
        let round = stage.round();
        let pool = round.pools().iter().find(|p| p.net_id == source).unwrap();
        assert!(pool.strips().is_empty());
        assert_eq!(pool.referees(), &[reference]);
    }
    let mut restored = Hall::new(None);
    send(&loaded, &mut restored);
    assert!(restored.pistes.is_empty());
    let job = restored.jobs.values().find(|j| j.netid == source).unwrap();
    assert_eq!(restored.slots[&job.slot.unwrap()].referees, vec![reference]);
}

#[test]
fn piste_only_does_not_require_referees_and_restores_multiple_pistes() {
    let (mut c, mut hall, job, _) = fixture();
    let a = hall.add_piste(0.0, 0.0);
    let b = hall.add_piste(0.0, 0.0);
    hall.drop_job(job, a);
    assert!(hall.set_pool_resources(job, &[a, b], &[]).is_err());
    hall.set_pool_resources_optional(job, &[a, b], &[], false)
        .unwrap();
    let slot = &hall.slots[&hall.jobs[&job].slot.unwrap()];
    assert_eq!(slot.pistes(), vec![a, b]);
    assert!(slot.referees.is_empty());
    let source = hall.jobs[&job].netid;
    apply(&mut c, &mut hall, source);
    let xml = c.to_xml();
    let mut loaded = Contest::new(None, false);
    loaded.load_xml_str(&xml).unwrap();
    let mut restored = Hall::new(None);
    send(&loaded, &mut restored);
    let job = restored.jobs.values().find(|j| j.netid == source).unwrap();
    let slot = &restored.slots[&job.slot.unwrap()];
    assert_eq!(slot.pistes().len(), 2);
    assert!(slot.referees.is_empty());
}

#[test]
fn referee_catalog_persists_without_an_open_competition() {
    let (_, hall, _, reference) = fixture();
    let path = std::env::temp_dir().join(format!("bp-local-catalog-{}.xml", std::process::id()));
    hall.save_referee_catalog(&path).unwrap();
    let mut restored = Hall::new(None);
    assert!(restored.load_referee_catalog(&path));
    assert!(restored.competitions.is_empty());
    assert!(restored.pistes.is_empty());
    assert_eq!(
        restored.referees.get(reference).unwrap().player.name(),
        "ARBITRE"
    );
    assert!(restored.referees.get(reference).unwrap().player.attending());
    std::fs::remove_file(path).unwrap();
}
