//! La scission produit une competition independante, y compris par equipes.

use bp_core::contest::{players::new_player, Contest};
use bp_core::player::{AttrId, PlayerClass};
use bp_core::rounds::splitting::Splitting;
use bp_core::stage::{Stage, StageKind};
use bp_core::tournament::{user_config::UserConfig, Tournament};

fn enter_split(c: &mut Contest) -> Vec<u32> {
    c.with_schedule(|s, ctx| {
        s.add_kind(ctx, StageKind::Splitting, Some(0));
        s.lock_current(ctx);
        s.enter_next(ctx);
        let split = s
            .stage_mut(1)
            .unwrap()
            .as_any_mut()
            .downcast_mut::<Splitting>()
            .unwrap();
        split.base_mut().name = "Tableau B".into();
        split.base().attendees.clone()
    })
}

fn select(c: &mut Contest, r: u32) {
    c.with_schedule(|s, ctx| {
        s.stage_mut(1)
            .unwrap()
            .as_any_mut()
            .downcast_mut::<Splitting>()
            .unwrap()
            .set_exported(ctx, r, true)
    });
}

#[test]
fn selected_fencers_keep_their_entry_positions_and_source_is_unchanged() {
    let mut c = Contest::new(None, false);
    c.create_default_schedule(true);
    c.name = "Challenge".into();
    for i in 0..5 {
        let mut p = new_player(PlayerClass::Fencer);
        p.set_str(&AttrId::global("name"), &format!("TIREUR{i}"));
        p.set_uint(&AttrId::global("ranking"), i + 1);
        let r = c.add_fencer(p).unwrap();
        c.with_schedule(|s, ctx| {
            s.checkin_mut().unwrap().set_attending(ctx, r, true);
        });
    }
    let refs = enter_split(&mut c);
    select(&mut c, refs[1]);
    select(&mut c, refs[4]);
    let before = c.to_xml();
    let mut t = Tournament::new(UserConfig::new());
    let source = t.manage(c);
    let target = t.split_current_contest(source).unwrap();
    assert_ne!(source, target);
    assert_eq!(t.contest_mut(source).unwrap().to_xml(), before);
    let target = t.contest_mut(target).unwrap();
    assert_eq!(target.name, "Challenge");
    assert_eq!(target.label, "Tableau B");
    assert!(target.filename.is_none());
    assert!(target.settings.manual_classification);
    assert!(!target
        .schedule
        .stages()
        .any(|s| s.kind() == StageKind::PoolSupervisor));
    let mut ranks: Vec<_> = target
        .players
        .iter()
        .map(|p| {
            assert!(!p.attr("exported").unwrap().as_bool());
            assert!(p.attr("final_rank").is_none());
            p.attr("ranking").unwrap().as_uint()
        })
        .collect();
    ranks.sort();
    assert_eq!(ranks, vec![2, 5]);
    let xml = target.to_xml();
    let mut loaded = Contest::new(None, false);
    loaded.load_xml_str(&xml).unwrap();
    assert_eq!(loaded.players.len(), 2);
    t.contest_mut(source).unwrap().with_schedule(|s, ctx| {
        s.lock_current(ctx);
        s.enter_next(ctx);
    });
    assert_eq!(
        t.contest(source)
            .unwrap()
            .schedule
            .current()
            .unwrap()
            .base()
            .attendees
            .len(),
        3
    );
}

#[test]
fn split_teams_rebuild_membership_inside_the_target_store() {
    let mut c = Contest::new(None, false);
    c.create_default_schedule(true);
    c.set_team_event(true);
    c.sync_settings();
    for team in ["CLUB A", "CLUB B"] {
        for i in 0..3 {
            let mut p = new_player(PlayerClass::Fencer);
            p.set_str(&AttrId::global("name"), &format!("{team} {i}"));
            p.set_str(&AttrId::global("team"), team);
            p.set_uint(&AttrId::global("ranking"), i + 1);
            let r = c.add_fencer(p).unwrap();
            c.with_schedule(|s, ctx| {
                s.checkin_mut().unwrap().set_attending(ctx, r, true);
            });
        }
    }
    let refs = enter_split(&mut c);
    assert_eq!(refs.len(), 2);
    select(&mut c, refs[1]);
    let source_team = c.players.get(refs[1]).unwrap().clone();
    let mut target = c.duplicate_split(1, None).unwrap();
    let teams: Vec<_> = target
        .players
        .iter()
        .filter(|p| p.class == PlayerClass::Team)
        .collect();
    assert_eq!(teams.len(), 1);
    let team = teams[0];
    assert_eq!(team.name(), source_team.name());
    assert_eq!(team.members.len(), 3);
    assert_eq!(team.attr("ranking").unwrap().as_uint(), 2);
    assert!(team.attending());
    for member in &team.members {
        let player = target.players.get(*member).unwrap();
        assert_eq!(player.class, PlayerClass::Fencer);
        assert_eq!(player.team, Some(team.reference));
        assert!(player.attending());
        assert!(!player.attr("exported").unwrap().as_bool());
    }
    assert_eq!(target.players.len(), 4);
    assert_eq!(c.players.get(refs[1]).unwrap().members, source_team.members);
    let xml = target.to_xml();
    let mut loaded = Contest::new(None, false);
    loaded.load_xml_str(&xml).unwrap();
    assert_eq!(loaded.players.len(), 4);
    let team = loaded
        .players
        .iter()
        .find(|p| p.class == PlayerClass::Team)
        .unwrap();
    assert_eq!(team.members.len(), 3);
    assert!(team
        .members
        .iter()
        .all(|r| loaded.players.get(*r).unwrap().team == Some(team.reference)));
}
