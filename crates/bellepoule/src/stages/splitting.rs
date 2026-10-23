//! Point de scission : divisions (réglages de la formule) et répartition des
//! tireurs, préremplie par les règles et modifiable à la main.

use bp_core::{
    filter::Filter,
    i18n::tr,
    rounds::splitting::{Criterion, Division, Size, SplitConfig, SplitMode, Splitting},
    stage::Stage,
};
use bp_gtk::{dialogs, glade::Glade};
use gtk::prelude::*;
use gtk4 as gtk;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use super::StageScreen;
use crate::contest_page::ContestPage;
use crate::widgets::{
    filter_dialog,
    list_document::{self, ListDocument},
    players_list::{Cell as ListCell, CellKind, ColumnSpec, PlayersListView},
    render,
};

/// Une ligne de l'éditeur des divisions.
struct DivisionRow {
    root: gtk::Box,
    name: gtk::Entry,
    size: gtk::SpinButton,
    unit: gtk::DropDown,
    rest: gtk::Label,
    kind: gtk::DropDown,
    value: gtk::Entry,
}

pub struct SplittingScreen {
    me: Weak<Self>,
    page: Weak<ContestPage>,
    index: usize,
    glade: Glade,
    root: gtk::Widget,
    configuration: gtk::Widget,
    name: gtk::Entry,
    list: PlayersListView,
    updating: Cell<bool>,
    // Réglages des divisions (formule)
    mode: gtk::DropDown,
    keep_box: gtk::Box,
    keep: gtk::SpinButton,
    keep_unit: gtk::DropDown,
    rows_box: gtk::Box,
    rows: RefCell<Vec<DivisionRow>>,
    // Placement de la sélection dans une division
    assign: gtk::DropDown,
    assign_labels: RefCell<Vec<String>>,
}

fn dropdown(items: &[String]) -> gtk::DropDown {
    let items: Vec<&str> = items.iter().map(String::as_str).collect();
    let d = gtk::DropDown::from_strings(&items);
    bp_gtk::util::scroll_passes_through(&d);
    d
}

fn spin(max: f64) -> gtk::SpinButton {
    let s = gtk::SpinButton::with_range(0.0, max, 1.0);
    s.set_digits(0);
    bp_gtk::util::scroll_passes_through(&s);
    s
}

/// Critère lu dans les champs : genre (liste) et valeurs saisies.
fn criterion_from(kind: u32, text: &str) -> Option<Criterion> {
    let years: Vec<i32> = text
        .split(|c: char| !c.is_ascii_digit())
        .filter_map(|s| s.parse().ok())
        .filter(|y| (1900..2200).contains(y))
        .collect();
    match kind {
        0 => {
            let values: Vec<String> = text.split([',', ';', ' ']).map(str::trim).filter(|s| !s.is_empty()).map(String::from).collect();
            (!values.is_empty()).then_some(Criterion::Category { values })
        }
        1 => (!years.is_empty()).then_some(Criterion::BirthYears { years }),
        2 => years.first().map(|y| Criterion::BornBefore { year: *y }),
        _ => years.first().map(|y| Criterion::BornFrom { year: *y }),
    }
}

fn criterion_text(c: Option<&Criterion>) -> (u32, String) {
    let join = |v: &[i32]| v.iter().map(|y| y.to_string()).collect::<Vec<_>>().join(", ");
    match c {
        None => (0, String::new()),
        Some(Criterion::Category { values }) => (0, values.join(", ")),
        Some(Criterion::BirthYears { years }) => (1, join(years)),
        Some(Criterion::BornBefore { year }) => (2, year.to_string()),
        Some(Criterion::BornFrom { year }) => (3, year.to_string()),
    }
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

        // Divisions, sous le libellé, dans les paramètres de la formule.
        let editor = gtk::Box::new(gtk::Orientation::Vertical, 6);
        editor.set_margin_top(10);
        let title = gtk::Label::new(None);
        title.set_markup(&format!("<b>{}</b>", tr("Divisions")));
        title.set_xalign(0.0);
        editor.append(&title);
        let mode_line = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        mode_line.append(&gtk::Label::new(Some(&tr("Fill in"))));
        let mode = dropdown(&[tr("by hand"), tr("by ranking"), tr("by criteria")]);
        mode_line.append(&mode);
        editor.append(&mode_line);
        let keep_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        keep_box.append(&gtk::Label::new(Some(&tr("Kept in this competition: the first"))));
        let keep = spin(9999.0);
        keep_box.append(&keep);
        let keep_unit = dropdown(&[tr("places"), "%".into()]);
        keep_box.append(&keep_unit);
        editor.append(&keep_box);
        let rows_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
        editor.append(&rows_box);
        let add = gtk::Button::with_label(&tr("Add a division"));
        add.set_halign(gtk::Align::Start);
        editor.append(&add);
        if let Some(vbox) = glade.widget("vbox1").and_then(|w| w.downcast::<gtk::Box>().ok()) {
            vbox.append(&editor);
        }

        // Placement de la sélection, dans la barre d'outils de la liste.
        let toolbar: gtk::Box = glade.get("toolbar3");
        let assign_line = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        assign_line.set_margin_start(12);
        assign_line.append(&gtk::Label::new(Some(&tr("Move the selection to"))));
        let assign = dropdown(&[tr("Stays here")]);
        assign_line.append(&assign);
        let place = gtk::Button::with_label(&tr("Move"));
        assign_line.append(&place);
        toolbar.append(&assign_line);

        let screen = Rc::new_cyclic(|me| Self {
            me: me.clone(),
            page: Rc::downgrade(page),
            index,
            glade,
            root,
            configuration,
            name,
            list,
            updating: Cell::new(false),
            mode,
            keep_box,
            keep,
            keep_unit,
            rows_box,
            rows: RefCell::new(Vec::new()),
            assign,
            assign_labels: RefCell::new(vec![tr("Stays here")]),
        });
        let weak = Rc::downgrade(&screen);
        screen.name.connect_changed(move |_| {
            if let Some(s) = weak.upgrade() {
                s.on_name_changed();
            }
        });
        for signal in ["mode", "keep_unit"] {
            let weak = Rc::downgrade(&screen);
            let d = if signal == "mode" { &screen.mode } else { &screen.keep_unit };
            d.connect_selected_notify(move |_| {
                if let Some(s) = weak.upgrade() {
                    s.on_config_changed();
                }
            });
        }
        let weak = Rc::downgrade(&screen);
        screen.keep.connect_value_changed(move |_| {
            if let Some(s) = weak.upgrade() {
                s.on_config_changed();
            }
        });
        let weak = Rc::downgrade(&screen);
        add.connect_clicked(move |_| {
            let Some(s) = weak.upgrade() else { return };
            let n = s.rows.borrow().len() + 2;
            s.add_row(&Division { name: format!("Division {n}"), ..Default::default() });
            s.on_config_changed();
        });
        let weak = Rc::downgrade(&screen);
        place.connect_clicked(move |_| {
            if let Some(s) = weak.upgrade() {
                let targets = s.list.selected();
                s.assign_to(&targets, s.assign.selected());
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
            // Cochée : la division choisie dans la barre (la première par défaut).
            let division = if value { s.assign.selected().max(1) } else { 0 };
            s.assign_to(targets, division);
        });
        screen.refresh();
        screen
    }

    /// Place des tireurs dans une division (0 = ils restent).
    fn assign_to(&self, targets: &[bp_core::player::PlayerRef], division: u32) {
        let Some(page) = self.page.upgrade() else { return };
        let changed = page
            .with_contest(|c| {
                c.with_schedule(|schedule, ctx| {
                    let stage = schedule.stage_mut(self.index)?.as_any_mut().downcast_mut::<Splitting>()?;
                    if stage.base().locked {
                        return None;
                    }
                    let refs: Vec<_> = targets.iter().copied().filter(|r| stage.base().attendees.contains(r)).collect();
                    for r in refs {
                        stage.set_division(ctx, r, division);
                    }
                    Some(())
                })
            })
            .flatten()
            .is_some();
        if changed {
            page.mark_dirty();
            self.refresh();
        }
    }

    /// Ajoute une ligne à l'éditeur des divisions.
    fn add_row(&self, d: &Division) {
        let root = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let name = gtk::Entry::new();
        name.set_placeholder_text(Some(&tr("Division name")));
        name.set_text(&d.name);
        name.set_width_chars(16);
        root.append(&name);
        let size = spin(9999.0);
        let unit = dropdown(&[tr("places"), "%".into()]);
        match d.size {
            Some(Size::Count(n)) => size.set_value(n as f64),
            Some(Size::Percent(p)) => {
                size.set_value(p as f64);
                unit.set_selected(1);
            }
            None => {}
        }
        root.append(&size);
        root.append(&unit);
        let rest = gtk::Label::new(Some(&tr("the rest")));
        rest.add_css_class("bp-muted");
        root.append(&rest);
        let kind = dropdown(&[tr("Categories"), tr("Birth years"), tr("Born in or before"), tr("Born in or after")]);
        let value = gtk::Entry::new();
        value.set_hexpand(true);
        let (k, text) = criterion_text(d.criterion.as_ref());
        kind.set_selected(k);
        value.set_text(&text);
        root.append(&kind);
        root.append(&value);
        let remove = gtk::Button::from_icon_name("list-remove-symbolic");
        remove.set_tooltip_text(Some(&tr("Remove this division")));
        remove.add_css_class("flat");
        root.append(&remove);
        self.rows_box.append(&root);

        let changed = {
            let me = self.me.clone();
            move || {
                if let Some(s) = me.upgrade() {
                    s.on_config_changed();
                }
            }
        };
        let c = changed.clone();
        name.connect_changed(move |_| c());
        let c = changed.clone();
        size.connect_value_changed(move |_| c());
        let c = changed.clone();
        unit.connect_selected_notify(move |_| c());
        let c = changed.clone();
        kind.connect_selected_notify(move |_| c());
        let c = changed.clone();
        value.connect_changed(move |_| c());
        let me = self.me.clone();
        let line = root.clone();
        remove.connect_clicked(move |_| {
            let Some(s) = me.upgrade() else { return };
            if s.rows.borrow().len() <= 1 {
                return;
            }
            s.rows.borrow_mut().retain(|r| r.root != line);
            s.rows_box.remove(&line);
            s.on_config_changed();
        });
        self.rows.borrow_mut().push(DivisionRow { root, name, size, unit, rest, kind, value });
        self.update_editor();
    }

    /// Réglages lus dans l'éditeur.
    fn read_config(&self) -> SplitConfig {
        let mode = match self.mode.selected() {
            1 => SplitMode::Ranking,
            2 => SplitMode::Criteria,
            _ => SplitMode::Manual,
        };
        let size = |spin: &gtk::SpinButton, unit: &gtk::DropDown| {
            let n = spin.value_as_int().max(0) as u32;
            if unit.selected() == 1 { Size::Percent(n) } else { Size::Count(n) }
        };
        let keep = Some(size(&self.keep, &self.keep_unit)).filter(|s| *s != Size::Count(0));
        let rows = self.rows.borrow();
        let last = rows.len().saturating_sub(1);
        let divisions = rows
            .iter()
            .enumerate()
            .map(|(i, r)| Division {
                name: r.name.text().to_string(),
                size: (i < last).then(|| size(&r.size, &r.unit)),
                criterion: criterion_from(r.kind.selected(), &r.value.text()),
            })
            .collect();
        SplitConfig { mode, keep, divisions }
    }

    /// Montre les réglages dans l'éditeur (sans toucher à un champ inchangé).
    fn show_config(&self, c: &SplitConfig) {
        let mode = match c.mode {
            SplitMode::Manual => 0,
            SplitMode::Ranking => 1,
            SplitMode::Criteria => 2,
        };
        if self.mode.selected() != mode {
            self.mode.set_selected(mode);
        }
        if self.read_config() == *c {
            self.update_editor();
            return;
        }
        let (keep, unit) = match c.keep {
            Some(Size::Count(n)) => (n, 0),
            Some(Size::Percent(p)) => (p, 1),
            None => (0, 0),
        };
        self.keep.set_value(keep as f64);
        self.keep_unit.set_selected(unit);
        for r in self.rows.borrow_mut().drain(..) {
            self.rows_box.remove(&r.root);
        }
        for d in &c.divisions {
            self.add_row(d);
        }
        self.update_editor();
    }

    /// Champs utiles selon le mode.
    fn update_editor(&self) {
        let mode = self.mode.selected();
        self.keep_box.set_visible(mode == 1);
        let rows = self.rows.borrow();
        let last = rows.len().saturating_sub(1);
        for (i, r) in rows.iter().enumerate() {
            let sized = mode == 1 && i < last;
            r.size.set_visible(sized);
            r.unit.set_visible(sized);
            r.rest.set_visible(mode == 1 && i == last);
            r.kind.set_visible(mode == 2);
            r.value.set_visible(mode == 2);
            r.value.set_placeholder_text(Some(&match r.kind.selected() {
                0 => tr("e.g. M13, M15"),
                1 => tr("e.g. 2014, 2015"),
                _ => tr("year"),
            }));
        }
    }

    fn on_config_changed(&self) {
        if self.updating.get() {
            return;
        }
        self.update_editor();
        let config = self.read_config();
        let Some(page) = self.page.upgrade() else { return };
        let changed = page
            .with_contest(|c| {
                c.with_schedule(|schedule, ctx| {
                    let stage = schedule.stage_mut(self.index)?.as_any_mut().downcast_mut::<Splitting>()?;
                    if stage.base().locked || stage.config == config {
                        return None;
                    }
                    stage.set_config(ctx, config);
                    Some(())
                })
            })
            .flatten()
            .is_some();
        if changed {
            page.mark_dirty();
            self.refresh();
        }
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

    /// Liste, verrou, libellé, réglages et noms des divisions.
    fn document(&self) -> Option<(ListDocument, bool, String, SplitConfig, Vec<String>)> {
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
                let mut rows =
                    list_document::rows_for(&c.players, &base.attendees, &specs, base.owner());
                // Colonne « Division » : où va chaque tireur.
                let names: Vec<String> = (1..=stage.config.divisions.len() as u32).map(|d| stage.division_name(d)).collect();
                specs.push(ColumnSpec {
                    code: "division".into(),
                    title: tr("Division"),
                    look: bp_core::attribute::Look::LongText,
                    kind: CellKind::Text,
                    editable: false,
                });
                for row in rows.iter_mut() {
                    let d = stage.division_of(&c.players, row.player);
                    let text = if d == 0 { String::new() } else { stage.division_name(d) };
                    row.cells.push(ListCell::Text(text));
                    row.sort.push(Some(bp_core::attribute::Value::UInt(d)));
                    for child in row.children.iter_mut() {
                        child.cells.push(ListCell::Text(String::new()));
                        child.sort.push(None);
                    }
                }
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
                    stage.config.clone(),
                    names,
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
        let label = self.name.text().to_string();
        let changed = page
            .with_contest(|c| {
                c.with_schedule(|schedule, ctx| {
                    let stage = schedule.stage_mut(self.index)?.as_any_mut().downcast_mut::<Splitting>()?;
                    if stage.base().locked {
                        return None;
                    }
                    let before = stage.config.clone();
                    stage.set_label(ctx, &label);
                    Some(stage.config != before)
                })
            })
            .flatten();
        if let Some(divisions_changed) = changed {
            page.mark_dirty();
            // N1/N2, Tableau A/B : les divisions viennent d'être préparées.
            if divisions_changed {
                self.refresh();
            }
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
        let Some((document, ..)) = self.document() else {
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
        let Some((document, locked, name, config, names)) = self.document() else {
            return;
        };
        self.updating.set(true);
        self.list.set_columns(document.specs);
        self.list.set_rows(document.rows);
        self.list.set_locked(locked);
        if self.name.text() != name {
            self.name.set_text(&name);
        }
        self.show_config(&config);
        // Divisions proposées pour placer la sélection.
        let mut labels = vec![tr("Stays here")];
        labels.extend(names);
        if *self.assign_labels.borrow() != labels {
            let selected = self.assign.selected();
            let items: Vec<&str> = labels.iter().map(String::as_str).collect();
            self.assign.set_model(Some(&gtk::StringList::new(&items)));
            self.assign.set_selected(if (selected as usize) < labels.len() { selected } else { 0 });
            *self.assign_labels.borrow_mut() = labels;
        }
        self.assign.set_sensitive(!locked);
        self.configuration.set_sensitive(!locked);
        self.updating.set(false);
    }
}
