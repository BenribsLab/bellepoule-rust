//! Tireurs à extraire et libellé de la compétition issue du point de scission.

use bp_core::{filter::Filter, i18n::tr, rounds::splitting::Splitting, stage::Stage};
use bp_gtk::{dialogs, glade::Glade};
use gtk::prelude::*;
use gtk4 as gtk;
use std::cell::Cell;
use std::rc::{Rc, Weak};

use super::StageScreen;
use crate::contest_page::ContestPage;
use crate::widgets::{
    filter_dialog,
    list_document::{self, ListDocument},
    players_list::PlayersListView,
    render,
};

pub struct SplittingScreen {
    page: Weak<ContestPage>,
    index: usize,
    glade: Glade,
    root: gtk::Widget,
    configuration: gtk::Widget,
    name: gtk::Entry,
    list: PlayersListView,
    updating: Cell<bool>,
}

impl SplittingScreen {
    pub fn new(page: &Rc<ContestPage>, index: usize) -> Rc<Self> {
        let glade = Glade::new("splitting.ui").expect("splitting.ui");
        let root: gtk::Widget = glade.get("root");
        let configuration: gtk::Widget = glade.get("stage_configuration");
        Glade::detach_from_parent(&root);
        Glade::detach_from_parent(&configuration);
        let list = PlayersListView::in_scrolled(glade.get("scrolledwindow3"));
        let name = glade
            .get::<gtk::ComboBox>("name_combobox")
            .child()
            .and_then(|w| w.downcast::<gtk::Entry>().ok())
            .expect("name_combobox entry");
        let screen = Rc::new(Self {
            page: Rc::downgrade(page),
            index,
            glade,
            root,
            configuration,
            name,
            list,
            updating: Cell::new(false),
        });
        let weak = Rc::downgrade(&screen);
        screen.name.connect_changed(move |_| {
            if let Some(s) = weak.upgrade() {
                s.on_name_changed();
            }
        });
        let weak = Rc::downgrade(&screen);
        screen
            .glade
            .connect_simple("on_splitting_filter_toolbutton_clicked", move |_| {
                if let Some(s) = weak.upgrade() {
                    s.on_filter();
                }
            });
        let weak = Rc::downgrade(&screen);
        screen
            .glade
            .connect_simple("on_splitting_print_toolbutton_clicked", move |_| {
                if let Some(s) = weak.upgrade() {
                    s.on_print();
                }
            });
        let weak = Rc::downgrade(&screen);
        screen.list.connect_toggled(move |targets, code, value| {
            let Some(s) = weak.upgrade() else { return };
            if code != "exported" || s.updating.get() {
                return;
            }
            let Some(page) = s.page.upgrade() else { return };
            let changed = page
                .with_contest(|c| {
                    c.with_schedule(|schedule, ctx| {
                        let stage = schedule
                            .stage_mut(s.index)?
                            .as_any_mut()
                            .downcast_mut::<Splitting>()?;
                        if stage.base().locked {
                            return None;
                        }
                        let refs: Vec<_> = targets
                            .iter()
                            .copied()
                            .filter(|r| stage.base().attendees.contains(r))
                            .collect();
                        for r in refs {
                            stage.set_exported(ctx, r, value);
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
        });
        screen.refresh();
        screen
    }

    fn filter() -> Filter {
        Filter::excluding(
            "splitting",
            &[
                "ref",
                "plugin_ID",
                "incident",
                "IP",
                "password",
                "cyphered_password",
                "HS",
                "attending",
                "final_rank",
                "global_status",
                "indice",
                "level",
                "workload_rate",
                "pool_nr",
                "promoted",
                "rank",
                "status",
                "team",
                "victories_count",
                "bouts_count",
                "victories_ratio",
                "strip",
                "time",
            ],
        )
    }

    fn document(&self) -> Option<(ListDocument, bool, String)> {
        self.page
            .upgrade()?
            .with_contest(|c| {
                let stage = c
                    .schedule
                    .stage(self.index)?
                    .as_any()
                    .downcast_ref::<Splitting>()?;
                let base = stage.base();
                let mut filter = Self::filter();
                filter.apply_list(&base.filter);
                let mut specs = render::column_specs(filter.layout(), !base.locked);
                list_document::make_editable(&mut specs, "exported", !base.locked);
                let rows =
                    list_document::rows_for(&c.players, &base.attendees, &specs, base.owner());
                Some((
                    ListDocument {
                        title: c.name.clone(),
                        subtitle: tr("List of fencers to extract"),
                        specs,
                        rows,
                        classification_borders: false,
                    },
                    base.locked,
                    base.name.clone(),
                ))
            })
            .flatten()
    }

    fn on_name_changed(&self) {
        if self.updating.get() {
            return;
        }
        let Some(page) = self.page.upgrade() else {
            return;
        };
        let changed = page
            .with_contest(|c| {
                let stage = c
                    .schedule
                    .stage_mut(self.index)?
                    .as_any_mut()
                    .downcast_mut::<Splitting>()?;
                if stage.base().locked {
                    return None;
                }
                stage.base_mut().name = self.name.text().to_string();
                Some(())
            })
            .flatten()
            .is_some();
        if changed {
            page.mark_dirty();
        }
    }

    fn on_filter(&self) {
        let Some(page) = self.page.upgrade() else {
            return;
        };
        let current = page
            .with_contest(|c| {
                c.schedule
                    .stage(self.index)
                    .map(|s| s.base().filter.clone())
            })
            .flatten();
        let Some(current) = current else { return };
        let mut filter = Self::filter();
        filter.apply_list(&current);
        if filter_dialog::select_attributes(page.window().as_ref(), &mut filter, None) {
            page.with_contest(|c| {
                if let Some(s) = c.schedule.stage_mut(self.index) {
                    s.base_mut().filter = filter.selected_codes();
                }
            });
            page.mark_dirty();
            self.refresh();
        }
    }

    fn on_print(&self) {
        let Some((document, _, _)) = self.document() else {
            return;
        };
        let parent = self.page.upgrade().and_then(|p| p.window());
        if let Err(e) = document.print(parent.as_ref()) {
            dialogs::error(parent.as_ref(), &e.to_string());
        }
    }
}

impl StageScreen for SplittingScreen {
    fn widget(&self) -> gtk::Widget {
        self.root.clone()
    }
    fn configuration(&self) -> Option<gtk::Widget> {
        Some(self.configuration.clone())
    }
    fn refresh(&self) {
        let Some((document, locked, name)) = self.document() else {
            return;
        };
        self.updating.set(true);
        self.list.set_columns(document.specs);
        self.list.set_rows(document.rows);
        self.list.set_locked(locked);
        if self.name.text() != name {
            self.name.set_text(&name);
        }
        self.configuration.set_sensitive(!locked);
        self.updating.set(false);
    }
}
