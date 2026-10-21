//! Classement final, impression et exports de la competition.

use std::path::PathBuf;
use std::rc::{Rc, Weak};

use bp_core::{
    i18n::tr,
    rounds::classification::{ExportType, GeneralClassificationStage},
    stage::{Stage, StageKind},
};
use bp_gtk::{dialogs, glade::Glade};
use gtk::prelude::*;
use gtk4 as gtk;

use super::StageScreen;
use crate::contest_page::ContestPage;
use crate::widgets::{
    filter_dialog,
    list_document::{self, ListDocument},
    players_list::PlayersListView,
    render,
};

pub struct ClassificationScreen {
    page: Weak<ContestPage>,
    index: usize,
    glade: Glade,
    root: gtk::Widget,
    list: PlayersListView,
}

impl ClassificationScreen {
    pub fn new(page: &Rc<ContestPage>, index: usize) -> Rc<Self> {
        let glade = Glade::new("general_classification.ui").expect("general_classification.ui");
        let root: gtk::Widget = glade.get("root");
        Glade::detach_from_parent(&root);
        let list = PlayersListView::new();
        glade
            .get::<gtk::Viewport>("classification_hook")
            .set_child(Some(list.widget()));
        let shift: gtk::Widget = glade.get("place_shifting");
        Glade::detach_from_parent(&shift);
        glade
            .get::<gtk::FileChooserDialog>("filechooserdialog")
            .content_area()
            .append(&shift);
        let screen = Rc::new(Self {
            page: Rc::downgrade(page),
            index,
            glade,
            root,
            list,
        });
        let weak = Rc::downgrade(&screen);
        screen.glade.connect_simple(
            "on_general_classification_filter_button_clicked",
            move |_| {
                if let Some(s) = weak.upgrade() {
                    s.on_filter();
                }
            },
        );
        let weak = Rc::downgrade(&screen);
        screen.glade.connect_simple(
            "on_general_classification_print_toolbutton_clicked",
            move |_| {
                if let Some(s) = weak.upgrade() {
                    s.on_print();
                }
            },
        );
        for (handler, format) in [
            ("on_export_html_toolbutton_clicked", ExportType::Html),
            ("on_export_fff_toolbutton_clicked", ExportType::Fff),
            ("on_export_pdf_toolbutton_clicked", ExportType::Pdf),
            ("on_export_frd_toolbutton_clicked", ExportType::Frd),
        ] {
            let weak = Rc::downgrade(&screen);
            screen.glade.connect_simple(handler, move |_| {
                if let Some(s) = weak.upgrade() {
                    s.on_export(format);
                }
            });
        }
        screen.refresh();
        screen
    }

    fn document(&self) -> Option<ListDocument> {
        self.page
            .upgrade()?
            .with_contest(|c| {
                let stage = c
                    .schedule
                    .stage(self.index)?
                    .as_any()
                    .downcast_ref::<GeneralClassificationStage>()?;
                let specs = render::column_specs(stage.filter.layout(), false);
                let rows = list_document::rows_for(
                    &c.players,
                    stage.result(),
                    &specs,
                    stage.base().owner(),
                );
                Some(ListDocument {
                    title: c.name.clone(),
                    subtitle: tr("General classification"),
                    specs,
                    rows,
                    classification_borders: true,
                })
            })
            .flatten()
    }

    fn on_filter(&self) {
        let Some(page) = self.page.upgrade() else {
            return;
        };
        let filter = page
            .with_contest(|c| {
                c.schedule
                    .stage(self.index)?
                    .as_any()
                    .downcast_ref::<GeneralClassificationStage>()
                    .map(|s| s.filter.clone())
            })
            .flatten();
        let Some(mut filter) = filter else { return };
        if filter_dialog::select_attributes(page.window().as_ref(), &mut filter, None) {
            page.with_contest(|c| {
                if let Some(stage) = c
                    .schedule
                    .stage_mut(self.index)
                    .and_then(|s| s.as_any_mut().downcast_mut::<GeneralClassificationStage>())
                {
                    stage.base_mut().filter = filter.selected_codes();
                    stage.base_mut().classification_filter = filter.selected_codes();
                    stage.filter = filter;
                }
            });
            page.mark_dirty();
            self.refresh();
        }
    }

    /// Impression : le classement final seul, ou les résultats complets de
    /// l'événement (poules, tableaux, puis classement final).
    fn on_print(&self) {
        let Some(page) = self.page.upgrade() else {
            return;
        };
        let parent = page.window();
        let dialog = gtk::Dialog::new();
        dialog.set_title(Some(&tr("Print")));
        dialog.set_transient_for(parent.as_ref());
        dialog.set_resizable(false);
        let content = dialog.content_area();
        content.set_spacing(6);
        content.set_margin_top(12);
        content.set_margin_bottom(12);
        content.set_margin_start(12);
        content.set_margin_end(12);
        let final_radio = gtk::CheckButton::with_label(&tr("Final classification"));
        let complete_radio = gtk::CheckButton::with_label(&tr("Complete results"));
        complete_radio.set_group(Some(&final_radio));
        final_radio.set_active(true);
        content.append(&final_radio);
        content.append(&complete_radio);
        dialog.add_button(&tr("_Cancel"), gtk::ResponseType::Cancel);
        dialog.add_button(&tr("_OK"), gtk::ResponseType::Ok).add_css_class("suggested-action");
        let accepted = dialogs::run_dialog(&dialog) == gtk::ResponseType::Ok;
        let complete = complete_radio.is_active();
        dialog.destroy();
        if !accepted {
            return;
        }
        if !complete {
            let Some(document) = self.document() else {
                return;
            };
            if let Err(e) = document.print(parent.as_ref()) {
                dialogs::error(parent.as_ref(), &e.to_string());
            }
            return;
        }
        let weak = self.page.clone();
        let index = self.index;
        let final_document = self.document();
        let job = page.with_contest(|c| c.name.clone()).unwrap_or_default();
        let result = bp_gtk::print::run_document(parent.as_ref(), &job, move || {
            let mut parts: Vec<Box<dyn bp_gtk::print::PageSource>> = Vec::new();
            if let Some(page) = weak.upgrade() {
                page.with_contest(|c| {
                    for i in 0..index {
                        let kind = c.schedule.stage(i).map(|s| s.kind());
                        match kind {
                            Some(StageKind::PoolSupervisor) => parts.extend(super::pool_supervisor::results_sources(c, i)),
                            Some(StageKind::TableSupervisor) => parts.extend(super::table_supervisor::results_sources(c, i)),
                            _ => {}
                        }
                    }
                });
            }
            if let Some(document) = final_document.as_ref() {
                parts.push(document.source());
            }
            Box::new(bp_gtk::print::Concat::new(parts))
        });
        if let Err(e) = result {
            dialogs::error(parent.as_ref(), &e.to_string());
        }
    }

    fn on_export(&self, format: ExportType) {
        let Some(page) = self.page.upgrade() else {
            return;
        };
        let Some(app) = page.app.upgrade() else {
            return;
        };
        let chooser: gtk::FileChooserDialog = self.glade.get("filechooserdialog");
        chooser.set_transient_for(page.window().as_ref());
        self.glade
            .get::<gtk::Widget>("place_shifting")
            .set_visible(format == ExportType::Fff);
        let (extension, label) = format.pattern();
        let filters = chooser.filters();
        for i in (0..filters.n_items()).rev() {
            if let Some(filter) = filters
                .item(i)
                .and_then(|f| f.downcast::<gtk::FileFilter>().ok())
            {
                chooser.remove_filter(&filter);
            }
        }
        let filter = dialogs::file_filter(Some(&tr(label)), &[&format!("*.{extension}")]);
        chooser.add_filter(&filter);
        chooser.set_filter(&filter);
        chooser.add_filter(&dialogs::all_files_filter());
        let dir = {
            let t = app.tournament.borrow();
            t.config
                .get_string("Competiton", "export_dir")
                .or_else(|| t.config.get_string("Competiton", "default_dir_name"))
                .map(PathBuf::from)
        };
        if let Some(dir) = dir.filter(|d| d.is_dir()) {
            let _ = chooser.set_current_folder(Some(&gtk::gio::File::for_path(dir)));
        }
        if let Some(name) = page.with_contest(|c| {
            GeneralClassificationStage::export_file_name(&c.default_file_name(), format)
        }) {
            chooser.set_current_name(&name);
        }
        let accepted = dialogs::run_dialog(&chooser) == gtk::ResponseType::Accept;
        let path = chooser.file().and_then(|f| f.path());
        chooser.hide();
        if !accepted {
            return;
        }
        let Some(mut path) = path else { return };
        if path.extension().is_none() {
            path.set_extension(extension);
        }
        let offset = GeneralClassificationStage::place_shifting_from_entry(
            &self.glade.get::<gtk::Entry>("place_entry").text(),
        )
        .saturating_sub(1);
        let result = match format {
            ExportType::Pdf => self
                .document()
                .ok_or_else(|| tr("Competition not found"))
                .and_then(|d| d.export_pdf(&path)),
            _ => {
                let bytes = page
                    .with_contest(|c| match format {
                        ExportType::Html => Some(c.dump_to_html().into_bytes()),
                        ExportType::Fff => c.export_final_classification_fff(offset),
                        ExportType::Frd => Some(c.export_askfred().into_bytes()),
                        ExportType::Pdf => unreachable!(),
                    })
                    .flatten();
                bytes
                    .ok_or_else(|| tr("Competition not found"))
                    .and_then(|b| std::fs::write(&path, b).map_err(|e| e.to_string()))
            }
        };
        if let Err(e) = result {
            dialogs::error(page.window().as_ref(), &e);
            return;
        }
        if let Some(dir) = path.parent() {
            let mut t = app.tournament.borrow_mut();
            t.config
                .set_string("Competiton", "export_dir", &dir.to_string_lossy());
            if let Err(e) = t.save_config() {
                log::warn!("export_dir: {e}");
            }
        }
    }
}

impl StageScreen for ClassificationScreen {
    fn widget(&self) -> gtk::Widget {
        self.root.clone()
    }
    fn refresh(&self) {
        let Some(document) = self.document() else {
            return;
        };
        self.list.set_columns(document.specs);
        self.list.set_rows(document.rows);
        self.list.set_locked(true);
    }
}
