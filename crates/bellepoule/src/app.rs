//! Fenêtre principale (port de `supervisor/tournament.cpp` et
//! `application/application.cpp`, parties interface).

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};

use bp_core::contest::{VERSION, VERSION_MATURITY, VERSION_REVISION};
use bp_core::i18n::tr;
use bp_core::tournament::{ContestId, Opened, Tournament};
use bp_gtk::dialogs::{self, FileChooser};
use bp_gtk::glade::{Glade, Signal};
use bp_gtk::resources;
use gtk4 as gtk;
use gtk::glib;
use gtk::prelude::*;

use crate::contest_page::ContestPage;
use crate::language;

/// Réponses du dialogue « Nouvelle compétition » (`CreateNewContest`).
const NEW_EMPTY: u16 = b'E' as u16;
const NEW_XML: u16 = b'X' as u16;
const NEW_ZIP: u16 = b'Z' as u16;
const NEW_ASKFRED: u16 = b'A' as u16;

const WEB_SITE: &str = "http://betton.escrime.free.fr/fencing-tournament-software/";

pub struct App {
    pub gtk_app: gtk::Application,
    pub glade: Glade,
    pub window: gtk::Window,
    pub notebook: gtk::Notebook,
    pub tournament: RefCell<Tournament>,
    pub pages: RefCell<Vec<Rc<ContestPage>>>,
    /// Les messages de confirmation de sortie sont déjà passés.
    quitting: std::cell::Cell<bool>,
}

impl App {
    pub fn start(gtk_app: &gtk::Application, file: Option<PathBuf>, smoke_test: bool) {
        // Ressources et valeurs discrètes (pays, clubs, ligues…)
        let resources_dir = resources::resources_dir().to_path_buf();
        bp_core::contest::players::set_resources_dir(resources_dir.clone());

        let mut tournament = Tournament::with_default_config();
        let lang = language::current(&tournament.config);
        bp_core::formats::discrete::init(&resources_dir, &lang);
        let share = resources::share_dir();
        tournament.examples_dir = [share.join("Exemples"), share.join("tests-data")].into_iter().find(|d| d.is_dir());
        language::activate(&language::current(&tournament.config));

        let glade = match Glade::new("tournament.ui") {
            Ok(g) => g,
            Err(e) => {
                eprintln!("BellePoule : impossible de charger l'interface : {}", e.0);
                gtk_app.quit();
                return;
            }
        };
        let window: gtk::Window = glade.get("root");
        let notebook: gtk::Notebook = glade.get("notebook");
        window.set_application(Some(gtk_app));
        dialogs::set_main_window(&window);
        glade.attach_actions(&window);
        for (action, accel) in glade.accels() {
            gtk_app.set_accels_for_action(&action, &[accel.as_str()]);
        }

        let app = Rc::new(App {
            gtk_app: gtk_app.clone(),
            glade,
            window: window.clone(),
            notebook,
            tournament: RefCell::new(tournament),
            pages: RefCell::new(Vec::new()),
            quitting: std::cell::Cell::new(false),
        });
        app.connect_handlers();
        app.prepare_dialogs();
        app.update_visibility();

        window.set_default_size(1000, 700);
        window.present();

        if let Some(f) = file {
            app.open_path(&f);
        }

        if smoke_test {
            let a = gtk_app.clone();
            glib::timeout_add_local_once(std::time::Duration::from_millis(1000), move || {
                println!("smoke-test OK");
                a.quit();
            });
        }
        // L'objet vit aussi longtemps que la fenêtre.
        unsafe { window.set_data("bellepoule-app", app) };
    }

    fn weak(self: &Rc<Self>) -> Weak<App> {
        Rc::downgrade(self)
    }

    /// Branche les gestionnaires de `tournament.ui`.
    fn connect_handlers(self: &Rc<Self>) {
        macro_rules! on {
            ($name:expr, $method:ident) => {{
                let w = self.weak();
                self.glade.connect_simple($name, move |_| {
                    if let Some(a) = w.upgrade() {
                        a.$method();
                    }
                });
            }};
        }
        on!("on_new_menuitem_activate", on_new);
        on!("on_open_menuitem_activate", on_open);
        on!("on_recent_menuitem_activate", on_recent);
        on!("on_reload_menuitem_activate", on_reload);
        on!("on_example_menuitem_activate", on_example);
        on!("on_template_imagemenuitem_activate", on_template);
        on!("on_backup_location_menuitem_activate", on_backup_location);
        on!("on_about_menuitem_activate", on_about);
        on!("on_user_manual_activate", on_user_manual);

        let open_web = |url: String| move |_: &Signal| dialogs::show_uri(&url);
        self.glade.connect_simple("on_translate_menuitem_activate", open_web(format!("{WEB_SITE}en/translate")));
        let download = if language::current(&self.tournament.borrow().config) == "fr" {
            format!("{WEB_SITE}fr/telechargement-v{VERSION}")
        } else {
            format!("{WEB_SITE}en/download-v{VERSION}")
        };
        self.glade.connect_simple("on_new_version_menuitem_activate", open_web(download));
        self.glade.connect_simple(
            "on_release_notes_activate",
            open_web(format!("https://git.launchpad.net/bellepoule/log/?h={VERSION}.0/master")),
        );

        {
            let w = self.weak();
            self.glade.connect_simple("on_activate_radiomenuitem_toggled", move |s| {
                if let (Some(a), Some(active)) = (w.upgrade(), s.active) {
                    a.tournament.borrow_mut().set_backup_active(active);
                    if active && a.tournament.borrow().backup_location().is_none() {
                        a.on_backup_location();
                    }
                }
            });
        }
        {
            let w = self.weak();
            self.glade.connect("on_root_delete_event", move |_| {
                if let Some(a) = w.upgrade() {
                    a.on_quit();
                }
                Some(true.to_value())
            });
        }
        {
            // Boutons du dialogue « Nouvelle compétition » : la réponse est le
            // code de la méthode choisie.
            let w = self.weak();
            self.glade.connect_simple("on_new_clicked", move |s| {
                let Some(a) = w.upgrade() else { return };
                let code = match s.object_id.as_deref() {
                    Some("new_empty") => NEW_EMPTY,
                    Some("new_xml") => NEW_XML,
                    Some("new_zip") => NEW_ZIP,
                    Some("new_askfred") => NEW_ASKFRED,
                    _ => return,
                };
                let dialog: gtk::Dialog = a.glade.get("new_messagedialog");
                dialog.response(gtk::ResponseType::Other(code));
            });
        }
        // Fonctions réseau : disponibles avec la liaison réseau.
        for item in ["ftp_menuitem", "access_code_menuitem", "ring_menuitem", "video_menuitem"] {
            if self.glade.find_menu_item(item).is_some() {
                self.glade.set_menu_item_sensitive(item, false);
            }
        }
    }

    /// Texte du dialogue « À propos » et menu des langues.
    fn prepare_dialogs(self: &Rc<Self>) {
        let about: gtk::AboutDialog = self.glade.get("about_dialog");
        about.set_version(Some(&format!("V{VERSION}.{VERSION_REVISION}{VERSION_MATURITY}")));
        about.set_translator_credits(Some(TRANSLATORS));
        about.set_hide_on_close(true);
        if let Some(logo) = self.glade.object::<gtk::Picture>("about_logo") {
            let _ = logo;
        }
    }

    /// Logo seul tant qu'aucune compétition n'est ouverte.
    fn update_visibility(&self) {
        let empty = self.pages.borrow().is_empty();
        self.notebook.set_visible(!empty);
        if let Some(logo) = self.glade.widget("logo") {
            logo.set_visible(empty);
        }
    }

    // ----------------------------------------------------------------------
    // Compétitions

    fn add_page(self: &Rc<Self>, id: ContestId) -> Option<Rc<ContestPage>> {
        let page = ContestPage::new(self, id)?;
        self.pages.borrow_mut().push(page.clone());
        self.update_visibility();
        Some(page)
    }

    /// `Tournament::OpenUriContest`.
    pub fn open_path(self: &Rc<Self>, path: &Path) {
        let result = self.tournament.borrow_mut().open(path);
        match result {
            Ok(opened) => self.after_open(opened, path),
            Err(e) => dialogs::error(Some(&self.window), &format!("{}: {e}", path.display())),
        }
    }

    fn after_open(self: &Rc<Self>, opened: Opened, path: &Path) {
        let mut pages = Vec::new();
        for id in &opened.contests {
            if let Some(p) = self.add_page(*id) {
                pages.push(p);
            }
        }
        if opened.askfred {
            dialogs::info(
                Some(&self.window),
                "AskFred import done",
                Some("Adjust the imported data if needed and \ndon't forget to save all the created events!"),
            );
        }
        for page in pages {
            if opened.ask_for_settings {
                page.ask_for_settings();
            } else if opened.needs_file_name {
                page.choose_file_name_if_missing();
            }
        }
        let _ = path;
    }

    pub fn page_for(&self, id: ContestId) -> Option<Rc<ContestPage>> {
        self.pages.borrow().iter().find(|p| p.id == id).cloned()
    }

    /// Affiche une compétition que le modèle du tournoi vient de créer.
    pub fn show_managed_contest(self: &Rc<Self>, id: ContestId) -> Option<Rc<ContestPage>> {
        if let Some(page) = self.page_for(id) { return Some(page); }
        if self.tournament.borrow().contest(id).is_none() { return None; }
        self.add_page(id)
    }

    /// Retire l'onglet d'une compétition fermée.
    pub fn close_page(&self, id: ContestId) {
        if let Some(i) = self.pages.borrow().iter().position(|p| p.id == id) {
            let page = self.pages.borrow_mut().remove(i);
            page.detach_from(&self.notebook);
        }
        self.tournament.borrow_mut().close(id);
        self.update_visibility();
    }

    // ----------------------------------------------------------------------
    // Menus

    /// `Tournament::OnNew`.
    fn on_new(self: &Rc<Self>) {
        let dialog: gtk::Dialog = self.glade.get("new_messagedialog");
        let response = dialogs::run_dialog(&dialog);
        dialog.set_visible(false);
        match response {
            gtk::ResponseType::Other(NEW_EMPTY) => {
                let id = self.tournament.borrow_mut().new_contest();
                if let Some(page) = self.add_page(id) {
                    page.ask_for_settings();
                }
            }
            gtk::ResponseType::Other(NEW_XML) => self.choose_and_open(Some("*.xml")),
            gtk::ResponseType::Other(NEW_ZIP) => self.choose_and_open(Some("*.zip")),
            gtk::ResponseType::Other(NEW_ASKFRED) => self.choose_and_open(Some("*.frd")),
            _ => {}
        }
    }

    fn on_open(self: &Rc<Self>) {
        self.choose_and_open(None);
    }

    /// `Tournament::OnOpen`.
    fn choose_and_open(self: &Rc<Self>, suffix: Option<&str>) {
        self.choose_and_open_in(None, suffix);
    }

    fn choose_and_open_in(self: &Rc<Self>, folder: Option<PathBuf>, suffix: Option<&str>) {
        let title = tr("Choose a competion file to open... ");
        let mut chooser = FileChooser::new(&title, gtk::FileChooserAction::Open);
        match suffix {
            Some("examples") => {
                chooser.filters.push(dialogs::file_filter(Some(&tr("Example files")), &["*.cotcot", "*.zip", "*.xml", "*.frd"]));
            }
            Some(s) => {
                chooser.filters.push(dialogs::file_filter(None, &[s]));
                chooser.filters.push(dialogs::file_filter(Some(&tr("Competition files")), &["*.zip", "*.xml", "*.frd"]));
            }
            None => {
                chooser.filters.push(dialogs::file_filter(Some(&tr("BellePoule files")), &["*.cotcot"]));
                chooser.filters.push(dialogs::file_filter(Some(&tr("Competition files")), &["*.cotcot", "*.zip", "*.xml", "*.frd"]));
            }
        }
        chooser.filters.push(dialogs::file_filter(Some(&tr("Fencer files")), &["*.fff", "*.csv"]));
        chooser.filters.push(dialogs::all_files_filter());
        chooser.current_folder = folder.or_else(|| {
            self.tournament.borrow().config.get_string("Competiton", "default_dir_name").map(PathBuf::from).filter(|d| d.is_dir())
        });
        let (response, file) = chooser.run();
        if response == gtk::ResponseType::Accept {
            if let Some(f) = file {
                self.open_path(&f);
            }
        }
    }

    /// `Tournament::OnRecent` : liste des fichiers ouverts récemment.
    fn on_recent(self: &Rc<Self>) {
        let files = self.tournament.borrow().recent_files();
        let dialog = gtk::Dialog::with_buttons(
            Some(&tr("Recently opened")),
            Some(&self.window),
            gtk::DialogFlags::MODAL | gtk::DialogFlags::DESTROY_WITH_PARENT,
            &[(&tr("_Cancel"), gtk::ResponseType::Cancel)],
        );
        let list = gtk::ListBox::new();
        list.set_selection_mode(gtk::SelectionMode::Single);
        for f in &files {
            let name = f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            let row = gtk::Label::new(Some(&name));
            row.set_xalign(0.0);
            row.set_tooltip_text(Some(&f.to_string_lossy()));
            row.set_margin_top(4);
            row.set_margin_bottom(4);
            list.append(&row);
        }
        {
            let d = dialog.clone();
            list.connect_row_activated(move |_, row| d.response(gtk::ResponseType::Other(row.index() as u16 + 1)));
        }
        let scroll = gtk::ScrolledWindow::new();
        scroll.set_child(Some(&list));
        scroll.set_min_content_width(500);
        scroll.set_min_content_height(250);
        dialog.content_area().append(&scroll);
        let response = dialogs::run_dialog(&dialog);
        dialog.destroy();
        if let gtk::ResponseType::Other(n) = response {
            if let Some(f) = files.get(n as usize - 1) {
                self.open_path(f);
            }
        }
    }

    /// `Tournament::OnReload`.
    fn on_reload(self: &Rc<Self>) {
        let dialog: gtk::Dialog = self.glade.get("reload_dialog");
        let answer = dialogs::run_dialog(&dialog);
        dialog.set_visible(false);
        if answer == gtk::ResponseType::Yes {
            self.tournament.borrow_mut().reload_fencers();
        }
    }

    /// `Tournament::OnOpenExample`.
    fn on_example(self: &Rc<Self>) {
        let dir = self.tournament.borrow().examples_dir.clone();
        self.choose_and_open_in(dir, Some("examples"));
    }

    /// `Tournament::OnOpenTemplate` : modèle CSV de liste de tireurs.
    fn on_template(self: &Rc<Self>) {
        let dir = glib::user_special_dir(glib::UserDirectory::Templates)
            .or_else(|| glib::user_special_dir(glib::UserDirectory::Documents))
            .unwrap_or_else(glib::home_dir);
        let file = dir.join("fencer_file_template.csv");
        let content = self.tournament.borrow().csv_template();
        match std::fs::write(&file, content) {
            Ok(()) => dialogs::show_uri(&file.to_string_lossy()),
            Err(e) => dialogs::error(Some(&self.window), &format!("{}: {e}", file.display())),
        }
    }

    /// `Tournament::OnBackupfileLocation`.
    fn on_backup_location(self: &Rc<Self>) {
        let title = tr("Choose a backup files location...");
        let mut chooser = FileChooser::new(&title, gtk::FileChooserAction::SelectFolder);
        chooser.current_folder = self.tournament.borrow().last_backup_location();
        let (response, folder) = chooser.run();
        if response == gtk::ResponseType::Accept {
            if let Some(f) = folder {
                self.tournament.borrow_mut().set_backup_location(&f);
                self.glade.set_menu_item_label("backup_location_menuitem", &f.to_string_lossy());
                self.glade.set_menu_item_active("activate_radiomenuitem", true);
            }
        }
    }

    fn on_about(self: &Rc<Self>) {
        let about: gtk::AboutDialog = self.glade.get("about_dialog");
        about.set_transient_for(Some(&self.window));
        about.present();
    }

    /// `Application::OnOpenUserManual`.
    fn on_user_manual(self: &Rc<Self>) {
        let code = language::current(&self.tournament.borrow().config);
        let pdf = resources::translations_dir().join(&code).join("user_manual.pdf");
        if pdf.is_file() {
            dialogs::show_uri(&pdf.to_string_lossy());
        } else {
            let page = if code == "fr" { "fr/documentation" } else { "en/documentation" };
            dialogs::show_uri(&format!("{WEB_SITE}{page}"));
        }
    }

    // ----------------------------------------------------------------------
    // Fermeture

    /// `BellPouleApp::OnQuit`.
    pub fn on_quit(self: &Rc<Self>) {
        if self.quitting.get() {
            return;
        }
        let ok = dialogs::confirm(
            Some(&self.window),
            &tr("<b><big>Do you really want to quit BellePoule</big></b>"),
            Some(&tr("All the unsaved competions will be lost.")),
            Some(&tr("Quit BellePoule?")),
        );
        if ok {
            self.quitting.set(true);
            let _ = self.tournament.borrow().save_config();
            self.window.destroy();
            self.gtk_app.quit();
        }
    }
}

const TRANSLATORS: &str = "Julien Diaz           (German)\nTina Schliemann       (German)\nMichael Weber         (German)\nFranz Lewin Wagner    (German)\nDom Walden            (English\nAureliano Martini     (Italian)\nFrancesco Nazzani     (Italian)\nJihwan Cho            (Korean)\nMarijn Somers         (Dutch)\nWerner Huysmans       (Dutch)\nFarid Elkhalki        (Japanese)\nJean-François Brun    (Japanese)\nAdolpho Jayme         (Spanish)\nAngel Oliver          (Spanish, Catalan)\nAlexis Pigeon         (Spanish)\nPaulo Morales         (Spanish)\nIgnacio Gil           (Spanish)\nStefano Schirone      (Spanish)\nDominik Denkiewicz    (Polish)\nEduardo Alberto Calvo (Spanish, Portuguese)\nRicardo Catanho       (Portuguese)\nRui Pedro Vieira      (Portuguese)\nMohamed Rebai         (Arabic)\nMikael Hiort af Ornäs (Swedish)\nSergev Makhtanov      (Russian)Alcedo Attis          (Hungarian)\nSimon Dobelsek        (Slovenian)\nIbon Santisteban      (Basque)\nBen Haeberli          (German)";
