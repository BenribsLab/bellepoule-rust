//! Départage des ex æquo à la limite du quota, comme `barrage.cpp`.

use bp_core::{filter::Filter, i18n::tr, rounds::barrage::Barrage, stage::Stage};
use bp_gtk::{dialogs, glade::Glade};
use gtk::prelude::*;
use gtk4 as gtk;
use std::rc::{Rc, Weak};

use super::StageScreen;
use crate::contest_page::ContestPage;
use crate::widgets::{
    filter_dialog,
    list_document::{self, ListDocument},
    players_list::PlayersListView,
    render,
};

struct Snapshot {
    main: ListDocument,
    classification: ListDocument,
    locked: bool,
    qualified: u32,
    dropped: u32,
}

pub struct BarrageScreen {
    page: Weak<ContestPage>,
    index: usize,
    glade: Glade,
    root: gtk::Widget,
    list: PlayersListView,
    classification: PlayersListView,
}

impl BarrageScreen {
    pub fn new(page: &Rc<ContestPage>, index: usize) -> Rc<Self> {
        let glade = Glade::new("barrage.ui").expect("barrage.ui");
        let root: gtk::Widget = glade.get("root");
        Glade::detach_from_parent(&root);
        let list = PlayersListView::in_scrolled(glade.get("main_hook"));
        let classification = PlayersListView::new();
        glade
            .get::<gtk::Viewport>("classification_hook")
            .set_child(Some(classification.widget()));
        let screen = Rc::new(Self {
            page: Rc::downgrade(page),
            index,
            glade,
            root,
            list,
            classification,
        });
        macro_rules! on {
            ($handler:expr, $method:ident) => {{
                let weak = Rc::downgrade(&screen);
                screen.glade.connect_simple($handler, move |_| {
                    if let Some(s) = weak.upgrade() {
                        s.$method();
                    }
                });
            }};
        }
        on!(
            "on_barrage_classification_toggletoolbutton_toggled",
            toggle_classification
        );
        on!("on_barrage_filter_toolbutton_clicked", on_filter);
        on!("on_barrage_print_toolbutton_clicked", on_print);
        let weak = Rc::downgrade(&screen);
        screen.list.connect_toggled(move |targets, code, value| {
            let Some(s) = weak.upgrade() else { return };
            if code != "promoted" {
                return;
            }
            let Some(page) = s.page.upgrade() else { return };
            let changed = page
                .with_contest(|c| {
                    c.with_schedule(|schedule, ctx| {
                        let stage = schedule
                            .stage_mut(s.index)?
                            .as_any_mut()
                            .downcast_mut::<Barrage>()?;
                        if stage.base().locked {
                            return None;
                        }
                        for player in targets
                            .iter()
                            .filter(|p| stage.barrage_players().contains(p))
                            .copied()
                            .collect::<Vec<_>>()
                        {
                            stage.set_promoted(ctx, player, value);
                        }
                        Some(())
                    })
                })
                .flatten()
                .is_some();
            if changed {
                page.mark_dirty();
                s.refresh();
            }
            let schedule = page.schedule.borrow().clone();
            if let Some(schedule) = schedule {
                schedule.refresh_sensitivity();
            }
        });
        screen.refresh();
        screen
    }

    fn filter(classification: bool) -> Filter {
        let mut excluded = vec![
            "ref",
            "plugin_ID",
            "incident",
            "IP",
            "password",
            "cyphered_password",
            "HS",
            "attending",
            "exported",
            "final_rank",
            "global_status",
            "indice",
            "level",
            "workload_rate",
            "pool_nr",
            "team",
            "victories_count",
            "bouts_count",
            "victories_ratio",
            "strip",
            "time",
        ];
        if classification {
            excluded.push("promoted");
        } else {
            excluded.extend(["rank", "status"]);
        }
        Filter::excluding(
            if classification {
                "barrage_classification"
            } else {
                "barrage"
            },
            &excluded,
        )
    }

    fn snapshot(&self) -> Option<Snapshot> {
        self.page
            .upgrade()?
            .with_contest(|c| {
                let title = c.name.clone();
                c.with_schedule(|schedule, ctx| {
                    let stage = schedule
                        .stage(self.index)?
                        .as_any()
                        .downcast_ref::<Barrage>()?;
                    let base = stage.base();
                    let mut filter = Self::filter(false);
                    filter.apply_list(&base.filter);
                    let mut specs = render::column_specs(filter.layout(), !base.locked);
                    list_document::make_editable(&mut specs, "promoted", !base.locked);
                    let rows = list_document::rows_for(
                        ctx.players,
                        stage.barrage_players(),
                        &specs,
                        base.owner(),
                    );
                    let main = ListDocument {
                        title: title.clone(),
                        subtitle: tr("Barrage"),
                        specs,
                        rows,
                        classification_borders: false,
                    };
                    let mut filter = Self::filter(true);
                    filter.apply_list(&base.classification_filter);
                    let specs = render::column_specs(filter.layout(), false);
                    let rows = list_document::ranked_rows(
                        ctx.players,
                        &stage.classification(ctx),
                        &specs,
                        base.owner(),
                    );
                    let classification = ListDocument {
                        title,
                        subtitle: tr("Classification"),
                        specs,
                        rows,
                        classification_borders: true,
                    };
                    Some(Snapshot {
                        main,
                        classification,
                        locked: base.locked,
                        qualified: stage.qualified_count(ctx.players),
                        dropped: stage.dropped_count(ctx.players),
                    })
                })
            })
            .flatten()
    }

    fn is_classification(&self) -> bool {
        self.glade
            .get::<gtk::ToggleButton>("barrage_classification_toggletoolbutton")
            .is_active()
    }
    fn toggle_classification(&self) {
        let classification = self.is_classification();
        self.glade
            .get::<gtk::Widget>("main_hook")
            .set_visible(!classification);
        self.glade
            .get::<gtk::Widget>("classification_hook")
            .set_visible(classification);
    }
    fn on_filter(&self) {
        let Some(page) = self.page.upgrade() else {
            return;
        };
        let classification = self.is_classification();
        let current = page
            .with_contest(|c| {
                let stage = c.schedule.stage(self.index)?;
                Some(if classification {
                    stage.base().classification_filter.clone()
                } else {
                    stage.base().filter.clone()
                })
            })
            .flatten();
        let Some(current) = current else { return };
        let mut filter = Self::filter(classification);
        filter.apply_list(&current);
        if filter_dialog::select_attributes(page.window().as_ref(), &mut filter, None) {
            page.with_contest(|c| {
                if let Some(stage) = c.schedule.stage_mut(self.index) {
                    if classification {
                        stage.base_mut().classification_filter = filter.selected_codes();
                    } else {
                        stage.base_mut().filter = filter.selected_codes();
                    }
                }
            });
            page.mark_dirty();
            self.refresh();
        }
    }
    fn on_print(&self) {
        let Some(snapshot) = self.snapshot() else {
            return;
        };
        let parent = self.page.upgrade().and_then(|p| p.window());
        let document = if self.is_classification() {
            snapshot.classification
        } else {
            snapshot.main
        };
        if let Err(e) = document.print(parent.as_ref()) {
            dialogs::error(parent.as_ref(), &e.to_string());
        }
    }
}

impl StageScreen for BarrageScreen {
    fn widget(&self) -> gtk::Widget {
        self.root.clone()
    }
    fn refresh(&self) {
        let Some(s) = self.snapshot() else { return };
        self.list.set_columns(s.main.specs);
        self.list.set_rows(s.main.rows);
        self.list.set_locked(s.locked);
        self.classification.set_columns(s.classification.specs);
        self.classification.set_rows(s.classification.rows);
        self.glade
            .get::<gtk::Label>("promoted_label")
            .set_text(&s.qualified.to_string());
        self.glade
            .get::<gtk::Label>("dropped_label")
            .set_text(&s.dropped.to_string());
        self.toggle_classification();
    }
}
