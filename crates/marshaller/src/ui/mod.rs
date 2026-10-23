#![allow(deprecated)]

//! Interface du Marshaller (port de `marshaller/marshaller.cpp` et des
//! modules graphiques `hall`, `piste`, `timeline`, `competition`, `batch`,
//! `batch_panel` et `RefereesList`).
//!
//! Le modèle (`Hall`) est partagé par les vues. Après chaque action, `Ui`
//! envoie les ordres réseau produits par le modèle, traite ses
//! notifications puis réaffiche les vues.

mod batches;
mod draw;
mod hall_view;
mod referees_view;
mod pool_resources;
mod job_referees;
mod group_dialog;
mod embedded_layout;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bp_core::i18n::tr;
use bp_gtk::{dialogs, glade::Glade};
use bp_net::ring::{HandshakeResult, Ring, RingConfig, RingEvent, Role};
use gtk::prelude::*;
use gtk4 as gtk;

use crate::hall::Hall;
use crate::model::{NetCommand, UiEvent};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ManagementFeatures {
    pub referees: bool,
    pub pistes: bool,
}

impl ManagementFeatures {
    pub const ALL: Self = Self { referees: true, pistes: true };
    pub fn enabled(self) -> bool { self.referees || self.pistes }
}

pub struct Ui {
    local_output: Option<Rc<dyn Fn(NetCommand)>>,
    features: std::cell::Cell<ManagementFeatures>,
    pub hall: RefCell<Hall>,
    glade: Glade,
    window: gtk::Window,
    ring: RefCell<Option<Ring>>,
    /// Clés des QR codes des arbitres (`GetSecretKey`), lues par les threads
    /// réseau.
    keys: Arc<Mutex<HashMap<u32, String>>>,
    hall_view: RefCell<Option<Rc<hall_view::HallView>>>,
    referees: RefCell<Option<Rc<referees_view::RefereesView>>>,
    batches: RefCell<Option<Rc<batches::BatchesView>>>,
    quitting: std::cell::Cell<bool>,
    /// Code de l'administrateur (scanner des feuilles).
    admin_code: RefCell<bp_net::wifi_code::WifiCode>,
}

impl Ui {
    /// `Marshaller::Marshaller` + `Start`.
    pub fn start(app: &gtk::Application) {
        let glade = match Glade::new("marshaller.ui") {
            Ok(g) => g,
            Err(e) => {
                eprintln!("Marshaller : impossible de charger l'interface : {}", e.0);
                app.quit();
                return;
            }
        };
        let window: gtk::Window = glade.get("root");
        window.set_application(Some(app));
        window.set_title(Some("ManageComp - Organisation"));
        dialogs::set_main_window(&window);
        dialogs::keep_focus_in_app();
        glade.attach_actions(&window);
        for (action, accel) in glade.accels() {
            app.set_accels_for_action(&action, &[accel.as_str()]);
        }

        let mut hall = Hall::new(None);
        if !hall.load_file(&Hall::json_path()) {
            hall.default_layout();
        }
        let ui = Rc::new(Ui {
            local_output: None,
            features: std::cell::Cell::new(ManagementFeatures::ALL),
            hall: RefCell::new(hall),
            glade,
            window: window.clone(),
            ring: RefCell::new(None),
            keys: Arc::new(Mutex::new(HashMap::new())),
            hall_view: RefCell::new(None),
            referees: RefCell::new(None),
            batches: RefCell::new(None),
            quitting: std::cell::Cell::new(false),
            admin_code: RefCell::new(bp_net::wifi_code::WifiCode::new("Administrator")),
        });

        *ui.referees.borrow_mut() = Some(referees_view::RefereesView::new(&ui, &ui.glade.get::<gtk::Notebook>("referee_notebook")));
        let hall_view = hall_view::HallView::new(&ui);
        ui.glade.get::<gtk::Viewport>("hall_viewport").set_child(Some(&hall_view.root()));
        *ui.hall_view.borrow_mut() = Some(hall_view);
        *ui.batches.borrow_mut() = Some(batches::BatchesView::new(&ui, &ui.glade.get::<gtk::Notebook>("batch_notebook")));

        ui.connect_handlers();
        ui.join_ring();
        ui.setup_smartphones();

        // Horloge, réaffichage périodique (`Clock`, `ScheduleRedraw`).
        let weak = Rc::downgrade(&ui);
        glib::timeout_add_local(Duration::from_secs(10), move || match weak.upgrade() {
            Some(ui) => {
                if ui.hall.borrow().redraw_scheduled {
                    ui.hall.borrow_mut().redraw();
                    ui.refresh();
                }
                glib::ControlFlow::Continue
            }
            None => glib::ControlFlow::Break,
        });

        ui.glade.get::<gtk::Paned>("hpaned").set_position(380);
        window.maximize();
        window.present();
        ui.refresh();
        unsafe { window.set_data("marshaller-ui", ui) };
    }

    pub fn window(&self) -> &gtk::Window {
        &self.window
    }

    /// Vue intégrée : ni nouvelle fenêtre principale, ni second anneau réseau.
    pub fn embedded(parent: &gtk::Window, key: &str, output: Rc<dyn Fn(NetCommand)>) -> Result<Rc<Self>, bp_gtk::glade::GladeError> {
        let glade = Glade::new("marshaller.ui")?;
        let root: gtk::Widget = glade.get("vbox1");
        Glade::detach_from_parent(&root);
        glade.get::<gtk::Widget>("menubar1").set_visible(false);
        glade.attach_actions(&root);
        let mut hall = Hall::new(None);
        if !hall.load_file(&Hall::json_path()) { hall.default_layout(); }
        hall.referees.cryptor_key = Some(key.to_string());
        hall.load_referee_catalog(&crate::config::config_dir().join("local-referees.xml"));
        let ui = Rc::new(Self {
            local_output: Some(output), features: std::cell::Cell::new(ManagementFeatures::default()),
            hall: RefCell::new(hall), glade, window: parent.clone(), ring: RefCell::new(None),
            keys: Arc::new(Mutex::new(HashMap::new())), hall_view: RefCell::new(None),
            referees: RefCell::new(None), batches: RefCell::new(None), quitting: std::cell::Cell::new(false),
            admin_code: RefCell::new(bp_net::wifi_code::WifiCode::new("Administrator")),
        });
        for object in ui.glade.objects() {
            if let Ok(dialog) = object.downcast::<gtk::Dialog>() { dialog.set_transient_for(Some(parent)); }
        }
        embedded_layout::organisation(&ui.glade);
        *ui.referees.borrow_mut() = Some(referees_view::RefereesView::new(&ui, &ui.glade.get("referee_notebook")));
        let hall_view = hall_view::HallView::new(&ui);
        ui.glade.get::<gtk::Viewport>("hall_viewport").set_child(Some(&hall_view.root()));
        *ui.hall_view.borrow_mut() = Some(hall_view);
        *ui.batches.borrow_mut() = Some(batches::BatchesView::new(&ui, &ui.glade.get("batch_notebook")));
        ui.connect_handlers();
        ui.setup_smartphones();
        ui.set_features(ManagementFeatures::default());
        Ok(ui)
    }

    pub fn widget(&self) -> gtk::Widget { self.glade.get("vbox1") }
    pub fn management_features(&self) -> ManagementFeatures { self.features.get() }

    pub fn edit_job_resources(self: &Rc<Self>, job: crate::slot::JobKey) {
        let (is_pool, multi) = {
            let hall = self.hall.borrow();
            (hall.jobs.get(&job).is_some_and(|j| j.is_pool), hall.jobs.get(&job).and_then(|j| j.slot).and_then(|sk| hall.slots.get(&sk)).is_some_and(|s| !s.additional_pistes.is_empty()))
        };
        if is_pool && (self.features.get().pistes || multi) { pool_resources::edit(self, job); }
        else if self.features.get().referees { job_referees::edit(self, job); }
    }

    pub fn set_features(self: &Rc<Self>, features: ManagementFeatures) {
        self.features.set(features);
        self.glade.get::<gtk::Widget>("referee_vbox").set_visible(features.referees);
        self.glade.get::<gtk::Widget>("hall_viewport").set_visible(features.pistes);
        self.glade.get::<gtk::Widget>("batch_vbox").set_visible(features.enabled());
        self.glade.get::<gtk::Widget>("collapse").set_visible(false);
        self.glade.get::<gtk::Notebook>("referee_notebook").set_show_tabs(true);
        if self.local_output.is_some() {
            if let Some(referees) = self.referees.borrow().as_ref() { referees.set_expanded(features.referees); }
        }
        self.glade.get::<gtk::Paned>("hpaned").set_position(if features.pistes { 310 } else { self.window.width() / 2 });
        self.refresh();
        if features.referees && self.local_output.is_some() {
            self.hall.borrow_mut().referees_spread(); self.flush_outbox();
        }
    }

    fn connect_handlers(self: &Rc<Self>) {
        macro_rules! on {
            ($name:expr, $body:expr) => {{
                let w = Rc::downgrade(self);
                let f = $body;
                self.glade.connect_simple($name, move |_| {
                    if let Some(ui) = w.upgrade() {
                        f(&ui);
                    }
                });
            }};
        }
        on!("on_root_delete_event", |ui: &Rc<Ui>| ui.quit());
        {
            let w = Rc::downgrade(self);
            if self.local_output.is_none() { self.window.connect_close_request(move |_| {
                if let Some(ui) = w.upgrade() {
                    ui.quit();
                }
                glib::Propagation::Stop
            }); }
        }
        on!("on_collapse_clicked", |ui: &Rc<Ui>| ui.collapse_referees());
        on!("on_network_config_menuitem_activate", |ui: &Rc<Ui>| ui.menu_dialog("network_dialog"));
        on!("on_scanner_menuitem_activate", |ui: &Rc<Ui>| ui.menu_dialog("scanner_dialog"));
        on!("on_tickets_menuitem_activate", |ui: &Rc<Ui>| ui.print_referees(true));
        on!("on_payment_menuitem_activate", |ui: &Rc<Ui>| ui.print_referees(false));
        on!("on_access_code_activate", |ui: &Rc<Ui>| ui.show_access_code());
        on!("on_SmartPoule_button_clicked", |_: &Rc<Ui>| dialogs::show_uri(
            "https://arbitre.managecomp.org"
        ));
        {
            let w = Rc::downgrade(self);
            self.glade.get::<gtk::Notebook>("batch_notebook").connect_switch_page(move |_, page, _| {
                if let Some(ui) = w.upgrade() {
                    let competition = unsafe { page.data::<u32>("competition").map(|p| *p.as_ref()) };
                    if let Some(c) = competition {
                        let weapon = ui.hall.borrow().competition(c).map(|x| x.weapon.clone());
                        if let Some(w) = weapon {
                            ui.expose_weapon(&w);
                        }
                    }
                }
            });
        }
    }

    // --------------------------------------------------------------------------------
    // Réseau

    /// `Net::Ring::Join` (rôle gestionnaire de ressources).
    fn join_ring(self: &Rc<Self>) {
        let keys = self.keys.clone();
        let provider = move |scheme: &str| -> Option<String> {
            let tokens: Vec<&str> = scheme.split('/').collect();
            let reference = tokens.get(2)?.trim().parse::<u32>().ok()?;
            keys.lock().ok()?.get(&reference).cloned()
        };
        let (ring, events) = Ring::join(RingConfig::new(Role::ResourceManager), Arc::new(provider));
        self.hall.borrow_mut().referees.cryptor_key = Some(ring.cryptor_key());
        *self.ring.borrow_mut() = Some(ring);
        self.poll_ring(events);
    }

    fn poll_ring(self: &Rc<Self>, events: Receiver<RingEvent>) {
        let weak = Rc::downgrade(self);
        glib::timeout_add_local(Duration::from_millis(200), move || {
            let Some(ui) = weak.upgrade() else { return glib::ControlFlow::Break };
            let mut changed = false;
            while let Ok(event) = events.try_recv() {
                match event {
                    RingEvent::Message(m) => {
                        changed |= ui.hall.borrow_mut().on_message(&m);
                    }
                    RingEvent::HandshakeResult(r) => ui.on_handshake_result(r),
                    RingEvent::Left => {
                        if let Some(app) = ui.window.application() {
                            app.quit();
                        }
                    }
                    _ => {}
                }
            }
            if changed {
                ui.after_change();
            }
            glib::ControlFlow::Continue
        });
    }

    /// `Marshaller::OnHanshakeResult`.
    fn on_handshake_result(&self, result: HandshakeResult) {
        match result {
            HandshakeResult::AuthenticationFailed => {
                dialogs::info(Some(&self.window), &tr("<b><big>Authentication failed!</big></b>"), None);
            }
            HandshakeResult::RoleRejected => {
                dialogs::info(
                    Some(&self.window),
                    &tr("<b><big>Another organisation is already running on the network!</big></b>"),
                    None,
                );
            }
            _ => {}
        }
    }

    /// Exécute les ordres réseau produits par le modèle.
    fn flush_outbox(&self) {
        let commands = self.hall.borrow_mut().take_outbox();
        let ring = self.ring.borrow().clone();
        for c in commands {
            if let Some(output) = &self.local_output { output(c); continue; }
            match c {
                NetCommand::Spread(mut m) => {
                    if let Some(r) = &ring {
                        r.spread_message(&mut m);
                    }
                }
                NetCommand::Recall(mut m) => {
                    if let Some(r) = &ring {
                        r.recall_message(&mut m);
                    }
                }
                NetCommand::ChangePassphrase(p) => {
                    if let Some(r) = &ring {
                        r.change_passphrase(&p);
                    }
                }
                NetCommand::AnnounceAvailability => {
                    if let Some(r) = &ring {
                        r.announce_availability();
                    }
                }
                NetCommand::RegisterSecretKey { referee, key } => {
                    if let Ok(mut k) = self.keys.lock() {
                        k.insert(referee, key);
                    }
                }
            }
        }
    }

    // --------------------------------------------------------------------------------
    // Mise à jour

    /// Après une action : ordres réseau, notifications, réaffichage.
    pub fn after_change(self: &Rc<Self>) {
        if self.local_output.is_some() {
            if let Err(e) = self.hall.borrow().save_referee_catalog(&crate::config::config_dir().join("local-referees.xml")) { log::warn!("referee catalog: {e}"); }
        }
        self.flush_outbox();
        let events = self.hall.borrow_mut().take_events();
        for e in events {
            match e {
                UiEvent::ExposeWeapon(w) => self.expose_weapon(&w),
                UiEvent::Blur => self.set_blurred(true),
                UiEvent::UnBlur => self.set_blurred(false),
                UiEvent::PartnerCredentials => {}
                _ => {}
            }
        }
        if self.hall.borrow().dirty {
            let _ = self.hall.borrow_mut().save_file(&Hall::json_path());
        }
        self.refresh();
    }

    /// Réaffiche toutes les vues.
    pub fn refresh(self: &Rc<Self>) {
        if let Some(v) = self.hall_view.borrow().clone() {
            v.refresh();
        }
        if let Some(v) = self.referees.borrow().clone() {
            v.refresh();
        }
        if let Some(v) = self.batches.borrow().clone() {
            v.refresh();
        }
    }

    /// Arbitres et lots grisés pendant le déplacement d'un match (`OnBlur`).
    fn set_blurred(&self, blurred: bool) {
        self.glade.get::<gtk::Widget>("referee_vbox").set_sensitive(!blurred);
        self.glade.get::<gtk::Widget>("batch_vbox").set_sensitive(!blurred);
    }

    /// Filtre local des armes, sans modifier le catalogue ni les indices des onglets.
    pub fn set_visible_weapons(&self, weapons: &[&str]) {
        if self.local_output.is_some() {
            if let Some(view) = self.referees.borrow().clone() {
                view.set_visible_weapons(weapons);
            }
        }
    }

    /// `OnExposeWeapon` : onglet des arbitres de l'arme.
    pub fn expose_weapon(&self, weapon: &str) {
        if let Some(i) = crate::weapon::index_of(weapon) {
            self.glade.get::<gtk::Notebook>("referee_notebook").set_current_page(Some(i as u32));
        }
    }

    /// Arbitres sélectionnés dans la liste de l'arme.
    pub fn selected_referees(&self, weapon: &str) -> Vec<u32> {
        self.referees.borrow().as_ref().map(|v| v.selected(weapon)).unwrap_or_default()
    }

    // --------------------------------------------------------------------------------
    // Appel des arbitres (vue étendue)

    /// `OnOpenCheckin` : la liste occupe toute la fenêtre.
    pub fn open_checkin(&self, page: u32) {
        if self.local_output.is_some() {
            self.glade.get::<gtk::Notebook>("referee_notebook").set_current_page(Some(page));
            return;
        }
        let paned = self.glade.get::<gtk::Paned>("hpaned");
        paned.set_position(self.window.width());
        let notebook = self.glade.get::<gtk::Notebook>("referee_notebook");
        notebook.set_current_page(Some(page));
        notebook.set_show_tabs(false);
        self.glade.get::<gtk::Widget>("hall_viewport").set_visible(false);
        self.glade.get::<gtk::Widget>("batch_vbox").set_visible(false);
        self.glade.get::<gtk::Widget>("collapse").set_visible(true);
    }

    /// `OnRefereeListCollapse`.
    fn collapse_referees(self: &Rc<Self>) {
        self.glade.get::<gtk::Paned>("hpaned").set_position(380);
        if let Some(v) = self.referees.borrow().clone() {
            v.collapse_all();
        }
        let notebook = self.glade.get::<gtk::Notebook>("referee_notebook");
        notebook.set_show_tabs(true);
        self.glade.get::<gtk::Widget>("hall_viewport").set_visible(true);
        self.glade.get::<gtk::Widget>("batch_vbox").set_visible(true);
        self.glade.get::<gtk::Widget>("collapse").set_visible(false);
        self.refresh();
    }

    // --------------------------------------------------------------------------------
    // Smartphones (`SmartPhones`)

    /// Réseau WIFI des smartphones (dialogue « WIFI ») et code de
    /// l'administrateur (dialogue « Scanner »).
    fn setup_smartphones(self: &Rc<Self>) {
        let config = crate::config::UserConfig::load(&crate::config::config_dir().join("config.ini"));
        let ssid: gtk::Entry = self.glade.get("SSID_entry");
        let passphrase: gtk::Entry = self.glade.get("passphrase_entry");
        passphrase.set_visibility(false);
        ssid.set_text(config.get("WIFI", "ssid").unwrap_or(""));
        passphrase.set_text(config.get("WIFI", "passphrase").unwrap_or(""));
        let w = Rc::downgrade(self);
        let refresh = move || {
            if let Some(ui) = w.upgrade() {
                ui.refresh_scanner_code();
            }
        };
        let r = refresh.clone();
        ssid.connect_changed(move |_| r());
        let r = refresh.clone();
        passphrase.connect_changed(move |_| r());
        let r = refresh.clone();
        self.glade.get::<gtk::ComboBox>("security_combobox").connect_changed(move |_| r());
        refresh();
    }

    /// `SmartPhones::RefreshScannerCode`.
    fn refresh_scanner_code(&self) {
        use bp_net::flash_code::FlashCode;
        let ssid = self.glade.get::<gtk::Entry>("SSID_entry").text().to_string();
        let passphrase = self.glade.get::<gtk::Entry>("passphrase_entry").text().to_string();
        let mut network = bp_net::wifi_network::WifiNetwork::new();
        network.set_ssid(Some(&ssid));
        network.set_passphrase(Some(&passphrase));
        network.set_encryption(Some("WPA"));
        bp_net::wifi_code::WifiCode::set_wifi_network(Some(network));
        let mut config = crate::config::UserConfig::load(&crate::config::config_dir().join("config.ini"));
        config.set("WIFI", "ssid", &ssid);
        config.set("WIFI", "passphrase", &passphrase);
        let _ = config.save();
        let code = self.admin_code.borrow();
        if let Some(texture) = draw::qr_texture(&code.text()) {
            self.glade.get::<gtk::Image>("scanner_code_image").set_paintable(Some(&texture));
        }
    }

    /// QR code d'un arbitre (`Referee::GetFlashCode`) : réseau, adresse,
    /// nom, référence et clé de chiffrement de son smartphone.
    pub fn referee_code(self: &Rc<Self>, reference: u32) -> Option<gtk::gdk::Texture> {
        use bp_net::flash_code::FlashCode;
        let key = self.hall.borrow_mut().referee_wifi_key(reference)?;
        let name = self.hall.borrow().referees.get(reference).map(|r| r.player.name())?;
        self.flush_outbox();
        let code = bp_net::wifi_code::WifiCode::for_player(&name, reference);
        code.set_key(Some(&key));
        draw::qr_texture(&code.text())
    }

    // --------------------------------------------------------------------------------
    // Menus

    fn menu_dialog(&self, id: &str) {
        let dialog: gtk::Dialog = self.glade.get(id);
        dialog.set_transient_for(Some(&self.window));
        dialogs::run_dialog(&dialog);
        dialog.set_visible(false);
    }

    /// `OnShowAccessCode` : QR code de l'application partenaire.
    fn show_access_code(&self) {
        self.glade.get::<gtk::Widget>("step1").set_visible(false);
        self.glade.get::<gtk::Widget>("step2").set_visible(false);
        if let Some(ring) = self.ring.borrow().as_ref() {
            use bp_net::flash_code::FlashCode;
            let code = ring.flash_code();
            if let Some(texture) = draw::qr_texture(&code.text()) {
                self.glade.get::<gtk::Image>("qrcode_image").set_paintable(Some(&texture));
            }
        }
        self.menu_dialog("pin_code_dialog");
    }

    /// `PrintMealTickets` / `PrintPaymentBook`.
    fn print_referees(&self, meal_tickets: bool) {
        let referees: Vec<(String, String, String)> = {
            let hall = self.hall.borrow();
            let mut out = Vec::new();
            for list in &hall.referees.lists {
                for r in &list.members {
                    if let Some(x) = hall.referees.get(*r) {
                        out.push((
                            x.player.name(),
                            x.player.first_name(),
                            x.player.attr("level").map(|v| v.xml_image()).unwrap_or_else(|| "-".into()),
                        ));
                    }
                }
            }
            out
        };
        let title = if meal_tickets { tr("Meal tickets") } else { tr("Payment book") };
        let job = title.clone();
        let result = bp_gtk::print::run_document(Some(&self.window), &job, move || {
            Box::new(draw::RefereesPrint { referees: referees.clone(), meal_tickets, title: title.clone() })
        });
        if let Err(e) = result {
            dialogs::error(Some(&self.window), &e.to_string());
        }
    }

    /// Quitte : enregistre la salle et quitte l'anneau.
    fn quit(&self) {
        if self.quitting.replace(true) {
            return;
        }
        let _ = self.hall.borrow_mut().save_file(&Hall::json_path());
        match self.ring.borrow().as_ref() {
            Some(ring) => {
                ring.leave();
                // Sans réponse des partenaires, on quitte quand même.
                let app = self.window.application();
                glib::timeout_add_local_once(Duration::from_secs(2), move || {
                    if let Some(app) = app {
                        app.quit();
                    }
                });
            }
            None => {
                if let Some(app) = self.window.application() {
                    app.quit();
                }
            }
        }
    }
}

use gtk::glib;
