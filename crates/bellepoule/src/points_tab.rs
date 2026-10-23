//! Onglet « Points » des préférences de la compétition : attribution de
//! points selon la place au classement général (barème, bonus par place et
//! « dans les X », poids, formule). Une modification s'applique tout de suite.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use bp_core::i18n::tr;
use bp_core::points::{self, FormulaStore, PlaceBonus, PointsSettings, TopBonus};
use bp_core::tournament::user_config::UserConfig;
use gtk::prelude::*;
use gtk4 as gtk;

use crate::contest_page::ContestPage;

/// Formules enregistrées : `formules.json` à côté des réglages.
fn library_path() -> std::path::PathBuf {
    UserConfig::default_path().with_file_name("formules.json")
}

/// Ajoute l'onglet aux préférences de la compétition.
pub fn install(page: &Rc<ContestPage>) {
    let Some(notebook) = page.glade.object::<gtk::Notebook>("properties_notebook") else { return };
    let editor = Editor::new(page);
    notebook.append_page(&editor.root, Some(&gtk::Label::new(Some(&tr("Points")))));
    let keep = editor.clone();
    editor.root.connect_map(move |_| keep.show());
}

/// Ligne de bonus : place (ou « dans les X ») et points.
struct BonusRow {
    root: gtk::Box,
    key: gtk::SpinButton,
    points: gtk::SpinButton,
}

struct Editor {
    me: Weak<Editor>,
    page: Weak<ContestPage>,
    root: gtk::ScrolledWindow,
    enabled: gtk::CheckButton,
    body: gtk::Box,
    weight: gtk::SpinButton,
    formula: gtk::Entry,
    library: gtk::DropDown,
    library_names: RefCell<Vec<String>>,
    save_name: gtk::Entry,
    places: gtk::Box,
    place_rows: RefCell<Vec<BonusRow>>,
    tops: gtk::Box,
    top_rows: RefCell<Vec<BonusRow>>,
    preview: gtk::Label,
    updating: Cell<bool>,
}

fn spin(min: f64, max: f64, step: f64, digits: u32) -> gtk::SpinButton {
    let s = gtk::SpinButton::with_range(min, max, step);
    s.set_digits(digits);
    bp_gtk::util::scroll_passes_through(&s);
    s
}

fn section(text: &str) -> gtk::Label {
    let l = gtk::Label::new(None);
    l.set_markup(&format!("<b>{}</b>", gtk::glib::markup_escape_text(text)));
    l.set_xalign(0.0);
    l.set_margin_top(8);
    l
}

impl Editor {
    fn new(page: &Rc<ContestPage>) -> Rc<Editor> {
        let content = gtk::Box::new(gtk::Orientation::Vertical, 8);
        content.set_margin_top(14);
        content.set_margin_bottom(14);
        content.set_margin_start(14);
        content.set_margin_end(14);
        let enabled = gtk::CheckButton::with_label(&tr("Award points for this competition (season ranking)"));
        content.append(&enabled);
        let body = gtk::Box::new(gtk::Orientation::Vertical, 8);
        content.append(&body);

        let intro = gtk::Label::new(Some(&tr("By default, out of N participants the 1st gets N points and the last 1 point.")));
        intro.set_wrap(true);
        intro.set_xalign(0.0);
        intro.add_css_class("bp-muted");
        body.append(&intro);
        let weight_line = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        weight_line.append(&gtk::Label::new(Some(&tr("Weight of the competition (C)"))));
        let weight = spin(0.0, 1000.0, 0.5, 1);
        weight_line.append(&weight);
        body.append(&weight_line);

        body.append(&section(&tr("Bonus by place")));
        let places = gtk::Box::new(gtk::Orientation::Vertical, 4);
        body.append(&places);
        let add_place = gtk::Button::with_label(&tr("Add a place bonus"));
        add_place.set_halign(gtk::Align::Start);
        body.append(&add_place);

        body.append(&section(&tr("Bonus for finishing in the top")));
        let tops = gtk::Box::new(gtk::Orientation::Vertical, 4);
        body.append(&tops);
        let add_top = gtk::Button::with_label(&tr("Add a top bonus"));
        add_top.set_halign(gtk::Align::Start);
        body.append(&add_top);

        body.append(&section(&tr("Formula")));
        let library_line = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        library_line.append(&gtk::Label::new(Some(&tr("Saved formulas"))));
        let library = gtk::DropDown::from_strings(&[]);
        bp_gtk::util::scroll_passes_through(&library);
        library.set_hexpand(true);
        library_line.append(&library);
        let delete = gtk::Button::with_label(&tr("Delete"));
        library_line.append(&delete);
        body.append(&library_line);
        let formula = gtk::Entry::new();
        formula.set_hexpand(true);
        body.append(&formula);
        let save_line = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        save_line.append(&gtk::Label::new(Some(&tr("Save this formula as"))));
        let save_name = gtk::Entry::new();
        save_name.set_hexpand(true);
        save_line.append(&save_name);
        let save = gtk::Button::with_label(&tr("Save"));
        save_line.append(&save);
        body.append(&save_line);
        let help = gtk::Expander::new(Some(&tr("How to write a formula?")));
        let help_text = gtk::Label::new(Some(points::HELP));
        help_text.set_wrap(true);
        help_text.set_xalign(0.0);
        help_text.set_selectable(true);
        help_text.set_margin_start(12);
        help.set_child(Some(&help_text));
        body.append(&help);
        let preview = gtk::Label::new(None);
        preview.set_xalign(0.0);
        preview.set_wrap(true);
        body.append(&preview);

        let root = gtk::ScrolledWindow::new();
        root.set_child(Some(&content));
        root.set_vexpand(true);

        let editor = Rc::new_cyclic(|me| Editor {
            me: me.clone(),
            page: Rc::downgrade(page),
            root,
            enabled,
            body,
            weight,
            formula,
            library,
            library_names: RefCell::new(Vec::new()),
            save_name,
            places,
            place_rows: RefCell::new(Vec::new()),
            tops,
            top_rows: RefCell::new(Vec::new()),
            preview,
            updating: Cell::new(false),
        });
        let w = Rc::downgrade(&editor);
        editor.enabled.connect_toggled(move |_| {
            if let Some(e) = w.upgrade() {
                e.on_changed();
            }
        });
        let w = Rc::downgrade(&editor);
        editor.weight.connect_value_changed(move |_| {
            if let Some(e) = w.upgrade() {
                e.on_changed();
            }
        });
        let w = Rc::downgrade(&editor);
        editor.formula.connect_changed(move |_| {
            if let Some(e) = w.upgrade() {
                e.on_changed();
            }
        });
        let w = Rc::downgrade(&editor);
        editor.library.connect_selected_notify(move |d| {
            let Some(e) = w.upgrade() else { return };
            if e.updating.get() {
                return;
            }
            let all = FormulaStore::load(&library_path()).all();
            if let Some(f) = all.get(d.selected() as usize) {
                e.formula.set_text(&f.formula);
                e.save_name.set_text(&f.name);
            }
        });
        let w = Rc::downgrade(&editor);
        save.connect_clicked(move |_| {
            let Some(e) = w.upgrade() else { return };
            let mut store = FormulaStore::load(&library_path());
            let parent = e.page.upgrade().and_then(|p| p.window());
            match store.put(&e.save_name.text(), &e.formula.text()) {
                Ok(()) => {
                    if let Err(err) = store.save(&library_path()) {
                        bp_gtk::dialogs::error(parent.as_ref(), &err.to_string());
                    }
                    e.fill_library();
                }
                Err(err) => bp_gtk::dialogs::error(parent.as_ref(), &err),
            }
        });
        let w = Rc::downgrade(&editor);
        delete.connect_clicked(move |_| {
            let Some(e) = w.upgrade() else { return };
            let mut store = FormulaStore::load(&library_path());
            let name = e.save_name.text().to_string();
            let parent = e.page.upgrade().and_then(|p| p.window());
            if store.remove(&name) {
                let _ = store.save(&library_path());
                e.fill_library();
            } else {
                bp_gtk::dialogs::error(parent.as_ref(), &tr("Only the formulas you saved can be deleted."));
            }
        });
        let w = Rc::downgrade(&editor);
        add_place.connect_clicked(move |_| {
            if let Some(e) = w.upgrade() {
                let next = e.place_rows.borrow().len() as f64 + 1.0;
                e.add_row(false, next, 1.0);
                e.on_changed();
            }
        });
        let w = Rc::downgrade(&editor);
        add_top.connect_clicked(move |_| {
            if let Some(e) = w.upgrade() {
                let next = 4.0 * 2f64.powi(e.top_rows.borrow().len() as i32);
                e.add_row(true, next, 1.0);
                e.on_changed();
            }
        });
        editor.show();
        editor
    }

    fn add_row(&self, top: bool, key: f64, value: f64) {
        let root = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        root.append(&gtk::Label::new(Some(&tr(if top { "In the top" } else { "Place" }))));
        let key_spin = spin(1.0, 9999.0, 1.0, 0);
        key_spin.set_value(key);
        root.append(&key_spin);
        root.append(&gtk::Label::new(Some(&tr("points:"))));
        let points = spin(-1000.0, 10000.0, 0.5, 1);
        points.set_value(value);
        root.append(&points);
        let remove = gtk::Button::from_icon_name("list-remove-symbolic");
        remove.add_css_class("flat");
        root.append(&remove);
        let changed = {
            let me = self.me.clone();
            move || {
                if let Some(e) = me.upgrade() {
                    e.on_changed();
                }
            }
        };
        let c = changed.clone();
        key_spin.connect_value_changed(move |_| c());
        let c = changed.clone();
        points.connect_value_changed(move |_| c());
        let me = self.me.clone();
        let line = root.clone();
        remove.connect_clicked(move |_| {
            let Some(e) = me.upgrade() else { return };
            let (rows, parent) = if top { (&e.top_rows, &e.tops) } else { (&e.place_rows, &e.places) };
            rows.borrow_mut().retain(|r| r.root != line);
            parent.remove(&line);
            e.on_changed();
        });
        let (rows, parent) = if top { (&self.top_rows, &self.tops) } else { (&self.place_rows, &self.places) };
        parent.append(&root);
        rows.borrow_mut().push(BonusRow { root, key: key_spin, points });
    }

    /// Liste des formules ; celle de la compétition est sélectionnée.
    fn fill_library(&self) {
        let all = FormulaStore::load(&library_path()).all();
        let names: Vec<String> = all.iter().map(|f| f.name.clone()).collect();
        self.updating.set(true);
        if *self.library_names.borrow() != names {
            let refs: Vec<&str> = names.iter().map(String::as_str).collect();
            self.library.set_model(Some(&gtk::StringList::new(&refs)));
            *self.library_names.borrow_mut() = names;
        }
        let current = self.formula.text().trim().to_string();
        match all.iter().position(|f| f.formula == current) {
            Some(i) => {
                self.library.set_selected(i as u32);
                if self.save_name.text().is_empty() {
                    self.save_name.set_text(&all[i].name);
                }
            }
            None => self.library.set_selected(gtk::INVALID_LIST_POSITION),
        }
        self.updating.set(false);
    }

    fn read(&self) -> PointsSettings {
        PointsSettings {
            enabled: self.enabled.is_active(),
            weight: self.weight.value(),
            place_bonus: self.place_rows.borrow().iter().map(|r| PlaceBonus { place: r.key.value_as_int().max(1) as u32, points: r.points.value() }).collect(),
            top_bonus: self.top_rows.borrow().iter().map(|r| TopBonus { top: r.key.value_as_int().max(1) as u32, points: r.points.value() }).collect(),
            formula: self.formula.text().trim().to_string(),
        }
    }

    /// Relit les réglages de la compétition.
    fn show(&self) {
        let Some(page) = self.page.upgrade() else { return };
        let Some(settings) = page.with_contest(|c| c.points.clone()) else { return };
        self.updating.set(true);
        self.enabled.set_active(settings.enabled);
        self.weight.set_value(settings.weight);
        self.formula.set_text(&settings.formula);
        for (rows, parent) in [(&self.place_rows, &self.places), (&self.top_rows, &self.tops)] {
            for r in rows.borrow_mut().drain(..) {
                parent.remove(&r.root);
            }
        }
        for b in &settings.place_bonus {
            self.add_row(false, b.place as f64, b.points);
        }
        for b in &settings.top_bonus {
            self.add_row(true, b.top as f64, b.points);
        }
        self.updating.set(false);
        self.fill_library();
        self.update_preview(&settings);
    }

    fn update_preview(&self, s: &PointsSettings) {
        self.body.set_visible(s.enabled);
        let text = match s.formula_error() {
            Some(e) => format!("⚠ {e}"),
            None => format!(
                "{} 1er = {}, 2e = {}, 3e = {}, 5e = {}, dernier = {}",
                tr("Example with 10 participants:"),
                points::image(s.points(10, 1).unwrap_or(0.0)),
                points::image(s.points(10, 2).unwrap_or(0.0)),
                points::image(s.points(10, 3).unwrap_or(0.0)),
                points::image(s.points(10, 5).unwrap_or(0.0)),
                points::image(s.points(10, 10).unwrap_or(0.0)),
            ),
        };
        self.preview.set_text(&text);
    }

    fn on_changed(&self) {
        if self.updating.get() {
            return;
        }
        let settings = self.read();
        self.update_preview(&settings);
        let Some(page) = self.page.upgrade() else { return };
        let changed = page.with_contest(|c| {
            let changed = c.points != settings;
            c.points = settings;
            changed
        });
        if changed == Some(true) {
            page.mark_dirty();
            if let Some(schedule) = page.schedule.borrow().as_ref() {
                schedule.refresh_screens();
            }
        }
    }
}
