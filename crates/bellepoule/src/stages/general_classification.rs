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
        let page = self.page.upgrade()?;
        if let Some(Some(doc)) = page.with_contest(Self::youth_document) {
            return Some(doc);
        }
        self.page
            .upgrade()?
            .with_contest(|c| {
                let stage = c
                    .schedule
                    .stage(self.index)?
                    .as_any()
                    .downcast_ref::<GeneralClassificationStage>()?;
                let mut specs = render::column_specs(stage.filter.layout(), false);
                let mut rows = list_document::rows_for(
                    &c.players,
                    stage.result(),
                    &specs,
                    stage.base().owner(),
                );
                // Points de la compétition (préférences « Points »).
                if c.points.enabled {
                    let points = c.final_points();
                    specs.push(crate::widgets::players_list::ColumnSpec {
                        code: "points".into(),
                        title: tr("Points"),
                        look: bp_core::attribute::Look::LongText,
                        kind: crate::widgets::players_list::CellKind::Text,
                        editable: false,
                    });
                    for row in rows.iter_mut() {
                        let p = points.iter().find(|(r, _, _)| *r == row.player).map(|(_, _, p)| *p);
                        row.cells.push(crate::widgets::players_list::Cell::Text(p.map(bp_core::points::image).unwrap_or_default()));
                        row.sort.push(p.map(|v| bp_core::attribute::Value::Int(-(v * 100.0) as i32)));
                    }
                }
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

    /// Entraînement des jeunes : le classement de chaque groupe, groupe par
    /// groupe, avec les victoires et les points.
    fn youth_document(c: &mut bp_core::contest::Contest) -> Option<ListDocument> {
        use crate::widgets::players_list::{Cell, CellKind, ColumnSpec, RowData};
        use bp_core::attribute::{Look, Value};
        let results = bp_core::rounds::youth::results(&c.schedule, &c.players)?;
        let column = |code: &str, title: String| ColumnSpec { code: code.into(), title, look: Look::LongText, kind: CellKind::Text, editable: false };
        let specs = vec![
            column("youth_group", tr("Group")),
            column("rank", tr("Place in the group")),
            column("name", tr("last name")),
            column("first_name", tr("first name")),
            column("club", tr("club")),
            column("victories_count", tr("Victories")),
            column("points", tr("Points")),
        ];
        // Une ligne de tireur ; `place` : dans le groupe ou au général.
        let row = |r: &bp_core::rounds::youth::YouthResult, place: Option<u32>| -> Option<RowData> {
            let p = c.players.get(r.player)?;
            let text = |code: &str| p.str_attr(code).unwrap_or_default().to_string();
            let opt = |v: Option<u32>| v.map(|n| n.to_string()).unwrap_or_default();
            Some(RowData {
                player: r.player,
                cells: vec![
                    Cell::Text(r.group.to_string()),
                    Cell::Text(opt(place)),
                    Cell::Text(p.name()),
                    Cell::Text(text("first_name")),
                    Cell::Text(text("club")),
                    Cell::Text(r.victories.to_string()),
                    Cell::Text(opt(r.points)),
                ],
                sort: vec![
                    Some(Value::UInt(r.group)),
                    Some(Value::UInt(place.unwrap_or(u32::MAX))),
                    Some(Value::Str(p.name())),
                    Some(Value::Str(text("first_name"))),
                    Some(Value::Str(text("club"))),
                    Some(Value::UInt(r.victories)),
                    Some(Value::UInt(r.points.unwrap_or(0))),
                ],
                children: Vec::new(),
                dimmed: false,
            })
        };
        // Ligne de titre d'une section, ses tireurs en sous-lignes.
        let section = |title: String, children: Vec<RowData>| RowData {
            player: 0,
            cells: std::iter::once(Cell::Text(title)).chain((1..specs.len()).map(|_| Cell::Text(String::new()))).collect(),
            sort: specs.iter().map(|_| None).collect(),
            children,
            dimmed: false,
        };
        let mut rows = Vec::new();
        let mut groups: Vec<u32> = results.iter().map(|r| r.group).collect();
        groups.dedup();
        for g in groups {
            let members = results.iter().filter(|r| r.group == g).filter_map(|r| row(r, r.place)).collect();
            rows.push(section(format!("{} {}", tr("Group"), g), members));
        }
        // Tous les groupes mélangés : points, puis victoires ; ex æquo à égalité.
        let mut all: Vec<&bp_core::rounds::youth::YouthResult> = results.iter().collect();
        all.sort_by_key(|r| (std::cmp::Reverse(r.points.unwrap_or(0)), std::cmp::Reverse(r.victories), r.place.unwrap_or(u32::MAX)));
        let mut general = Vec::new();
        let mut place = 0u32;
        let mut previous: Option<(u32, u32)> = None;
        for (i, r) in all.iter().enumerate() {
            let key = (r.points.unwrap_or(0), r.victories);
            if previous != Some(key) {
                place = i as u32 + 1;
                previous = Some(key);
            }
            if let Some(line) = row(r, Some(place)) {
                general.push(line);
            }
        }
        rows.push(section(tr("General classification (all groups)"), general));
        Some(ListDocument {
            title: c.name.clone(),
            subtitle: tr("Youth training — ranking of each group"),
            specs,
            rows,
            classification_borders: false,
        })
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

    /// Choix du contenu : `Some(true)` pour les résultats complets,
    /// `Some(false)` pour le classement final seul, `None` si annulé.
    /// Entraînement des jeunes : les résultats complets par défaut.
    fn ask_scope(&self, page: &ContestPage, title: &str) -> Option<bool> {
        let parent = page.window();
        let dialog = gtk::Dialog::new();
        dialog.set_title(Some(title));
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
        let youth = page.with_contest(|c| c.youth_pool_stage()).flatten().is_some();
        complete_radio.set_active(youth);
        final_radio.set_active(!youth);
        content.append(&final_radio);
        content.append(&complete_radio);
        dialog.add_button(&tr("_Cancel"), gtk::ResponseType::Cancel);
        dialog.add_button(&tr("_OK"), gtk::ResponseType::Ok).add_css_class("suggested-action");
        let accepted = dialogs::run_dialog(&dialog) == gtk::ResponseType::Ok;
        let complete = complete_radio.is_active();
        dialog.destroy();
        accepted.then_some(complete)
    }

    /// Pages à imprimer ou à exporter en PDF : le classement final, précédé
    /// des poules et des tableaux (résultats complets). Entraînement des
    /// jeunes : le classement, puis chaque groupe avec ses poules et son
    /// tableau remplis (matchs de classement compris).
    fn pages(page: &Weak<ContestPage>, index: usize, final_document: Option<&ListDocument>, complete: bool) -> Box<dyn bp_gtk::print::PageSource> {
        let mut parts: Vec<Box<dyn bp_gtk::print::PageSource>> = Vec::new();
        let page = page.upgrade();
        let youth = page.as_ref().and_then(|p| p.with_contest(|c| c.youth_pool_stage()).flatten());
        if complete && youth.is_none() {
            if let Some(page) = &page {
                page.with_contest(|c| {
                    for i in 0..index {
                        match c.schedule.stage(i).map(|s| s.kind()) {
                            Some(StageKind::PoolSupervisor) => parts.extend(super::pool_supervisor::results_sources(c, i)),
                            Some(StageKind::TableSupervisor) => parts.extend(super::table_supervisor::results_sources(c, i)),
                            _ => {}
                        }
                    }
                });
            }
        }
        if let Some(document) = final_document {
            parts.push(document.source());
        }
        if let (true, Some(pools), Some(page)) = (complete, youth, &page) {
            if let Some(groups) = super::group_print::source(page, pools) {
                parts.push(groups);
            }
        }
        Box::new(bp_gtk::print::Concat::new(parts))
    }

    /// Impression : le classement final seul, ou les résultats complets de
    /// l'événement.
    fn on_print(&self) {
        let Some(page) = self.page.upgrade() else {
            return;
        };
        let Some(complete) = self.ask_scope(&page, &tr("Print")) else {
            return;
        };
        let parent = page.window();
        let (weak, index, final_document) = (self.page.clone(), self.index, self.document());
        let job = page.with_contest(|c| c.name.clone()).unwrap_or_default();
        let result = bp_gtk::print::run_document(parent.as_ref(), &job, move || {
            Self::pages(&weak, index, final_document.as_ref(), complete)
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
        let complete = match format {
            ExportType::Html | ExportType::Pdf => match self.ask_scope(&page, &tr("Export")) {
                Some(complete) => complete,
                None => return,
            },
            _ => true,
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
            ExportType::Pdf => {
                let document = self.document();
                bp_gtk::print::export_pdf(Self::pages(&self.page, self.index, document.as_ref(), complete).as_mut(), &path)
            }
            _ => {
                let youth = page.with_contest(Self::youth_document).flatten().map(|d| d.to_html());
                let bytes = page
                    .with_contest(|c| match format {
                        ExportType::Html => Some(c.dump_to_html_scope(youth, !complete).into_bytes()),
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
        self.list.expand_all();
        self.list.set_locked(true);
    }
}
