//! Onglet d'une compétition (port des parties interface de
//! `supervisor/contest.cpp`) : barre d'outils, propriétés, enregistrement.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::{Rc, Weak};

use bp_core::category::{Category, DISPLAY_IMAGES};
use bp_core::contest::{Contest, GENDER_IMAGES, LEVEL_IMAGES};
use bp_core::i18n::tr;
use bp_core::tournament::ContestId;
use bp_core::weapon::Weapon;
use bp_gtk::dialogs::{self, FileChooser};
use bp_gtk::glade::Glade;
use gtk4 as gtk;
use gtk::glib;
use gtk::prelude::*;

use crate::app::App;

/// Délai d'enregistrement automatique après une modification.
const SAVE_DELAY: u32 = bp_core::contest::SAVE_DELAY_SECONDS as u32;

pub struct ContestPage {
    pub id: ContestId,
    pub app: Weak<App>,
    pub glade: Glade,
    root: gtk::Widget,
    title: gtk::Widget,
    /// Date choisie dans le calendrier (jour, mois, année).
    date: Cell<(u32, u32, u32)>,
    css: gtk::CssProvider,
    save_timer: RefCell<Option<glib::SourceId>>,
    pub schedule: RefCell<Option<Rc<crate::schedule_view::ScheduleView>>>,
    hero: crate::visuals::CompetitionHeader,
}

impl ContestPage {
    pub fn new(app: &Rc<App>, id: ContestId) -> Option<Rc<ContestPage>> {
        let glade = match Glade::new("contest.ui") {
            Ok(g) => g,
            Err(e) => {
                dialogs::error(Some(&app.window), &e.0);
                return None;
            }
        };
        let root: gtk::Widget = glade.get("root");
        let title: gtk::Widget = glade.get("notebook_title");
        // Le C++ place la page dans le carnet du tournoi (`AttachTo`).
        Glade::detach_from_parent(&root);
        Glade::detach_from_parent(&title);
        app.notebook.append_page(&root, Some(&title));
        app.notebook.set_tab_reorderable(&root, true);
        let last = app.notebook.n_pages() - 1;
        app.notebook.set_current_page(Some(last));

        let css = gtk::CssProvider::new();
        title.add_css_class("bp-competition-summary");
        glade.get::<gtk::Widget>("colorbox").add_css_class("bp-event-color");
        for w in [&title, &glade.get::<gtk::Widget>("colorbox")] {
            w.style_context().add_provider(&css, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
        }
        glade.attach_actions(&root);
        root.add_css_class("bp-competition-page");
        let hero = crate::visuals::CompetitionHeader::new();
        if let Some(container) = root.downcast_ref::<gtk::Box>() { container.prepend(&hero.root); }

        let page = Rc::new(ContestPage {
            id,
            app: Rc::downgrade(app),
            glade,
            root,
            title,
            date: Cell::new((1, 1, 2000)),
            css,
            save_timer: RefCell::new(None),
            schedule: RefCell::new(None),
            hero,
        });
        page.populate_combos();
        page.connect_handlers();
        page.fill_in_properties();
        page.display_properties();
        page.paint_color();

        let schedule = crate::schedule_view::ScheduleView::new(&page);
        page.glade.get::<gtk::Viewport>("schedule_viewport").set_child(Some(&schedule.root()));
        if let Some(box_) = page.glade.widget("formula_alignment") {
            bp_gtk::glade::container_add(&box_, &schedule.config_widget());
        }
        schedule.refresh();
        *page.schedule.borrow_mut() = Some(schedule);
        Some(page)
    }

    // ----------------------------------------------------------------------
    // Accès à la compétition

    fn app(&self) -> Option<Rc<App>> {
        self.app.upgrade()
    }

    pub fn with_contest<R>(&self, f: impl FnOnce(&mut Contest) -> R) -> Option<R> {
        let app = self.app()?;
        let mut t = app.tournament.borrow_mut();
        t.contest_mut(self.id).map(f)
    }

    /// Comme `with_contest`, sans attendre : `None` si le tournoi est déjà
    /// en cours d'utilisation (boîte de dialogue ouverte pendant une action).
    pub fn try_with_contest<R>(&self, f: impl FnOnce(&mut Contest) -> R) -> Option<R> {
        let app = self.app()?;
        let mut t = app.tournament.try_borrow_mut().ok()?;
        t.contest_mut(self.id).map(f)
    }

    pub fn window(&self) -> Option<gtk::Window> {
        self.app().map(|a| a.window.clone())
    }

    // ----------------------------------------------------------------------
    // Construction

    fn populate_combos(&self) {
        let weapon: gtk::ComboBoxText = self.glade.get("weapon_combo");
        for w in Weapon::list() {
            weapon.append_text(&w.label());
        }
        let gender: gtk::ComboBoxText = self.glade.get("gender_combo");
        for g in GENDER_IMAGES {
            gender.append_text(&tr(g));
        }
        let level: gtk::ComboBoxText = self.glade.get("level_combo");
        for l in LEVEL_IMAGES {
            level.append_text(&tr(l));
        }
        let categories: gtk::ListStore = self.glade.get("category_liststore");
        for c in DISPLAY_IMAGES {
            categories.set(&categories.append(), &[(0, &tr(c))]);
        }
        let colors: gtk::ListStore = self.glade.get("color_liststore");
        for c in bp_core::contest::COLORS {
            colors.set(&colors.append(), &[(0, &c)]);
        }
    }

    fn connect_handlers(self: &Rc<Self>) {
        macro_rules! on {
            ($name:expr, $method:ident) => {{
                let w = Rc::downgrade(self);
                self.glade.connect_simple($name, move |_| {
                    if let Some(p) = w.upgrade() {
                        p.$method();
                    }
                });
            }};
        }
        on!("on_save_toolbutton_clicked", on_save);
        // Ctrl+S : enregistrer la compétition affichée.
        {
            let w = Rc::downgrade(self);
            let shortcut = gtk::Shortcut::new(
                gtk::ShortcutTrigger::parse_string("<Control>s"),
                Some(gtk::CallbackAction::new(move |_, _| {
                    if let Some(p) = w.upgrade() {
                        p.on_save();
                    }
                    gtk::glib::Propagation::Stop
                })),
            );
            let controller = gtk::ShortcutController::new();
            controller.set_scope(gtk::ShortcutScope::Managed);
            controller.add_shortcut(shortcut);
            self.root.add_controller(controller);
        }
        self.show_saved_state(true);
        on!("on_properties_toolbutton_clicked", on_properties);
        on!("on_calendar_button_clicked", on_calendar);
        on!("on_contest_close_button_clicked", on_close);
        on!("on_color_combobox_changed", on_color_changed);
        on!("on_team_radiobutton_toggled", on_team_toggled);
        on!("on_web_site_button_clicked", on_web_site);
    }

    // ----------------------------------------------------------------------
    // Propriétés (`FillInProperties` / `ReadProperties` / `DisplayProperties`)

    fn entry(&self, id: &str) -> gtk::Entry {
        self.glade.get(id)
    }

    /// Entrée de texte d'une liste déroulante éditable.
    fn combo_entry(&self, id: &str) -> Option<gtk::Entry> {
        self.glade.get::<gtk::ComboBox>(id).child().and_then(|c| c.downcast::<gtk::Entry>().ok())
    }

    fn fill_in_date(&self, day: u32, month: u32, year: u32) {
        self.date.set((day, month, year));
        let label = format!("{day:02}/{month:02}/{year}");
        self.glade.get::<gtk::Button>("calendar_button").set_label(&label);
        if let Ok(d) = glib::DateTime::from_local(year as i32, month as i32, day as i32, 0, 0, 0.0) {
            self.glade.get::<gtk::Calendar>("calendar").select_day(&d);
        }
    }

    pub fn fill_in_properties(&self) {
        let Some(data) = self.with_contest(|c| {
            (
                c.name.clone(),
                c.organizer.clone(),
                c.web_site.clone(),
                c.location.clone(),
                c.label.clone(),
                c.weapon.index() as u32,
                c.gender as u32,
                c.level as u32,
                c.settings.score_stuffing_allowed,
                c.settings.team_event,
                (c.day, c.month, c.year),
                [c.checkin_time, c.scratch_time, c.start_time],
                c.category.clone(),
            )
        }) else {
            return;
        };
        let (name, organizer, web, location, label, weapon, gender, level, stuffing, team, date, times, category) = data;
        self.entry("title_entry").set_text(&name);
        self.entry("organizer_entry").set_text(&organizer);
        self.entry("web_site_entry").set_text(&web);
        self.entry("location_entry").set_text(&location);
        if let Some(e) = self.combo_entry("label_entry") {
            e.set_text(&label);
        }
        self.glade.get::<gtk::ComboBoxText>("weapon_combo").set_active(Some(weapon));
        self.glade.get::<gtk::ComboBoxText>("gender_combo").set_active(Some(gender));
        self.glade.get::<gtk::ComboBoxText>("level_combo").set_active(Some(level));
        self.glade.get::<gtk::CheckButton>("allow_radiobutton").set_active(stuffing);
        self.glade.get::<gtk::CheckButton>("forbid_radiobutton").set_active(!stuffing);
        self.glade.get::<gtk::CheckButton>("team_radiobutton").set_active(team);
        self.glade.get::<gtk::CheckButton>("individual_radiobutton").set_active(!team);
        self.fill_in_date(date.0, date.1, date.2);
        for (prefix, t) in [("checkin", times[0]), ("scratch", times[1]), ("start", times[2])] {
            self.glade.get::<gtk::SpinButton>(&format!("{prefix}_hour_spinbutton")).set_value(t.hour as f64);
            self.glade.get::<gtk::SpinButton>(&format!("{prefix}_minute_spinbutton")).set_value(t.minute as f64);
        }
        self.fill_in_category(&category);
    }

    fn fill_in_category(&self, category: &Category) {
        let combo: gtk::ComboBox = self.glade.get("category_combobox");
        match category.index() {
            Some(i) => combo.set_active(Some(i as u32)),
            None => {
                combo.set_active(None);
                if let Some(e) = self.combo_entry("category_combobox") {
                    e.set_text(&category.display_image());
                }
            }
        }
    }

    /// `ReadProperties`.
    pub fn read_properties(&self) {
        let (day, month, year) = self.date.get();
        let text = |id: &str| self.entry(id).text().to_string();
        let name = text("title_entry");
        let organizer = text("organizer_entry");
        let web = text("web_site_entry");
        let location = text("location_entry");
        let label = self.combo_entry("label_entry").map(|e| e.text().to_string()).unwrap_or_default();
        let weapon = self.glade.get::<gtk::ComboBoxText>("weapon_combo").active().unwrap_or(0) as usize;
        let gender = self.glade.get::<gtk::ComboBoxText>("gender_combo").active().unwrap_or(0) as usize;
        let level = self.glade.get::<gtk::ComboBoxText>("level_combo").active().unwrap_or(0) as usize;
        let stuffing = self.glade.get::<gtk::CheckButton>("allow_radiobutton").is_active();
        let team = self.glade.get::<gtk::CheckButton>("team_radiobutton").is_active();
        let category_index = self.glade.get::<gtk::ComboBox>("category_combobox").active().map(|i| i as usize);
        let category_text = self.combo_entry("category_combobox").map(|e| e.text().to_string()).unwrap_or_default();
        let spin = |id: &str| self.glade.get::<gtk::SpinButton>(id).value() as u32;
        let times = ["checkin", "scratch", "start"].map(|p| bp_core::contest::ContestTime {
            hour: spin(&format!("{p}_hour_spinbutton")),
            minute: spin(&format!("{p}_minute_spinbutton")),
        });

        let Some(app) = self.app() else { return };
        let mut t = app.tournament.borrow_mut();
        // Les préférences sont modifiées en même temps que la compétition.
        let mut config = std::mem::take(&mut t.config);
        if let Some(c) = t.contest_mut(self.id) {
            c.name = name;
            c.organizer = organizer;
            c.web_site = web;
            c.location = location;
            c.label = label;
            c.weapon = Weapon::from_index(weapon).unwrap_or_else(Weapon::default_weapon);
            c.gender = gender;
            c.level = level;
            c.category.read_user_choice(category_index, &category_text);
            c.day = day;
            c.month = month;
            c.year = year;
            c.checkin_time = times[0];
            c.scratch_time = times[1];
            c.start_time = times[2];
            c.set_score_stuffing(stuffing);
            c.set_team_event(team);
            c.apply_properties(Some(&mut config));
        }
        t.config = config;
        drop(t);
        self.glade.get::<gtk::Label>("banner").set_text("");
        self.display_properties();
        if let Some(s) = self.schedule.borrow().as_ref() {
            s.refresh();
        }
    }

    /// `DisplayProperties` : textes de l'onglet.
    pub fn display_properties(&self) {
        let Some((name, weapon, gender, category)) =
            self.with_contest(|c| (c.name.clone(), c.weapon.label(), c.gender_label(), c.category_label()))
        else {
            return;
        };
        self.glade.get::<gtk::Label>("contest_name_label").set_text(&name);
        self.glade.get::<gtk::Label>("contest_weapon_label").set_text(&weapon);
        self.glade.get::<gtk::Label>("contest_gender_label").set_text(&gender);
        self.glade.get::<gtk::Label>("contest_category_label").set_text(&category);
        self.refresh_header();
    }

    pub fn widget(&self) -> gtk::Widget { self.root.clone() }

    pub fn show_properties(self: &Rc<Self>) { self.on_properties(); }

    pub fn refresh_header(&self) {
        if let Some((name, subtitle, meta, phase)) = self.try_with_contest(|c| {
            let subtitle = format!("{} · {} · {}", c.weapon.label(), c.gender_label(), c.category_label());
            let meta = [c.location.clone(), c.date_image()].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join("  |  ");
            let phase = c.schedule.current().map(|stage| tr(stage.kind().class_name())).unwrap_or_default();
            (c.name.clone(), subtitle, meta, phase)
        }) {
            self.hero.name.set_text(&name);
            self.hero.subtitle.set_text(&subtitle);
            self.hero.meta.set_text(&meta);
            self.hero.phase.set_text(&phase);
        }
    }

    /// `PaintColor` : couleur de la compétition sur l'onglet.
    fn paint_color(&self) {
        let Some(color) = self.with_contest(|c| c.color.clone()) else { return };
        self.css.load_from_string(&format!(
            ".bp-competition-summary {{ border-left: 4px solid {color}; }} .bp-event-color {{ background-color: {color}; }}"
        ));
        let combo: gtk::ComboBox = self.glade.get("color_combobox");
        if let Some(i) = bp_core::contest::COLORS.iter().position(|c| c.eq_ignore_ascii_case(&color)) {
            combo.set_active(Some(i as u32));
        }
    }

    // ----------------------------------------------------------------------
    // Gestionnaires

    fn on_color_changed(&self) {
        let combo: gtk::ComboBox = self.glade.get("color_combobox");
        let Some(i) = combo.active() else { return };
        let Some(color) = bp_core::contest::COLORS.get(i as usize) else { return };
        let changed = self.with_contest(|c| {
            let changed = !c.color.eq_ignore_ascii_case(color);
            c.color = color.to_string();
            changed
        });
        if changed == Some(true) {
            self.paint_color();
        }
    }

    fn on_team_toggled(&self) {}

    fn on_web_site(&self) {
        let url = self.entry("web_site_entry").text().to_string();
        if !url.is_empty() {
            dialogs::show_uri(&url);
        }
    }

    fn on_calendar(&self) {
        let dialog: gtk::Dialog = self.glade.get("calendar_dialog");
        let ok = dialogs::run_dialog(&dialog) == gtk::ResponseType::Other(1);
        dialog.set_visible(false);
        if ok {
            let d = self.glade.get::<gtk::Calendar>("calendar").date();
            self.fill_in_date(d.day_of_month() as u32, d.month() as u32, d.year() as u32);
        }
    }

    fn on_properties(self: &Rc<Self>) {
        self.glade.get::<gtk::Label>("banner").set_text("");
        self.fill_in_properties();
        let dialog: gtk::Dialog = self.glade.get("properties_dialog");
        dialogs::run_dialog(&dialog);
        self.read_properties();
        self.mark_dirty();
        dialog.set_visible(false);
    }

    /// `Contest::AskForSettings` : propriétés, puis nom du fichier.
    pub fn ask_for_settings(self: &Rc<Self>) {
        self.with_contest(|c| c.set_score_stuffing(false));
        self.fill_in_properties();
        let dialog: gtk::Dialog = self.glade.get("properties_dialog");
        dialogs::run_dialog(&dialog);
        self.read_properties();
        dialog.set_visible(false);
        self.choose_file_name_if_missing();
    }

    /// Demande un nom de fichier si la compétition n'en a pas, puis enregistre.
    pub fn choose_file_name_if_missing(&self) {
        if self.with_contest(|c| c.filename.is_none() && !c.read_only) == Some(true) {
            self.choose_save_name("default_dir_name");
        }
        self.save();
    }

    /// `Contest::GetSaveFileName` : renvoie la réponse de l'utilisateur.
    fn choose_save_name(&self, config_key: &str) -> gtk::ResponseType {
        let title = tr("Choose a file...");
        let mut chooser = FileChooser::new(&title, gtk::FileChooserAction::Save);
        chooser.filters.push(dialogs::file_filter(Some(&tr("BellePoule files")), &["*.cotcot"]));
        chooser.filters.push(dialogs::all_files_filter());
        let Some(app) = self.app() else { return gtk::ResponseType::Cancel };
        chooser.current_folder =
            app.tournament.borrow().config.get_string("Competiton", config_key).map(PathBuf::from).filter(|d| d.is_dir());
        chooser.current_name = self.with_contest(|c| c.default_file_name());
        let (response, file) = chooser.run();
        if response == gtk::ResponseType::Accept {
            if let Some(mut f) = file {
                if f.extension().is_none() {
                    f.set_extension("cotcot");
                }
                if let Some(dir) = f.parent() {
                    app.tournament.borrow_mut().config.set_string("Competiton", config_key, &dir.to_string_lossy());
                }
                app.tournament.borrow_mut().add_recent(&f);
                self.with_contest(|c| c.filename = Some(f.clone()));
                self.glade.get::<gtk::Widget>("save_toolbutton").set_tooltip_text(Some(&f.to_string_lossy()));
            }
        }
        response
    }

    fn on_save(&self) {
        if self.with_contest(|c| c.filename.is_none() && !c.read_only) == Some(true) {
            self.choose_save_name("default_dir_name");
        }
        self.save();
    }

    /// `Contest::Save` : fichier, copie web et sauvegarde.
    pub fn save(&self) {
        let Some(app) = self.app() else { return };
        if let Some(t) = self.save_timer.borrow_mut().take() {
            t.remove();
        }
        let has_name = self.with_contest(|c| c.filename.is_some()) == Some(true);
        if !has_name {
            return;
        }
        let result = app.tournament.borrow_mut().save_contest(self.id);
        match result {
            Ok(()) => {
                self.show_saved_state(true);
                // `Contest::Publish`
                if let Some(target) = crate::publication::target(&app.glade) {
                    self.with_contest(|c| crate::publication::publish(&target, c));
                }
            }
            Err(e) => dialogs::error(Some(&app.window), &format!("{e}")),
        }
    }

    /// Bouton « Enregistrer » : toujours cliquable, son libellé dit si la
    /// dernière version est enregistrée.
    fn show_saved_state(&self, saved: bool) {
        let button = self.glade.get::<gtk::Widget>("save_toolbutton");
        button.set_sensitive(true);
        let label = button
            .first_child()
            .and_then(|b| b.last_child())
            .and_then(|l| l.downcast::<gtk::Label>().ok());
        if let Some(label) = label {
            label.set_use_underline(false);
            label.set_text(&if saved { tr("Saved") } else { tr("Save") });
        }
        if saved {
            button.remove_css_class("suggested-action");
        } else {
            button.add_css_class("suggested-action");
        }
    }

    /// À la fermeture de l'application : enregistre la compétition si elle a
    /// un fichier. Renvoie vrai s'il reste des changements non enregistrés.
    pub fn save_before_quit(&self) -> bool {
        let named = self.with_contest(|c| c.filename.is_some() && !c.read_only) == Some(true);
        if named {
            self.save();
        }
        self.with_contest(|c| c.dirty && !c.read_only) == Some(true)
    }

    /// Nom affiché de la compétition.
    pub fn display_name(&self) -> String {
        self.with_contest(|c| c.name.clone()).unwrap_or_default()
    }

    /// `MakeDirty` : l'enregistrement automatique part 5 secondes plus tard.
    pub fn mark_dirty(self: &Rc<Self>) {
        self.with_contest(|c| c.mark_dirty());
        self.show_saved_state(false);
        if let Some(t) = self.save_timer.borrow_mut().take() {
            t.remove();
        }
        let weak = Rc::downgrade(self);
        let id = glib::timeout_add_seconds_local_once(SAVE_DELAY, move || {
            if let Some(p) = weak.upgrade() {
                *p.save_timer.borrow_mut() = None;
                p.save();
            }
        });
        *self.save_timer.borrow_mut() = Some(id);
    }

    /// `on_contest_close_button_clicked`.
    fn on_close(self: &Rc<Self>) {
        let mut cancel = false;
        if self.with_contest(|c| c.filename.is_none() && !c.read_only) == Some(true) {
            let title = tr("Choose a file...");
            let mut chooser = FileChooser::new(&title, gtk::FileChooserAction::Save);
            chooser.buttons = vec![
                (tr("_Cancel"), gtk::ResponseType::Cancel),
                (tr("Close without saving"), gtk::ResponseType::Reject),
                (tr("_Save"), gtk::ResponseType::Accept),
            ];
            chooser.filters.push(dialogs::file_filter(Some(&tr("BellePoule files")), &["*.cotcot"]));
            chooser.filters.push(dialogs::all_files_filter());
            chooser.current_name = self.with_contest(|c| c.default_file_name());
            let (response, file) = chooser.run();
            match response {
                gtk::ResponseType::Cancel | gtk::ResponseType::DeleteEvent | gtk::ResponseType::None => cancel = true,
                gtk::ResponseType::Accept => {
                    if let Some(mut f) = file {
                        if f.extension().is_none() {
                            f.set_extension("cotcot");
                        }
                        self.with_contest(|c| c.filename = Some(f));
                    }
                }
                _ => {}
            }
        }
        if cancel {
            return;
        }
        self.save();
        if let Some(app) = self.app() {
            app.close_page(self.id);
        }
    }

    /// Retire la page du carnet de l'application.
    /// Vrai si cet onglet est celui affiché.
    pub fn is_current(&self, notebook: &gtk::Notebook) -> bool {
        notebook.page_num(&self.root).is_some() && notebook.page_num(&self.root) == notebook.current_page()
    }

    /// En-tête des impressions (`Contest::DrawPage`).
    pub fn print_header(&self) -> Option<bp_gtk::print::PageHeader> {
        self.with_contest(|c| bp_gtk::print::PageHeader {
            name: c.name.clone(),
            subtitle: format!("{} - {} - {}", c.weapon.label(), c.gender_label(), c.category.display_image()),
            organizer: c.organizer.clone(),
            date: c.date_image(),
            color: c.color.clone(),
        })
    }

    pub fn detach_from(&self, notebook: &gtk::Notebook) {
        if let Some(n) = notebook.page_num(&self.root) {
            notebook.remove_page(Some(n));
        }
        let _ = &self.title;
    }
}
