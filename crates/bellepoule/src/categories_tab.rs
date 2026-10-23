//! Onglet « Catégories » des préférences : modèle de catégories générales
//! (noms, années de naissance, bascule de la saison), enregistré dans
//! `categories.json` à côté des réglages ; le modèle FFE est fourni.
//!
//! Les catégories sont toujours calculées d'après la date de naissance
//! ([`bp_core::categories`]) ; une modification est appliquée tout de suite
//! (colonne « Catégorie générale » de l'appel, critères des divisions).

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::{Rc, Weak};

use bp_core::categories::{self, AgeCategory, CategoryModel, CategoryStore};
use bp_core::i18n::tr;
use bp_core::tournament::user_config::UserConfig;
use bp_gtk::dialogs;
use gtk::prelude::*;
use gtk4 as gtk;

use crate::contest_page::ContestPage;

thread_local! {
    static STORE: RefCell<CategoryStore> = RefCell::new(CategoryStore::default());
}

fn path() -> PathBuf {
    UserConfig::default_path().with_file_name("categories.json")
}

/// Au démarrage : modèles enregistrés et modèle actif.
pub fn init() {
    let store = CategoryStore::load(&path());
    categories::set_active(store.current());
    STORE.with(|s| *s.borrow_mut() = store);
}

fn save_store() {
    STORE.with(|s| {
        if let Err(e) = s.borrow().save(&path()) {
            log::warn!("categories.json : {e}");
        }
    });
}

/// Ajoute l'onglet aux préférences de la compétition.
pub fn install(page: &Rc<ContestPage>) {
    let Some(notebook) = page.glade.object::<gtk::Notebook>("properties_notebook") else { return };
    let editor = Editor::new(page);
    notebook.append_page(&editor.root, Some(&gtk::Label::new(Some(&tr("Categories")))));
    // Relu à chaque affichage : un autre onglet a pu changer le modèle.
    let keep = editor.clone();
    editor.root.connect_map(move |_| keep.show(&categories::active()));
}

struct Row {
    root: gtk::Box,
    name: gtk::Entry,
    oldest: gtk::SpinButton,
    open: gtk::CheckButton,
    newest: gtk::SpinButton,
}

struct Editor {
    me: Weak<Editor>,
    page: Weak<ContestPage>,
    root: gtk::ScrolledWindow,
    models: gtk::DropDown,
    model_names: RefCell<Vec<String>>,
    model_name: gtk::Entry,
    day: gtk::SpinButton,
    month: gtk::DropDown,
    season: gtk::Label,
    rows_box: gtk::Box,
    rows: RefCell<Vec<Row>>,
    status: gtk::Label,
    updating: Cell<bool>,
}

fn year_spin() -> gtk::SpinButton {
    let s = gtk::SpinButton::with_range(1900.0, 2200.0, 1.0);
    s.set_digits(0);
    bp_gtk::util::scroll_passes_through(&s);
    s
}

const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];

impl Editor {
    fn new(page: &Rc<ContestPage>) -> Rc<Editor> {
        let content = gtk::Box::new(gtk::Orientation::Vertical, 10);
        for m in ["top", "bottom", "start", "end"] {
            match m {
                "top" => content.set_margin_top(14),
                "bottom" => content.set_margin_bottom(14),
                "start" => content.set_margin_start(14),
                _ => content.set_margin_end(14),
            }
        }
        let intro = gtk::Label::new(Some(&tr("The general category of each fencer is computed from the birth date. Settings common to every competition.")));
        intro.set_wrap(true);
        intro.set_xalign(0.0);
        intro.add_css_class("bp-muted");
        content.append(&intro);

        let model_line = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        model_line.append(&gtk::Label::new(Some(&tr("Model"))));
        let models = gtk::DropDown::from_strings(&[]);
        bp_gtk::util::scroll_passes_through(&models);
        model_line.append(&models);
        let delete = gtk::Button::with_label(&tr("Delete this model"));
        model_line.append(&delete);
        content.append(&model_line);

        let switch_line = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        switch_line.append(&gtk::Label::new(Some(&tr("Season changes on"))));
        let day = gtk::SpinButton::with_range(1.0, 31.0, 1.0);
        bp_gtk::util::scroll_passes_through(&day);
        switch_line.append(&day);
        let month_names: Vec<String> = MONTHS.iter().map(|m| tr(m)).collect();
        let month_refs: Vec<&str> = month_names.iter().map(String::as_str).collect();
        let month = gtk::DropDown::from_strings(&month_refs);
        bp_gtk::util::scroll_passes_through(&month);
        switch_line.append(&month);
        let season = gtk::Label::new(None);
        season.add_css_class("bp-muted");
        season.set_margin_start(12);
        switch_line.append(&season);
        content.append(&switch_line);

        let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        for (text, width) in [(tr("Category"), 140), (tr("Born from"), 110), (String::new(), 90), (tr("to"), 110)] {
            let l = gtk::Label::new(Some(&text));
            l.set_xalign(0.0);
            l.set_size_request(width, -1);
            l.add_css_class("bp-muted");
            header.append(&l);
        }
        content.append(&header);
        let rows_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
        content.append(&rows_box);
        let add = gtk::Button::with_label(&tr("Add a category"));
        add.set_halign(gtk::Align::Start);
        content.append(&add);

        let save_line = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        save_line.set_margin_top(8);
        save_line.append(&gtk::Label::new(Some(&tr("Model name"))));
        let model_name = gtk::Entry::new();
        model_name.set_width_chars(20);
        save_line.append(&model_name);
        let save = gtk::Button::with_label(&tr("Save the model"));
        save.add_css_class("suggested-action");
        save_line.append(&save);
        let status = gtk::Label::new(None);
        status.add_css_class("bp-muted");
        save_line.append(&status);
        content.append(&save_line);

        let root = gtk::ScrolledWindow::new();
        root.set_child(Some(&content));
        root.set_vexpand(true);

        let editor = Rc::new_cyclic(|me| Editor {
            me: me.clone(),
            page: Rc::downgrade(page),
            root,
            models,
            model_names: RefCell::new(Vec::new()),
            model_name,
            day,
            month,
            season,
            rows_box,
            rows: RefCell::new(Vec::new()),
            status,
            updating: Cell::new(false),
        });

        let weak = Rc::downgrade(&editor);
        editor.models.connect_selected_notify(move |d| {
            let Some(e) = weak.upgrade() else { return };
            if e.updating.get() {
                return;
            }
            let Some(name) = e.model_names.borrow().get(d.selected() as usize).cloned() else { return };
            let model = STORE.with(|s| {
                let mut s = s.borrow_mut();
                s.active = name;
                s.current()
            });
            save_store();
            categories::set_active(model.clone());
            e.show(&model);
            e.refresh_lists();
        });
        let weak = Rc::downgrade(&editor);
        editor.day.connect_value_changed(move |_| {
            if let Some(e) = weak.upgrade() {
                e.on_changed();
            }
        });
        let weak = Rc::downgrade(&editor);
        editor.month.connect_selected_notify(move |_| {
            if let Some(e) = weak.upgrade() {
                e.on_changed();
            }
        });
        let weak = Rc::downgrade(&editor);
        add.connect_clicked(move |_| {
            let Some(e) = weak.upgrade() else { return };
            let season = e.season_year();
            e.add_row(&AgeCategory::from_birth_years(&tr("New category"), season, Some(season - 10), season - 9));
            e.on_changed();
        });
        let weak = Rc::downgrade(&editor);
        save.connect_clicked(move |_| {
            if let Some(e) = weak.upgrade() {
                e.on_save();
            }
        });
        let weak = Rc::downgrade(&editor);
        delete.connect_clicked(move |_| {
            if let Some(e) = weak.upgrade() {
                e.on_delete();
            }
        });
        editor.show(&categories::active());
        editor
    }

    /// Saison en cours d'après la bascule affichée.
    fn season_year(&self) -> i32 {
        let model = CategoryModel { switch_day: self.day.value_as_int() as u32, switch_month: self.month.selected() + 1, ..CategoryModel::ffe() };
        model.season(categories::today())
    }

    fn add_row(&self, c: &AgeCategory) {
        let season = self.season_year();
        let root = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let name = gtk::Entry::new();
        name.set_text(&c.name);
        name.set_size_request(140, -1);
        root.append(&name);
        let (oldest, newest_year) = c.birth_years(season);
        let oldest_spin = year_spin();
        oldest_spin.set_value(oldest.unwrap_or(newest_year - 1) as f64);
        oldest_spin.set_size_request(110, -1);
        root.append(&oldest_spin);
        let open = gtk::CheckButton::with_label(&tr("and before"));
        open.set_active(oldest.is_none());
        open.set_size_request(90, -1);
        oldest_spin.set_sensitive(oldest.is_some());
        root.append(&open);
        let newest = year_spin();
        newest.set_value(newest_year as f64);
        newest.set_size_request(110, -1);
        root.append(&newest);
        let remove = gtk::Button::from_icon_name("list-remove-symbolic");
        remove.add_css_class("flat");
        remove.set_tooltip_text(Some(&tr("Remove this category")));
        root.append(&remove);
        self.rows_box.append(&root);

        let changed = {
            let me = self.me.clone();
            move || {
                if let Some(e) = me.upgrade() {
                    e.on_changed();
                }
            }
        };
        let c1 = changed.clone();
        name.connect_changed(move |_| c1());
        let c2 = changed.clone();
        oldest_spin.connect_value_changed(move |_| c2());
        let c3 = changed.clone();
        newest.connect_value_changed(move |_| c3());
        let spin = oldest_spin.clone();
        open.connect_toggled(move |b| {
            spin.set_sensitive(!b.is_active());
            changed();
        });
        let me = self.me.clone();
        let line = root.clone();
        remove.connect_clicked(move |_| {
            let Some(e) = me.upgrade() else { return };
            e.rows.borrow_mut().retain(|r| r.root != line);
            e.rows_box.remove(&line);
            e.on_changed();
        });
        self.rows.borrow_mut().push(Row { root, name, oldest: oldest_spin, open, newest });
    }

    /// Modèle lu dans les champs.
    fn read(&self) -> CategoryModel {
        let season = self.season_year();
        let categories = self
            .rows
            .borrow()
            .iter()
            .map(|r| {
                let oldest = (!r.open.is_active()).then(|| r.oldest.value_as_int());
                AgeCategory::from_birth_years(r.name.text().trim(), season, oldest, r.newest.value_as_int())
            })
            .collect();
        CategoryModel {
            name: self.model_name.text().trim().to_string(),
            switch_day: self.day.value_as_int() as u32,
            switch_month: self.month.selected() + 1,
            categories,
        }
    }

    /// Montre un modèle (liste des modèles, bascule, catégories).
    fn show(&self, model: &CategoryModel) {
        self.updating.set(true);
        let (names, active) = STORE.with(|s| {
            let s = s.borrow();
            (s.models.iter().map(|m| m.name.clone()).collect::<Vec<_>>(), s.active.clone())
        });
        if *self.model_names.borrow() != names {
            let refs: Vec<&str> = names.iter().map(String::as_str).collect();
            self.models.set_model(Some(&gtk::StringList::new(&refs)));
            *self.model_names.borrow_mut() = names.clone();
        }
        if let Some(i) = names.iter().position(|n| *n == active) {
            self.models.set_selected(i as u32);
        }
        self.model_name.set_text(&model.name);
        self.day.set_value(model.switch_day as f64);
        self.month.set_selected(model.switch_month.saturating_sub(1));
        for r in self.rows.borrow_mut().drain(..) {
            self.rows_box.remove(&r.root);
        }
        for c in &model.categories {
            self.add_row(c);
        }
        self.updating.set(false);
        self.update_status();
    }

    fn update_status(&self) {
        let season = self.season_year();
        self.season.set_text(&format!("{} {}-{}", tr("Current season:"), season, season + 1));
        let model = self.read();
        let saved = STORE.with(|s| s.borrow().models.iter().any(|m| *m == model));
        self.status.set_text(&if saved { String::new() } else { tr("(not saved)") });
    }

    /// Une modification s'applique tout de suite (sans être enregistrée).
    fn on_changed(&self) {
        if self.updating.get() {
            return;
        }
        let model = self.read();
        categories::set_active(model);
        self.update_status();
        self.refresh_lists();
    }

    fn on_save(&self) {
        let model = self.read();
        let parent = self.page.upgrade().and_then(|p| p.window());
        if model.name.is_empty() {
            dialogs::error(parent.as_ref(), &tr("Give the model a name."));
            return;
        }
        if model.name == "FFE" && model != CategoryModel::ffe() {
            dialogs::error(parent.as_ref(), &tr("The FFE model is provided: save your changes under another name."));
            return;
        }
        STORE.with(|s| s.borrow_mut().put(model.clone()));
        save_store();
        categories::set_active(model.clone());
        self.show(&model);
    }

    fn on_delete(&self) {
        let name = self.model_name.text().trim().to_string();
        let parent = self.page.upgrade().and_then(|p| p.window());
        if name == "FFE" {
            dialogs::error(parent.as_ref(), &tr("The FFE model is provided and cannot be deleted."));
            return;
        }
        if !dialogs::confirm(parent.as_ref(), &format!("{} « {name} » ?", tr("Delete the model")), None, None) {
            return;
        }
        let model = STORE.with(|s| {
            let mut s = s.borrow_mut();
            s.models.retain(|m| m.name != name);
            s.active = "FFE".into();
            s.current()
        });
        save_store();
        categories::set_active(model.clone());
        self.show(&model);
        self.refresh_lists();
    }

    /// Les listes affichées (appel…) suivent le nouveau calcul.
    fn refresh_lists(&self) {
        let Some(page) = self.page.upgrade() else { return };
        let Some(app) = page.app.upgrade() else { return };
        for p in app.pages.borrow().iter() {
            if let Some(schedule) = p.schedule.borrow().as_ref() {
                schedule.refresh_screens();
            }
        }
    }
}
