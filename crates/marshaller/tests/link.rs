//! Échange BellePoule ↔ Marshaller sans réseau : les colis d'une compétition
//! (compétition, arbitres, tireurs, tour de poules, poules) créent les lots et
//! les matchs du Marshaller ; une poule placée sur une piste avec un arbitre
//! renvoie une feuille de route appliquée à la poule de BellePoule.

use bp_core::contest::players::new_player;
use bp_core::contest::Contest;
use bp_core::player::{AttrId, PlayerClass};
use bp_core::rounds::pool::PoolAllocatorStage;
use bp_net::message::Message;
use marshaller::hall::Hall;
use marshaller::model::NetCommand;

fn messages(c: &Contest) -> Vec<Message> {
    c.parcels()
        .into_iter()
        .map(|p| {
            let mut m = Message::new(p.name);
            m.set_net_id(p.net_id);
            for (k, v) in &p.fields {
                m.set_str(k, v);
            }
            m.set_fitness(1);
            m
        })
        .collect()
}

#[test]
fn pools_are_scheduled_and_roadmap_comes_back() {
    let mut c = Contest::new_with_default_stages(None);
    for i in 0..12u32 {
        let mut p = new_player(PlayerClass::Fencer);
        p.set_str(&AttrId::global("name"), &format!("TIREUR{i:02}"));
        p.set_str(&AttrId::global("club"), if i % 2 == 0 { "CLUB A" } else { "CLUB B" });
        p.set_uint(&AttrId::global("ranking"), i + 1);
        let r = c.add_fencer(p).unwrap();
        c.with_schedule(|s, ctx| {
            s.checkin_mut().unwrap().set_attending(ctx, r, true);
        });
    }
    // Un arbitre de l'arme de la compétition
    let mut referee = new_player(PlayerClass::Referee);
    referee.set_str(&AttrId::global("name"), "ARBITRE");
    referee.set_str(&AttrId::global("first_name"), "Jean");
    referee.set_str(&AttrId::global("club"), "CLUB C");
    let referee_ref = c.referees.add(referee);
    c.with_schedule(|s, ctx| {
        s.lock_current(ctx);
        s.enter_next(ctx);
    });
    let pools = c
        .schedule
        .stages()
        .find_map(|s| s.as_any().downcast_ref::<PoolAllocatorStage>())
        .map(|a| a.round().pools().iter().map(|p| p.net_id).collect::<Vec<_>>())
        .unwrap();
    assert!(!pools.is_empty());

    // Le Marshaller reçoit tout.
    let mut hall = Hall::new(None);
    hall.default_layout();
    for m in messages(&c) {
        hall.on_message(&m);
    }
    assert_eq!(hall.competitions.len(), 1);
    let comp = &hall.competitions[0];
    assert_eq!(comp.batches.len(), 1, "un lot par tour de poules");
    assert_eq!(comp.batches[0].rows.len(), pools.len(), "une ligne par poule");
    assert!(!comp.fencers.is_empty(), "tireurs reçus");
    let weapon = comp.weapon.clone();
    let (cid, bid) = (comp.id, comp.batches[0].id);
    let list = hall.referees.list_index(&weapon).unwrap();
    assert!(hall.referees.lists[list].members.contains(&referee_ref), "arbitre reçu dans la liste de l'arme");
    hall.set_referee_attending(referee_ref, true);
    hall.take_outbox();

    // Placement de la première poule sur la piste 1 avec l'arbitre.
    let job = hall.batch_pending_jobs(cid, bid)[0];
    let piste = hall.pistes[0].key;
    assert!(hall.job_drop_allowed(job, piste));
    hall.drop_job(job, piste);
    assert!(hall.referee_drop_allowed(referee_ref, piste));
    hall.drop_referee(referee_ref, piste);

    // Feuille de route renvoyée à BellePoule.
    let roadmap = hall
        .take_outbox()
        .into_iter()
        .filter_map(|cmd| match cmd {
            NetCommand::Spread(m) if m.is("BellePoule2D::Roadmap") => Some(m),
            _ => None,
        })
        .last()
        .expect("feuille de route diffusée");
    assert_eq!(roadmap.get_uint("piste"), 1);
    assert_eq!(roadmap.get_list("referees"), vec![referee_ref]);

    // Côté BellePoule : la poule prend la piste, l'heure et l'arbitre.
    let source = roadmap.get_uint("source");
    assert!(c.apply_roadmap(
        source,
        roadmap.fitness(),
        roadmap.get_uint("piste"),
        roadmap.get_str("start_date").as_deref(),
        roadmap.get_str("start_time").as_deref(),
        &roadmap.get_list("referees"),
    ));
    let alloc = c.schedule.stages().find_map(|s| s.as_any().downcast_ref::<PoolAllocatorStage>()).unwrap();
    let round = alloc.round();
    let pool = round.pools().iter().find(|p| p.net_id == source).unwrap();
    assert!(pool.piste_label().is_some(), "piste et heure de la poule");
    assert_eq!(pool.referees(), &[referee_ref]);

    // Renuméroter une piste déjà occupée rediffuse sa feuille de route.
    hall.renumber_piste(piste, 12).unwrap();
    let changed = hall.take_outbox().into_iter().filter_map(|cmd| match cmd {
        NetCommand::Spread(m) if m.is("BellePoule2D::Roadmap") => Some(m),
        _ => None,
    }).last().expect("feuille de route renumérotée");
    assert_eq!(changed.get_uint("piste"), 12);
    assert_eq!(changed.get_list("referees"), vec![referee_ref]);
    assert_eq!(changed.get_uint("source"), source);
}

#[test]
fn table_bouts_are_scheduled_and_roadmap_comes_back() {
    use bp_core::rounds::pool::PoolSupervisorStage;
    use bp_core::rounds::table::TableSupervisor;
    let mut c = Contest::new_with_default_stages(None);
    for i in 0..10u32 {
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
        sup.stuff(ctx);
        s.lock_current(ctx);
        s.enter_next(ctx);
    });
    assert!(c.schedule.current().unwrap().as_any().downcast_ref::<TableSupervisor>().is_some());

    let mut hall = Hall::new(None);
    hall.default_layout();
    for m in messages(&c) {
        hall.on_message(&m);
    }
    let comp = &hall.competitions[0];
    let table_batch = comp.batches.iter().find(|b| b.name.contains(":T")).expect("lot du tableau");
    assert!(!table_batch.rows.is_empty(), "matchs du tableau reçus");
    let (cid, bid) = (comp.id, table_batch.id);
    hall.take_outbox();

    let job = hall.batch_pending_jobs(cid, bid)[0];
    let piste = hall.pistes[1].key;
    hall.drop_job(job, piste);
    let roadmap = hall
        .take_outbox()
        .into_iter()
        .filter_map(|cmd| match cmd {
            NetCommand::Spread(m) if m.is("BellePoule2D::Roadmap") => Some(m),
            _ => None,
        })
        .last()
        .expect("feuille de route du match");
    assert_eq!(roadmap.get_uint("piste"), 2);
    assert!(c.apply_table_roadmap(
        roadmap.get_uint("batch"),
        roadmap.get_uint("source"),
        roadmap.fitness(),
        roadmap.get_uint("piste"),
        roadmap.get_str("start_date").as_deref(),
        roadmap.get_str("start_time").as_deref(),
        roadmap.get_uint("duration_span"),
        &roadmap.get_list("referees"),
    ));
    let sup = c.schedule.current().unwrap().as_any().downcast_ref::<TableSupervisor>().unwrap();
    let placed = sup
        .table_sets()
        .iter()
        .flat_map(|s| s.nodes().iter().filter_map(|n| n.bout.as_ref()))
        .any(|m| m.piste == 2 && m.start_time.is_some());
    assert!(placed, "le match a sa piste et son heure");
}

#[test]
fn referee_smartphone_key_reaches_bellepoule() {
    let mut c = Contest::new_with_default_stages(None);
    let weapon = c.weapon.xml_image.to_string();
    let mut hall = Hall::new(None);
    hall.referees.cryptor_key = Some("0123456789abcdefghijABCDEFGHIJKL".to_string());
    let mut p = bp_core::Player::new(PlayerClass::Referee);
    p.set_str(&AttrId::global("name"), "ARBITRE");
    p.set_str(&AttrId::global("first_name"), "Paul");
    let list = hall.referees.list_index(&weapon).unwrap();
    let reference = hall.register_referee(list, p);
    hall.referees_spread();
    let key = hall.referee_wifi_key(reference).unwrap();
    let parcel = hall
        .take_outbox()
        .into_iter()
        .filter_map(|cmd| match cmd {
            NetCommand::Spread(m) if m.is("BellePoule2D::Referee") => Some(m),
            _ => None,
        })
        .last()
        .expect("arbitre diffusé");
    let xml = parcel.get_str("xml").unwrap();
    let decrypt = |cypher: &str| {
        bp_net::cryptor::Cryptor::decrypt(cypher, "AAAAAAAAAAAAAAAAAAAAAA==", "0123456789abcdefghijABCDEFGHIJKL")
    };
    assert!(c.manage_referee_xml(&xml, &decrypt));
    assert_eq!(c.referee_password(reference), Some(key));
}

#[test]
fn bout_time_can_be_set_by_hand() {
    let mut c = Contest::new_with_default_stages(None);
    for i in 0..12u32 {
        let mut p = new_player(PlayerClass::Fencer);
        p.set_str(&AttrId::global("name"), &format!("TIREUR{i:02}"));
        p.set_uint(&AttrId::global("ranking"), i + 1);
        let r = c.add_fencer(p).unwrap();
        c.with_schedule(|s, ctx| {
            s.checkin_mut().unwrap().set_attending(ctx, r, true);
        });
    }
    c.with_schedule(|s, ctx| {
        s.lock_current(ctx);
        s.enter_next(ctx);
    });
    let mut hall = Hall::new(None);
    hall.default_layout();
    for m in messages(&c) {
        hall.on_message(&m);
    }
    let (cid, bid) = (hall.competitions[0].id, hall.competitions[0].batches[0].id);
    let jobs = hall.batch_pending_jobs(cid, bid);
    let piste = hall.pistes[0].key;
    hall.drop_job(jobs[0], piste);
    let start = hall.slots.get(&hall.jobs[&jobs[0]].slot.unwrap()).unwrap().start;
    hall.take_outbox();

    // Deux heures plus tard : accepté, la feuille de route suit.
    let later = marshaller::time::add(start, 2 * marshaller::time::HOUR);
    hall.set_job_start(jobs[0], later).unwrap();
    let roadmap = hall
        .take_outbox()
        .into_iter()
        .filter_map(|cmd| match cmd {
            NetCommand::Spread(m) if m.is("BellePoule2D::Roadmap") => Some(m),
            _ => None,
        })
        .last()
        .unwrap();
    assert_eq!(roadmap.get_str("start_time"), Some(marshaller::time::xml_time(later)));

    // Une autre poule sur la même piste ne peut pas prendre la même heure.
    hall.drop_job(jobs[1], piste);
    assert!(hall.set_job_start(jobs[1], later).is_err());
}
