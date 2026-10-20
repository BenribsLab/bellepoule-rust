//! Contrats du modèle utilisés par le nouvel écran et ses impressions.

use bp_core::attribute::Value;
use bp_core::bout::StartTime;
use bp_core::contest::{players::new_player, Contest};
use bp_core::player::{AttrId, PlayerClass};
use bp_core::rounds::table::{LayoutMetrics, PrintSession, PrintType, Roadmap, TableSupervisor};
use bp_core::stage::Stage;

fn competition(count: u32) -> Contest {
    let mut contest = Contest::new(None, false);
    contest.create_default_schedule(true);
    for i in 0..count {
        let mut player = new_player(PlayerClass::Fencer);
        player.set_str(&AttrId::global("name"), &format!("TIREUR{i:03}"));
        player.set_uint(&AttrId::global("ranking"), i + 1);
        player.set(&AttrId::global("attending"), Value::Bool(true));
        contest.add_fencer(player).unwrap();
    }
    contest.with_schedule(|schedule, ctx| {
        schedule.lock_current(ctx);
        schedule.enter_next(ctx);
    });
    contest
}

#[test]
fn drawing_and_score_sheet_use_the_separate_referee_store() {
    let mut contest = competition(8);
    // Collision volontaire : la référence 1 est aussi celle d'un tireur.
    let mut referee = new_player(PlayerClass::Referee);
    referee.set_ref(1);
    referee.set_str(&AttrId::global("name"), "ARBITRE");
    referee.set_str(&AttrId::global("first_name"), "Camille");
    let referee = contest.referees.add(referee);
    let referees = contest.referees.store().clone();
    contest.with_schedule(|schedule, ctx| {
        let stage = schedule
            .stage_mut(1)
            .unwrap()
            .as_any_mut()
            .downcast_mut::<TableSupervisor>()
            .unwrap();
        let set_index = stage.displayed_table_set().unwrap();
        let node = stage
            .table_set(set_index)
            .quick_search(ctx.players)
            .into_iter()
            .flat_map(|t| t.matches)
            .next()
            .unwrap()
            .node;
        let roadmap = Roadmap {
            piste: 3,
            start_time: StartTime::parse("20.10.2026", "14:20"),
            duration_span: 1,
            referees: vec![referee],
        };
        stage.apply_roadmap(set_index, node, Some(&roadmap));
        let set = stage.table_set(set_index);
        let drawing = set.drawing_with_referees(ctx.players, &referees, &LayoutMetrics::default());
        let roadmap = drawing
            .cell(node)
            .unwrap()
            .match_label
            .as_ref()
            .unwrap()
            .roadmap
            .as_ref()
            .unwrap();
        assert!(roadmap.referees[0].1.contains("ARBITRE"));
        assert!(!roadmap.referees[0].1.contains("TIREUR"));
        let sheet = set
            .score_sheet_with_referees(node, ctx.players, &referees, 11, stage.base().net_id)
            .unwrap();
        assert_eq!(sheet.referee, Some(("ARBITRE".into(), "Camille".into())));
        assert!(sheet.flash_ref.unwrap().starts_with("#b/"));
    });
}

#[test]
fn bracket_pagination_covers_all_rows_at_each_density() {
    let mut contest = competition(64);
    contest.with_schedule(|schedule, _| {
        let stage = schedule
            .stage(1)
            .unwrap()
            .as_any()
            .downcast_ref::<TableSupervisor>()
            .unwrap();
        for rows in [8, 16, 32] {
            let mut set = stage
                .table_set(stage.displayed_table_set().unwrap())
                .clone();
            let size = set.table(set.from_table()).size();
            let mut session =
                PrintSession::new(PrintType::DisplayedTables, "Test", Some(&set), None);
            let pages = set.prepare_print(
                &mut session,
                rows,
                &LayoutMetrics::default(),
                550.0,
                780.0,
                60.0,
            );
            assert_eq!(pages, size.div_ceil(rows) as usize);
            assert!(session.scale().is_finite() && session.scale() > 0.0);
            let bounds: Vec<_> = (0..pages)
                .map(|p| session.page_bounds(p).unwrap())
                .collect();
            assert!(bounds.iter().all(|b| b.x2 > b.x1 && b.y2 > b.y1));
            assert!(bounds.windows(2).all(|b| b[0].y2 <= b[1].y1));
        }
    });
}

#[test]
fn printing_a_match_only_removes_it_from_the_pending_selection() {
    let mut contest = competition(8);
    contest.with_schedule(|schedule, _| {
        let stage = schedule
            .stage_mut(1)
            .unwrap()
            .as_any_mut()
            .downcast_mut::<TableSupervisor>()
            .unwrap();
        let set = stage.table_set_mut(stage.displayed_table_set().unwrap());
        let all = set.matches_to_print(None, true);
        assert_eq!(all.len(), 4);
        set.mark_printed(all[0]);
        assert_eq!(set.matches_to_print(None, true), all);
        let pending = set.matches_to_print(None, false);
        assert_eq!(pending.len(), 3);
        assert!(!pending.contains(&all[0]));
    });
}
