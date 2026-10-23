//! Rencontres par équipes : le journal des relais d'un match de tableau
//! survit à l'enregistrement et à la réouverture du fichier.

use bp_core::contest::{players::new_player, Contest};
use bp_core::player::{AttrId, PlayerClass, PlayerRef};
use bp_core::relay::{RelayFormat, RelayLog};
use bp_core::rounds::table::supervisor::TableSupervisor;

#[test]
fn relay_log_of_a_table_bout_is_saved_and_reloaded() {
    let mut c = Contest::new(None, false);
    c.set_team_event(true);
    c.create_default_schedule(true);
    for (k, team) in ["YERRES", "MELUN", "BRUNOY", "SAVIGNY"].iter().enumerate() {
        for i in 0..3 {
            let mut p = new_player(PlayerClass::Fencer);
            p.set_str(&AttrId::global("name"), &format!("TIREUR{k}{i}"));
            p.set_str(&AttrId::global("team"), team);
            c.with_schedule(|s, ctx| {
                s.checkin_mut().unwrap().add(ctx, p);
            });
        }
    }
    let fencers: Vec<PlayerRef> = c.players.iter().filter(|p| p.class == PlayerClass::Fencer).map(|p| p.reference).collect();
    for f in fencers {
        c.with_schedule(|s, ctx| s.checkin_mut().unwrap().set_attending(ctx, f, true));
    }
    c.with_schedule(|s, ctx| {
        s.lock_current(ctx);
        s.enter_next(ctx);
    });
    let log = RelayLog {
        revision: 3,
        format: RelayFormat::default(),
        top: 1,
        lineup: vec!["A".into(), "B".into(), "C".into(), "D".into(), "E".into(), "F".into()],
        relays: vec![[5, 2], [7, 10]],
        winner: None,
    };
    let i = c.schedule.current_index();
    let node = c.with_schedule(|s, _| {
        let sup = s.stage_mut(i).unwrap().as_any_mut().downcast_mut::<TableSupervisor>().unwrap();
        let node = sup.table_sets()[0].nodes().iter().position(|n| n.bout.as_ref().is_some_and(|m| m.opponent(0).is_some() && m.opponent(1).is_some())).unwrap();
        sup.table_set_mut(0).bout_mut(node).unwrap().relay = Some(log.clone());
        node
    });
    let path = std::env::temp_dir().join(format!("managecomp-relais-{}.cotcot", std::process::id()));
    c.save_to(&path).unwrap();
    let mut reloaded = Contest::new(None, false);
    reloaded.load_file(&path).unwrap();
    let _ = std::fs::remove_file(&path);
    let sup = reloaded.schedule.stage(i).unwrap().as_any().downcast_ref::<TableSupervisor>().unwrap();
    assert_eq!(sup.table_sets()[0].nodes()[node].bout.as_ref().unwrap().relay.as_ref(), Some(&log));
}
