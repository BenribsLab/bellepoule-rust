//! Exports d'une compétition terminée : HTML, FFF, CSV, relecture du fichier.

use bp_core::contest::players::new_player;
use bp_core::contest::Contest;
use bp_core::player::{AttrId, PlayerClass};
use bp_core::rounds::pool::PoolSupervisorStage;
use bp_core::rounds::table::TableSupervisor;
use rand::SeedableRng;

fn finished_competition(n: u32) -> Contest {
    finished_with_quota(n, None)
}

fn finished_with_quota(n: u32, quota: Option<u32>) -> Contest {
    let mut c = Contest::new_with_default_stages(None);
    c.name = "Challenge de test".into();
    for i in 0..n {
        let mut p = new_player(PlayerClass::Fencer);
        p.set_str(&AttrId::global("name"), &format!("TIREUR{i:02}"));
        p.set_str(&AttrId::global("first_name"), "Jean");
        p.set_str(&AttrId::global("club"), "BETTON");
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
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(5);
    c.with_schedule(|s, ctx| {
        let pools = s.stage_mut(2).unwrap().as_any_mut().downcast_mut::<PoolSupervisorStage>().unwrap();
        pools.round().set_nb_qualified(quota);
        pools.stuff(ctx);
        s.lock_current(ctx);
        s.enter_next(ctx);
        let t = s.stage_mut(3).unwrap().as_any_mut().downcast_mut::<TableSupervisor>().unwrap();
        let set = t.visible_table_sets()[0];
        t.select_table_set(Some(set));
        for _ in 0..10 {
            t.stuff(ctx, &mut rng);
            t.display_next(set);
        }
        s.lock_current(ctx);
        s.enter_next(ctx);
    });
    c
}

#[test]
fn html_export_contains_every_round() {
    let mut c = finished_competition(12);
    let html = c.dump_to_html();
    assert!(html.starts_with("<html>"));
    assert!(html.contains("Challenge de test"));
    // Un onglet par phase visible (le fournisseur d'entrée n'en a pas).
    assert_eq!(html.matches("OnTabClicked ('round_").count(), 4);
    assert!(html.contains("TIREUR00"), "le classement doit figurer dans la page");
    assert!(html.trim_end().ends_with("</html>"));
}

#[test]
fn html_export_final_classification_only() {
    let mut c = finished_competition(12);
    let html = c.dump_to_html_scope(None, true);
    assert_eq!(html.matches("OnTabClicked ('round_").count(), 1, "le classement final seul");
    assert!(html.contains("TIREUR00"));
}

#[test]
fn fff_export_lists_every_fencer() {
    let c = finished_competition(12);
    let bytes = c.export_final_classification_fff(0).expect("classement final");
    let text: String = bytes.iter().map(|b| *b as char).collect();
    let lines: Vec<&str> = text.lines().collect();
    assert!(lines[0].starts_with("FFF;WIN;competition;"));
    assert_eq!(lines.len(), 2 + 12, "2 lignes d'en-tête + un tireur par ligne");
    assert!(lines[2].contains("TIREUR") && lines[2].ends_with(";1,t"), "le premier a la place 1 : {}", lines[2]);
    assert!(lines[13].ends_with(";12,t"), "le dernier a la place 12 : {}", lines[13]);
}

#[test]
fn csv_export_can_be_imported_again() {
    let c = finished_competition(10);
    let csv = c.export_fencers_csv(';');
    let imported = bp_core::formats::csv::import(csv.as_bytes(), PlayerClass::Fencer);
    assert_eq!(imported.players.len(), 10);
    assert!(imported.players.players().any(|p| p.name() == "TIREUR03"));
}

#[test]
fn askfred_reads_referees_from_their_own_store() {
    let mut c = finished_competition(8);
    let fencer = c.players.iter().next().unwrap().reference;
    let mut referee = new_player(PlayerClass::Referee);
    referee.set_ref(fencer);
    referee.set_str(&AttrId::global("name"), "ARBITRE TEST");
    referee.set_str(&AttrId::global("first_name"), "Alice");
    assert_eq!(c.referees.add(referee), fencer);
    let before = c.to_xml();
    let xml = c.export_askfred();
    let doc = bp_core::xml::parse(&xml).unwrap();
    let referee = doc.descendants().find(|n| n.has_tag_name("Referee")).unwrap();
    assert_eq!(referee.attribute("LastName"), Some("ARBITRE TEST"));
    assert_eq!(referee.attribute("FirstName"), Some("Alice"));
    assert_eq!(doc.descendants().filter(|n| n.has_tag_name("Fencer")).count(), 8);
    assert_eq!(c.to_xml(), before, "un export ne modifie pas la competition");
}

#[test]
fn fff_start_place_does_not_change_the_final_ranks() {
    let c = finished_competition(8);
    let original = c.export_final_classification_fff(0).unwrap();
    let shifted = c.export_final_classification_fff(8).unwrap();
    let text: String = shifted.iter().map(|b| *b as char).collect();
    assert!(text.lines().nth(2).unwrap().ends_with(";9,t"));
    assert_eq!(c.export_final_classification_fff(0).unwrap(), original);
}

#[test]
fn recalculating_after_an_eliminated_export_does_not_shift_ranks_again() {
    let mut c = finished_with_quota(16, Some(8));
    let finalists = c.schedule.current().unwrap().base().attendees.clone();
    let eliminated = c.players.iter()
        .filter(|p| !finalists.contains(&p.reference))
        .min_by_key(|p| p.attr("final_rank").unwrap().as_uint())
        .unwrap().reference;
    c.players.get_mut(eliminated).unwrap().set_uint(&AttrId::global("exported"), 1);
    c.with_schedule(|s, ctx| s.refresh_general_classification(ctx));
    let first = c.export_final_classification_fff(0).unwrap();
    c.with_schedule(|s, ctx| s.refresh_general_classification(ctx));
    assert_eq!(c.export_final_classification_fff(0).unwrap(), first);
}

#[test]
fn finished_competition_is_saved_and_reopened() {
    let mut c = finished_competition(16);
    let dir = std::env::temp_dir().join(format!("bp_contest_{}", std::process::id()));
    let file = dir.join("fini.cotcot");
    c.filename = Some(file.clone());
    c.save(None, None).unwrap();
    c.archive("Table", None, None).unwrap();
    assert!(dir.join("fini.zip").exists(), "l'archive des étapes doit exister");

    let mut again = Contest::new(None, false);
    again.load_file(&file).unwrap();
    assert_eq!(again.name, "Challenge de test");
    assert!(again.schedule.is_over());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn points_follow_the_final_ranking_and_are_saved() {
    let mut c = finished_competition(12);
    assert!(c.final_points().is_empty(), "points désactivés par défaut");
    c.points.enabled = true;
    c.points.weight = 2.0;
    c.points.place_bonus = vec![bp_core::points::PlaceBonus { place: 1, points: 5.0 }];
    let points = c.final_points();
    assert_eq!(points.len(), 12);
    let first = points.iter().find(|(_, place, _)| *place == 1).unwrap();
    assert_eq!(first.2, (12.0 - 1.0 + 1.0 + 5.0) * 2.0);
    let last = points.iter().max_by_key(|(_, place, _)| *place).unwrap();
    assert_eq!(last.2, 2.0);
    let mut again = bp_core::contest::Contest::new(None, false);
    again.load_xml_str(&c.to_xml()).unwrap();
    assert_eq!(again.points, c.points);
}
