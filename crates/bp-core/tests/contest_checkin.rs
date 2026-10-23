//! Actions de l'écran d'appel : ajout, présence, suppression, équipes.

use bp_core::contest::players::new_player;
use bp_core::contest::Contest;
use bp_core::player::{AttrId, PlayerClass};
use bp_core::rounds::checkin::ToggleOutcome;

fn fencer(name: &str, ranking: u32) -> bp_core::player::Player {
    let mut p = new_player(PlayerClass::Fencer);
    p.set_str(&AttrId::global("name"), name);
    p.set_uint(&AttrId::global("ranking"), ranking);
    p
}

#[test]
fn several_latecomers_are_registered_before_renumbering() {
    use bp_core::attribute::Value;
    let mut c = Contest::new_with_default_stages(None);
    c.with_schedule(|s, ctx| {
        let checkin = s.checkin_mut().unwrap();
        let r = checkin.form_new_player(ctx, fencer("MARTIN", 1)).unwrap();
        checkin.set_attending(ctx, r, true);
    });
    c.with_schedule(|s, ctx| s.lock_current(ctx));
    // Le dialogue travaille dans le magasin, avant de transmettre ses ajouts
    // au pointage. Les retardataires sont renumérotés à partir de 10000.
    let mut additions = Vec::new();
    for name in ["ALBERT", "ZOE"] {
        let mut player = fencer(name, 2);
        player.set(&AttrId::global("attending"), Value::Bool(true));
        player.set_str(&AttrId::global("incident"), "R");
        additions.push(c.players.add(player));
    }
    c.with_schedule(|s, ctx| {
        s.checkin_mut().unwrap().on_attendees_toggled(ctx, &additions);
        s.apply_renumbering(ctx);
    });
    let checkin = c.schedule.checkin().unwrap();
    assert_eq!(checkin.players().len(), 3);
    let mut names: Vec<_> = checkin.players().iter().map(|r| c.players.get(*r).unwrap().name()).collect();
    names.sort();
    assert_eq!(names, ["ALBERT", "MARTIN", "ZOE"]);
    assert_eq!(checkin.presents().len(), 3);
    assert_eq!(c.players.get(10000).unwrap().name(), "ALBERT");
    assert_eq!(c.players.get(10001).unwrap().name(), "ZOE");
}

#[test]
fn add_toggle_remove_individuals() {
    let mut c = Contest::new_with_default_stages(None);
    let mut refs = Vec::new();
    for (i, n) in ["DUPONT", "MARTIN", "BERNARD", "PETIT"].iter().enumerate() {
        let r = c.with_schedule(|s, ctx| s.checkin_mut().unwrap().form_new_player(ctx, fencer(n, i as u32 + 1))).unwrap();
        refs.push(r);
    }
    let tally = |c: &mut Contest| {
        let team = c.settings.team_event;
        let t = c.schedule.checkin().unwrap().tally(&c.players, team);
        (t.presents(), t.absents())
    };
    assert_eq!(tally(&mut c), (0, 4), "un nouveau tireur est absent tant qu'on ne l'a pas coché");

    c.with_schedule(|s, ctx| {
        let ch = s.checkin_mut().unwrap();
        assert_eq!(ch.set_attending(ctx, refs[0], true), ToggleOutcome::Done);
        ch.set_attending(ctx, refs[1], true);
    });
    assert_eq!(tally(&mut c), (2, 2));

    c.with_schedule(|s, ctx| s.checkin_mut().unwrap().set_all_attending(ctx, true));
    assert_eq!(tally(&mut c), (4, 0));
    c.with_schedule(|s, ctx| s.checkin_mut().unwrap().set_all_attending(ctx, false));
    assert_eq!(tally(&mut c), (0, 4));

    c.with_schedule(|s, ctx| s.checkin_mut().unwrap().remove(ctx, refs[2]));
    assert_eq!(tally(&mut c), (0, 3));
    assert!(c.players.get(refs[2]).is_none());
}

#[test]
fn editing_a_fencer_attribute_applies_side_effects() {
    let mut c = Contest::new_with_default_stages(None);
    let r = c.with_schedule(|s, ctx| s.checkin_mut().unwrap().form_new_player(ctx, fencer("DUPONT", 5))).unwrap();
    c.with_schedule(|s, ctx| {
        let ch = s.checkin_mut().unwrap();
        ch.set_player_attribute(ctx, r, "name", Some("DURAND"));
        ch.set_player_attribute(ctx, r, "club", Some("BETTON"));
        ch.set_player_attribute(ctx, r, "club", None);
    });
    let p = c.players.get(r).unwrap();
    assert_eq!(p.name(), "DURAND");
    assert!(p.attr("club").is_none(), "un champ vidé est supprimé");
}

#[test]
fn team_events_refuse_to_toggle_a_team_directly() {
    let mut c = Contest::new_with_default_stages(None);
    c.set_team_event(true);
    c.sync_settings();
    let mut team = new_player(PlayerClass::Team);
    team.set_str(&AttrId::global("name"), "ESCRIME CLUB");
    let t = c.with_schedule(|s, ctx| s.checkin_mut().unwrap().form_new_player(ctx, team)).unwrap();
    let outcome = c.with_schedule(|s, ctx| s.checkin_mut().unwrap().set_attending(ctx, t, true));
    assert!(matches!(outcome, ToggleOutcome::TeamNotToggleable { .. }));

    // Un tireur qui déclare cette équipe la rejoint.
    let f = c.with_schedule(|s, ctx| s.checkin_mut().unwrap().form_new_player(ctx, fencer("DUPONT", 1))).unwrap();
    c.with_schedule(|s, ctx| s.checkin_mut().unwrap().set_player_attribute(ctx, f, "team", Some("ESCRIME CLUB")));
    assert_eq!(c.players.get(f).unwrap().team, Some(t));
    assert_eq!(c.players.get(t).unwrap().members, vec![f]);

    // Supprimer l'équipe supprime ses membres.
    c.with_schedule(|s, ctx| s.checkin_mut().unwrap().remove(ctx, t));
    assert!(c.players.get(f).is_none());
}


#[test]
fn team_capacity_and_missing_fencers_are_checked_at_checkin() {
    use bp_core::player::{AttrId, PlayerClass};
    let mut c = Contest::new_with_default_stages(None);
    c.set_team_event(true);
    c.set_team_size(3, 1);
    let mut members = Vec::new();
    for i in 0..4 {
        let mut p = bp_core::contest::players::new_player(PlayerClass::Fencer);
        p.set_str(&AttrId::global("name"), &format!("TIREUR{i}"));
        p.set_str(&AttrId::global("team"), "YERRES");
        p.set_uint(&AttrId::global("attending"), 1);
        members.push(c.with_schedule(|s, ctx| s.checkin_mut().unwrap().add(ctx, p)));
    }
    let settings = c.settings.clone();
    let ch = c.schedule.checkin().unwrap();
    // 3 tireurs + 1 remplaçant : un cinquième est refusé.
    assert!(ch.team_full_error(&c.players, &settings, None, "YERRES").is_some());
    assert!(ch.team_full_error(&c.players, &settings, Some(members[0]), "YERRES").is_none());
    assert!(ch.team_full_error(&c.players, &settings, None, "MELUN").is_none());
    // Deux absents : il manque un tireur.
    for m in &members[..2] {
        c.with_schedule(|s, ctx| s.checkin_mut().unwrap().set_attending(ctx, *m, false));
    }
    let settings = c.settings.clone();
    let missing = c.schedule.checkin().unwrap().incomplete_teams(&c.players, &settings);
    assert_eq!(missing, vec![("YERRES".to_string(), 2)]);
}
