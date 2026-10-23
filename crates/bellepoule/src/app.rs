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

/// Site de ManageComp (documentation, téléchargement).
const WEB_SITE: &str = "https://managecomp.org/";
/// Sources de ManageComp.
const SOURCE_SITE: &str = "https://github.com/BenribsLab";

pub struct App {
    pub gtk_app: gtk::Application,
    pub glade: Glade,
    pub window: gtk::Window,
    pub notebook: gtk::Notebook,
    pub tournament: RefCell<Tournament>,
    pub pages: RefCell<Vec<Rc<ContestPage>>>,
    /// Liaison avec le Marshaller.
    pub network: RefCell<Option<Rc<crate::network::Network>>>,
    /// Liaison SmartPoule (serveur BellePoule Web).
    pub smartpoule: RefCell<Option<Rc<crate::smartpoule::SmartPoule>>>,
    pub management: RefCell<Option<Rc<crate::management::Management>>>,
    management_return: RefCell<Option<gtk::Widget>>,
    pub shell: RefCell<Option<Rc<crate::shell::Shell>>>,
    /// Les messages de confirmation de sortie sont déjà passés.
    quitting: std::cell::Cell<bool>,
}

impl App {
    pub fn start(gtk_app: &gtk::Application, file: Option<PathBuf>, smoke_test: bool, capture: Option<crate::capture::Shot>) {
        bp_gtk::appearance::install();
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
                eprintln!("ManageComp : impossible de charger l'interface : {}", e.0);
                gtk_app.quit();
                return;
            }
        };
        let window: gtk::Window = glade.get("root");
        let notebook: gtk::Notebook = glade.get("notebook");
        window.set_application(Some(gtk_app));
        dialogs::set_main_window(&window);
        dialogs::keep_focus_in_app();
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
            network: RefCell::new(None),
            smartpoule: RefCell::new(None),
            management: RefCell::new(None),
            management_return: RefCell::new(None),
            shell: RefCell::new(None),
            quitting: std::cell::Cell::new(false),
        });
        app.connect_handlers();
        app.prepare_dialogs();
        app.setup_printing();
        app.update_visibility();
        if !smoke_test {
            *app.network.borrow_mut() = Some(crate::network::Network::start(&app));
            if capture.is_none() {
                *app.smartpoule.borrow_mut() = Some(crate::smartpoule::SmartPoule::start(&app));
            }
        }
        let management = crate::management::Management::new(&app);
        *app.management.borrow_mut() = Some(management);
        let shell = crate::shell::Shell::new(&app);
        *app.shell.borrow_mut() = Some(shell);
        app.update_visibility();

        window.set_default_size(1200, 800);
        window.present();

        if let Some(f) = file {
            app.open_path(&f);
        }
        if let Some(shot) = capture {
            crate::capture::run(&app, shot);
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

    /// Mise en page des impressions (préférences) et en-tête de la
    /// compétition affichée.
    fn setup_printing(self: &Rc<Self>) {
        let layout = bp_gtk::print::PrintLayout::load(&self.tournament.borrow().config);
        let w = self.weak();
        bp_gtk::print::init_layout(layout, move |l| {
            if let Some(a) = w.upgrade() {
                let mut t = a.tournament.borrow_mut();
                l.save(&mut t.config);
                let _ = t.save_config();
            }
        });
        let w = self.weak();
        bp_gtk::print::set_header_provider(move || {
            let a = w.upgrade()?;
            let pages = a.pages.borrow();
            let page = pages.iter().find(|p| p.is_current(&a.notebook))?.clone();
            drop(pages);
            page.print_header()
        });
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
        on!("on_ftp_menuitem_activate", on_publication);
        on!("on_access_code_activate", on_access_code);
        on!("on_smartpoule_menuitem_activate", on_smartpoule);
        on!("on_new_menuitem_activate", on_new);
        on!("on_youth_menuitem_activate", on_new_youth);
        on!("on_open_menuitem_activate", on_open);
        on!("on_recent_menuitem_activate", on_recent);
        on!("on_reload_menuitem_activate", on_reload);
        on!("on_example_menuitem_activate", on_example);
        on!("on_template_imagemenuitem_activate", on_template);
        on!("on_backup_location_menuitem_activate", on_backup_location);
        on!("on_about_menuitem_activate", on_about);
        on!("on_user_manual_activate", on_user_manual);

        {
            let w = self.weak();
            let current = language::current(&self.tournament.borrow().config);
            language::populate_menu(&self.glade, &current, move |code| {
                if let Some(a) = w.upgrade() {
                    language::remember(&mut a.tournament.borrow_mut().config, code);
                }
            });
        }
        let open_web = |url: String| move |_: &Signal| dialogs::show_uri(&url);
        self.glade.connect_simple("on_translate_menuitem_activate", open_web(format!("{SOURCE_SITE}")));
        let download = if language::current(&self.tournament.borrow().config) == "fr" {
            format!("{WEB_SITE}telecharger/")
        } else {
            format!("{WEB_SITE}en/download/")
        };
        self.glade.connect_simple("on_new_version_menuitem_activate", open_web(download));
        self.glade.connect_simple(
            "on_release_notes_activate",
            open_web(format!("{SOURCE_SITE}/managecomp-rust/releases")),
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
        crate::publication::setup(&self.glade);
        // Diffusion vidéo : non disponible.
        for item in ["video_menuitem"] {
            if self.glade.find_menu_item(item).is_some() {
                self.glade.set_menu_item_sensitive(item, false);
            }
        }
    }

    /// `on_ftp_menuitem_activate` : dialogue « Publication ».
    fn on_publication(self: &Rc<Self>) {
        let dialog: gtk::Dialog = self.glade.get("publication_dialog");
        dialog.set_transient_for(Some(&self.window));
        dialogs::run_dialog(&dialog);
        dialog.set_visible(false);
    }

    /// BellePoule2D > SmartPoule… : téléphones des arbitres.
    fn on_smartpoule(self: &Rc<Self>) {
        crate::smartpoule::show_dialog(self);
    }

    /// `Tournament::OnShowAccessCode` : QR code d'appairage (BellePoule2D).
    fn on_access_code(self: &Rc<Self>) {
        use bp_net::flash_code::FlashCode;
        self.glade.get::<gtk::Widget>("step1").set_visible(false);
        if let Some(w) = self.glade.widget("step2") {
            w.set_visible(false);
        }
        if let Some(ring) = bp_net::ring::Ring::broker() {
            let text = ring.flash_code().text();
            if let Some(png) = bp_net::flash_code::QrMatrix::encode(&text).map(|m| m.to_image(4).to_png()) {
                if let Ok(texture) = gtk::gdk::Texture::from_bytes(&glib::Bytes::from_owned(png)) {
                    self.glade.get::<gtk::Image>("qrcode_image").set_paintable(Some(&texture));
                }
            }
        }
        let dialog: gtk::Dialog = self.glade.get("pin_code_dialog");
        dialog.set_transient_for(Some(&self.window));
        dialogs::run_dialog(&dialog);
        dialog.set_visible(false);
    }

    /// Texte du dialogue « À propos » et menu des langues.
    fn prepare_dialogs(self: &Rc<Self>) {
        let about: gtk::AboutDialog = self.glade.get("about_dialog");
        about.set_version(Some(&format!("V{VERSION}.{VERSION_REVISION}{VERSION_MATURITY}")));
        about.set_translator_credits(Some(TRANSLATORS));
        // Logiciel libre dérivé de BellePoule : les mentions d'origine sont conservées.
        about.set_program_name(Some("ManageComp"));
        about.set_comments(Some("Logiciel libre de gestion de compétitions d'escrime.\n\nDérivé de BellePoule, de Yannick Le Roux."));
        about.set_copyright(Some("© 2026 les contributeurs de ManageComp\nBellePoule © 2009-2017 Yannick Le Roux (亚尼可)"));
        about.set_website(Some(WEB_SITE));
        about.set_website_label("managecomp.org");
        about.add_credit_section("BellePoule", &["Yannick Le Roux http://betton.escrime.free.fr"]);
        about.set_hide_on_close(true);
        if let Some(logo) = self.glade.object::<gtk::Picture>("about_logo") {
            let _ = logo;
        }
    }

    /// Logo seul tant qu'aucune compétition n'est ouverte.
    fn update_visibility(&self) {
        let empty = self.notebook.n_pages() == 0;
        self.notebook.set_visible(!empty);
        if let Some(logo) = self.glade.widget("logo") {
            logo.set_visible(empty);
        }
        if let Some(shell) = self.shell.borrow().clone() { shell.refresh(); }
    }

    pub fn open_management(&self) {
        let Some(manager) = self.management.borrow().clone() else { return };
        let current = self.notebook.current_page()
            .and_then(|index| self.notebook.nth_page(Some(index)));
        if current.as_ref().is_some_and(|page| page != manager.widget().upcast_ref::<gtk::Widget>()) {
            *self.management_return.borrow_mut() = current;
        }
        if self.notebook.page_num(manager.widget()).is_none() {
            self.notebook.append_page(manager.widget(), Some(&gtk::Label::new(Some(&tr("Organisation")))));
        }
        self.notebook.set_current_page(self.notebook.page_num(manager.widget()));
        self.update_visibility();
    }

    pub fn close_management(&self) {
        let Some(manager) = self.management.borrow().clone() else { return };
        manager.flush();
        if let Some(index) = self.notebook.page_num(manager.widget()) {
            self.notebook.remove_page(Some(index));
        }
        let previous = self.management_return.borrow_mut().take();
        if let Some(index) = previous.as_ref().and_then(|page| self.notebook.page_num(page)) {
            self.notebook.set_current_page(Some(index));
        }
        self.update_visibility();
    }

    pub fn active_contest(&self) -> Option<Rc<ContestPage>> {
        let pages = self.pages.borrow().clone();
        pages.iter().find(|page| page.is_current(&self.notebook)).cloned()
            .or_else(|| {
                let previous = self.management_return.borrow();
                pages.iter().find(|page| previous.as_ref() == Some(&page.widget())).cloned()
            })
            .or_else(|| pages.last().cloned())
    }

    pub fn navigate_to(&self, kinds: &[bp_core::stage::StageKind]) {
        let Some(page) = self.active_contest() else { return };
        let schedule = page.schedule.borrow().clone();
        if let Some(schedule) = schedule {
            if schedule.show_stage(kinds) {
                self.notebook.set_current_page(self.notebook.page_num(&page.widget()));
                self.update_visibility();
            }
        }
    }

    pub fn open_contest_preferences(&self) {
        if let Some(page) = self.active_contest() { page.show_properties(); }
    }

    pub fn resume_contest(&self) {
        if let Some(page) = self.active_contest() {
            self.notebook.set_current_page(self.notebook.page_num(&page.widget()));
            self.update_visibility();
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
    /// Vignette de l'accueil : revient sur la compétition si elle est déjà
    /// ouverte, sinon l'ouvre.
    pub fn open_recent(self: &Rc<Self>, path: &Path) {
        let pages = self.pages.borrow().clone();
        let open = pages.into_iter().find(|p| p.with_contest(|c| c.filename.as_deref() == Some(path)).unwrap_or(false));
        match open {
            Some(page) => {
                self.notebook.set_current_page(self.notebook.page_num(&page.widget()));
                self.update_visibility();
            }
            None => self.open_path(path),
        }
    }

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
        let page = {
            let mut pages = self.pages.borrow_mut();
            let index = pages.iter().position(|p| p.id == id);
            index.map(|i| pages.remove(i))
        };
        // Ne conserver aucun emprunt pendant le retrait GTK : les signaux
        // de changement d'onglet peuvent consulter à nouveau les pages.
        if let Some(page) = page {
            page.detach_from(&self.notebook);
        }
        self.tournament.borrow_mut().close(id);
        self.update_visibility();
    }

    // ----------------------------------------------------------------------
    // Menus

    /// `Tournament::OnNew`.
    pub fn on_new(self: &Rc<Self>) {
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

    /// Entraînement des jeunes (extension) : nouvelle compétition dont la
    /// formule répartit les tireurs en groupes.
    pub fn on_new_youth(self: &Rc<Self>) {
        let id = self.tournament.borrow_mut().new_youth_training();
        if let Some(page) = self.add_page(id) {
            page.ask_for_settings();
        }
    }

    pub fn on_open(self: &Rc<Self>) {
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
                chooser.filters.push(dialogs::file_filter(Some(&tr("ManageComp files")), &["*.cotcot"]));
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
    pub fn on_user_manual(self: &Rc<Self>) {
        let code = language::current(&self.tournament.borrow().config);
        let pdf = resources::translations_dir().join(&code).join("user_manual.pdf");
        if pdf.is_file() {
            dialogs::show_uri(&pdf.to_string_lossy());
        } else {
            let page = if code == "fr" { "docs/" } else { "en/docs/" };
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
        let management = self.management.borrow().clone();
        if let Some(management) = management { management.flush(); }
        // Les compétitions qui ont un fichier sont enregistrées ; on ne
        // demande confirmation que s'il en reste de non enregistrées.
        let pages: Vec<Rc<ContestPage>> = self.pages.borrow().clone();
        let unsaved: Vec<String> = pages.iter().filter(|p| p.save_before_quit()).map(|p| p.display_name()).collect();
        let ok = unsaved.is_empty()
            || dialogs::confirm(
                Some(&self.window),
                &tr("<b><big>Do you really want to quit ManageComp</big></b>"),
                Some(&format!("{}\n{}", tr("These competitions are not saved:"), unsaved.join("\n"))),
                Some(&tr("Quit ManageComp?")),
            );
        if ok {
            self.quitting.set(true);
            let _ = self.tournament.borrow().save_config();
            if let Some(n) = self.network.borrow().as_ref() {
                n.leave();
            }
            self.window.destroy();
            self.gtk_app.quit();
        }
    }
}

const TRANSLATORS: &str = "Julien Diaz           (German)\nTina Schliemann       (German)\nMichael Weber         (German)\nFranz Lewin Wagner    (German)\nDom Walden            (English\nAureliano Martini     (Italian)\nFrancesco Nazzani     (Italian)\nJihwan Cho            (Korean)\nMarijn Somers         (Dutch)\nWerner Huysmans       (Dutch)\nFarid Elkhalki        (Japanese)\nJean-François Brun    (Japanese)\nAdolpho Jayme         (Spanish)\nAngel Oliver          (Spanish, Catalan)\nAlexis Pigeon         (Spanish)\nPaulo Morales         (Spanish)\nIgnacio Gil           (Spanish)\nStefano Schirone      (Spanish)\nDominik Denkiewicz    (Polish)\nEduardo Alberto Calvo (Spanish, Portuguese)\nRicardo Catanho       (Portuguese)\nRui Pedro Vieira      (Portuguese)\nMohamed Rebai         (Arabic)\nMikael Hiort af Ornäs (Swedish)\nSergev Makhtanov      (Russian)Alcedo Attis          (Hungarian)\nSimon Dobelsek        (Slovenian)\nIbon Santisteban      (Basque)\nBen Haeberli          (German)";
