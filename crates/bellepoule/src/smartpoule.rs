//! SmartPoule : liaison avec les téléphones des arbitres par le serveur
//! d'API de BellePoule Web (relais sur Internet : BellePoule et les
//! téléphones n'ont pas besoin d'être sur le même réseau).
//!
//! Toutes les 3 secondes, si le serveur et la clé d'API du club sont
//! réglés : BellePoule dépose un instantané (arbitres et leurs clés, matchs à
//! arbitrer, feuilles) puis relève les scores saisis sur les téléphones, les
//! applique et rend compte du résultat.
//!
//! Réglages (`Tournament/…` de la configuration) : `SmartPoule/server`,
//! `SmartPoule/api_key`, `SmartPoule/club` (identifiant renvoyé par le
//! serveur), `SmartPoule/admin_key` (QR code de l'administrateur).

use std::cell::RefCell;
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;

use bp_core::i18n::tr;
use bp_gtk::dialogs;
use gtk::prelude::*;
use gtk4 as gtk;
use gtk4::glib;
use serde_json::{json, Value};

use crate::app::App;
use crate::contest_page::ContestPage;

/// Réglages de la liaison.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Settings {
    pub server: String,
    pub api_key: String,
    pub club: String,
    pub admin_key: String,
}

impl Settings {
    pub fn load(app: &App) -> Settings {
        let t = app.tournament.borrow();
        let get = |k: &str| t.config.get_string("SmartPoule", k).unwrap_or_default();
        Settings { server: get("server"), api_key: get("api_key"), club: get("club"), admin_key: get("admin_key") }
    }

    fn save(&self, app: &App) {
        let mut t = app.tournament.borrow_mut();
        t.config.set_string("SmartPoule", "server", &self.server);
        t.config.set_string("SmartPoule", "api_key", &self.api_key);
        t.config.set_string("SmartPoule", "club", &self.club);
        t.config.set_string("SmartPoule", "admin_key", &self.admin_key);
        let _ = t.save_config();
    }

    fn base(&self) -> String {
        self.server.trim().trim_end_matches('/').to_string()
    }

    pub fn configured(&self) -> bool {
        !self.server.trim().is_empty() && !self.api_key.trim().is_empty()
    }

    /// Texte du QR code d'un arbitre (`reference = 0` : administrateur).
    /// Adresse du QR code d'un arbitre : la page ManageComp Arbitre
    /// (`https://arbitre.<domaine>/` quand le serveur est `https://api.<domaine>`).
    pub fn qr_text(&self, reference: u32, key: &str) -> String {
        let base = self.base();
        match base.split_once("://") {
            Some((scheme, host)) if host.starts_with("api.") && !host.contains('/') => {
                format!("{scheme}://arbitre.{}/?c={}&r={reference}&k={key}", &host[4..], self.club)
            }
            _ => format!("{base}/smartpoule/?c={}&r={reference}&k={key}", self.club),
        }
    }
}

/// Résultats renvoyés par le fil réseau.
enum Incoming {
    Club(String),
    Pending(Vec<Value>),
    Status(String),
}

pub struct SmartPoule {
    app: Weak<App>,
    last_state: RefCell<String>,
    busy: Arc<AtomicBool>,
    tx: Sender<Incoming>,
    rx: Receiver<Incoming>,
    pub status: RefCell<String>,
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new().timeout(Duration::from_secs(10)).build()
}

impl SmartPoule {
    pub fn start(app: &Rc<App>) -> Rc<SmartPoule> {
        let (tx, rx) = channel();
        let sp = Rc::new(SmartPoule {
            app: Rc::downgrade(app),
            last_state: RefCell::new(String::new()),
            busy: Arc::new(AtomicBool::new(false)),
            tx,
            rx,
            status: RefCell::new(tr("Not configured")),
        });
        let weak = Rc::downgrade(&sp);
        glib::timeout_add_local(Duration::from_secs(3), move || match weak.upgrade() {
            Some(sp) => {
                sp.tick();
                glib::ControlFlow::Continue
            }
            None => glib::ControlFlow::Break,
        });
        sp
    }

    /// Un cycle : résultats reçus, puis nouvel échange si le précédent est fini.
    fn tick(&self) {
        let Some(app) = self.app.upgrade() else { return };
        while let Ok(incoming) = self.rx.try_recv() {
            match incoming {
                Incoming::Club(club) => {
                    let mut s = Settings::load(&app);
                    s.club = club;
                    s.save(&app);
                }
                Incoming::Pending(list) => self.apply(&app, list),
                Incoming::Status(text) => *self.status.borrow_mut() = text,
            }
        }
        let mut settings = Settings::load(&app);
        if !settings.configured() {
            *self.status.borrow_mut() = tr("Not configured");
            return;
        }
        if settings.admin_key.is_empty() {
            settings.admin_key = bp_net::flash_code::get_key256();
            settings.save(&app);
        }
        if self.busy.load(Ordering::SeqCst) || app.tournament.try_borrow_mut().is_err() {
            return;
        }
        let state = self.snapshot(&app, &settings).to_string();
        let send_state = *self.last_state.borrow() != state;
        if send_state {
            *self.last_state.borrow_mut() = state.clone();
        }
        self.busy.store(true, Ordering::SeqCst);
        let (busy, tx) = (self.busy.clone(), self.tx.clone());
        let need_club = settings.club.is_empty();
        std::thread::spawn(move || {
            let base = settings.base();
            let key = settings.api_key.trim().to_string();
            let agent = agent();
            let result = (|| -> Result<(), String> {
                if need_club {
                    let me: Value = agent.get(&format!("{base}/api/me")).set("X-Api-Key", &key).call().map_err(describe)?.into_json().map_err(|e| e.to_string())?;
                    if let Some(id) = me.get("id").and_then(Value::as_str) {
                        let _ = tx.send(Incoming::Club(id.to_string()));
                    }
                }
                if send_state {
                    agent
                        .put(&format!("{base}/api/smartpoule/state"))
                        .set("X-Api-Key", &key)
                        .set("Content-Type", "application/json")
                        .send_string(&state)
                        .map_err(describe)?;
                }
                let pending: Value = agent.get(&format!("{base}/api/smartpoule/pending")).set("X-Api-Key", &key).call().map_err(describe)?.into_json().map_err(|e| e.to_string())?;
                if let Some(list) = pending.as_array().filter(|l| !l.is_empty()) {
                    let _ = tx.send(Incoming::Pending(list.clone()));
                }
                Ok(())
            })();
            let _ = tx.send(Incoming::Status(match result {
                Ok(()) => tr("Connected"),
                Err(e) => format!("{} : {e}", tr("Connection error")),
            }));
            busy.store(false, Ordering::SeqCst);
        });
        // Un échec d'envoi de l'instantané le fera renvoyer.
        if !send_state {
            return;
        }
    }

    /// Instantané : arbitres et leurs clés, matchs, feuilles.
    fn snapshot(&self, app: &Rc<App>, settings: &Settings) -> Value {
        let pages: Vec<Rc<ContestPage>> = app.pages.borrow().clone();
        let mut referees: Vec<Value> = Vec::new();
        let mut jobs: Vec<Value> = Vec::new();
        let mut sheets = serde_json::Map::new();
        for page in &pages {
            let Some(()) = page.try_with_contest(|c| {
                ensure_referee_keys(c);
                for (r, key) in c.referee_passwords() {
                    if !referees.iter().any(|x| x["reference"] == json!(r)) {
                        let name = c.referees.get(r).map(|p| format!("{} {}", p.name(), p.first_name())).unwrap_or_default();
                        referees.push(json!({ "reference": r, "key": key, "name": name }));
                    }
                }
                for job in c.smartpoule_jobs(None) {
                    if let Some(token) = job.get("token").and_then(Value::as_str) {
                        if let Some(sheet) = c.smartpoule_sheet(token, None) {
                            sheets.insert(token.to_string(), sheet);
                        }
                    }
                    jobs.push(job);
                }
            }) else {
                continue;
            };
        }
        json!({ "admin_key": settings.admin_key, "referees": referees, "jobs": jobs, "sheets": sheets })
    }

    /// Applique les scores reçus puis rend compte au serveur.
    fn apply(&self, app: &Rc<App>, list: Vec<Value>) {
        let pages: Vec<Rc<ContestPage>> = app.pages.borrow().clone();
        let mut results = Vec::new();
        for s in list {
            let id = s.get("id").and_then(Value::as_u64).unwrap_or(0);
            let token = s.get("token").and_then(Value::as_str).unwrap_or("").to_string();
            let bout = s.get("bout").and_then(Value::as_u64).unwrap_or(0) as usize;
            let values: Vec<String> = s.get("values").and_then(Value::as_array).map(|v| v.iter().map(|x| x.as_str().unwrap_or("").to_string()).collect()).unwrap_or_default();
            let contest: u32 = token.split('~').nth(1).and_then(|c| c.parse().ok()).unwrap_or(0);
            let page = pages.iter().find(|p| p.with_contest(|c| c.net_id == contest).unwrap_or(false));
            let officiating = s.get("officiating").filter(|v| !v.is_null()).cloned();
            let relay = s.get("relay").filter(|v| !v.is_null()).cloned();
            let outcome = match page {
                Some(page) => {
                    let r = page.with_contest(|c| -> Result<(), String> {
                        if let Some(state) = officiating {
                            let state = serde_json::from_value(state).map_err(|_| "journal d'arbitrage invalide")?;
                            c.smartpoule_apply_officiating(&token, bout, state)?;
                        }
                        if let Some(log) = relay.clone() {
                            let log = serde_json::from_value(log).map_err(|_| "journal des relais invalide")?;
                            c.smartpoule_apply_relay(&token, bout, log)?;
                        }
                        if values.len() == 2 { c.smartpoule_apply_score(&token, bout, [values[0].clone(), values[1].clone()])?; }
                        else if s.get("officiating").is_none_or(|v| v.is_null()) && relay.is_none() { return Err("saisie incomplète".into()); }
                        Ok(())
                    }).unwrap_or(Err("compétition fermée".into()));
                    if r.is_ok() {
                        page.mark_dirty();
                        if let Some(view) = page.schedule.borrow().clone() {
                            view.refresh_screens();
                        }
                    }
                    r
                }
                None => Err("compétition non ouverte dans ManageComp".into()),
            };
            results.push(match outcome {
                Ok(()) => json!({ "id": id, "ok": true }),
                Err(e) => json!({ "id": id, "ok": false, "error": e }),
            });
        }
        let settings = Settings::load(app);
        let body = json!({ "results": results }).to_string();
        // Les feuilles changent : nouvel instantané au prochain cycle.
        self.last_state.borrow_mut().clear();
        std::thread::spawn(move || {
            let _ = agent()
                .post(&format!("{}/api/smartpoule/results", settings.base()))
                .set("X-Api-Key", settings.api_key.trim())
                .set("Content-Type", "application/json")
                .send_string(&body);
        });
    }
}

fn describe(e: ureq::Error) -> String {
    match e {
        ureq::Error::Status(401, _) => tr("API key refused"),
        ureq::Error::Status(code, r) => format!("HTTP {code} {}", r.into_string().unwrap_or_default()),
        ureq::Error::Transport(t) => t.to_string(),
    }
}

/// Chaque arbitre reçoit une clé personnelle (son QR code) s'il n'en a pas.
fn ensure_referee_keys(c: &mut bp_core::contest::Contest) {
    let refs: Vec<u32> = c.referees.refs().to_vec();
    for r in refs {
        if let Some(p) = c.referees.get_mut(r) {
            if p.str_attr("password").map(|k| k.len() < 16).unwrap_or(true) {
                p.set_str(&bp_core::player::AttrId::global("password"), &bp_net::flash_code::get_key256());
                c.dirty = true;
            }
        }
    }
}

// ------------------------------------------------------------------ dialogue

/// Adresse des écrans de piste : sous-domaine `live.` quand le serveur est
/// `https://api.<domaine>` (https://live.managecomp.org/?c=…), sinon `/live/`.
fn live_link(server: &str, club: &str) -> String {
    let server = server.trim().trim_end_matches('/');
    match server.split_once("://") {
        Some((scheme, host)) if host.starts_with("api.") && !host.contains('/') => format!("{scheme}://live.{}/?c={club}", &host[4..]),
        _ => format!("{server}/live/?c={club}"),
    }
}

fn qr_texture(text: &str) -> Option<gtk::gdk::Texture> {
    let png = bp_net::flash_code::QrMatrix::encode(text)?.to_image(4).to_png();
    gtk::gdk::Texture::from_bytes(&glib::Bytes::from_owned(png)).ok()
}

/// Menu BellePoule2D > SmartPoule… : réglages du serveur, QR codes de
/// l'administrateur et des arbitres, impression des QR codes.
pub fn show_dialog(app: &Rc<App>) {
    let settings = Settings::load(app);
    let dialog = gtk::Dialog::with_buttons(
        Some("Arbitres sur téléphone"),
        Some(&app.window),
        gtk::DialogFlags::MODAL,
        &[(&tr("_Close"), gtk::ResponseType::Close)],
    );
    dialog.set_default_size(720, 640);
    let content = gtk::Box::new(gtk::Orientation::Vertical, 10);
    content.set_margin_start(14);
    content.set_margin_end(14);
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    let intro = gtk::Label::new(Some(&tr("The referees' phones (ManageComp Arbitre app) exchange through your ManageComp server: no need to be on the same network.")));
    intro.set_wrap(true);
    intro.set_xalign(0.0);
    content.append(&intro);

    let grid = gtk::Grid::new();
    grid.set_row_spacing(6);
    grid.set_column_spacing(10);
    let server = gtk::Entry::new();
    server.set_placeholder_text(Some("https://bellepoule.monclub.fr"));
    server.set_text(&settings.server);
    server.set_hexpand(true);
    let key = gtk::PasswordEntry::new();
    key.set_show_peek_icon(true);
    key.set_text(&settings.api_key);
    for (row, (label, widget)) in [(tr("Server address"), server.upcast_ref::<gtk::Widget>()), (tr("Club API key"), key.upcast_ref())].into_iter().enumerate() {
        let l = gtk::Label::new(Some(&label));
        l.set_xalign(1.0);
        grid.attach(&l, 0, row as i32, 1, 1);
        grid.attach(widget, 1, row as i32, 1, 1);
    }
    let save = gtk::Button::with_label(&tr("Save and connect"));
    grid.attach(&save, 1, 2, 1, 1);
    content.append(&grid);
    let status = gtk::Label::new(None);
    status.set_xalign(0.0);
    let current = app.smartpoule.borrow().as_ref().map(|s| s.status.borrow().clone()).unwrap_or_default();
    status.set_text(&format!("{} : {current}", tr("Status")));
    content.append(&status);

    // Écrans de piste : LivePoule relit les scores en direct sur le serveur.
    if !settings.server.trim().is_empty() && !settings.club.is_empty() {
        let link = live_link(&settings.server, &settings.club);
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let label = gtk::Label::new(Some(&tr("Piste screens (ManageComp Live)")));
        row.append(&label);
        let entry = gtk::Entry::new();
        entry.set_text(&link);
        entry.set_editable(false);
        entry.set_hexpand(true);
        row.append(&entry);
        let copy = gtk::Button::with_label(&tr("Copy"));
        {
            let link = link.clone();
            copy.connect_clicked(move |b| b.clipboard().set_text(&link));
        }
        row.append(&copy);
        content.append(&row);
        let hint = gtk::Label::new(Some(&tr("Start each screen with: managecomp-live --live <this address> --piste <number>. The same address opened in a browser shows the live score of a piste.")));
        hint.set_wrap(true);
        hint.set_xalign(0.0);
        hint.add_css_class("dim-label");
        content.append(&hint);
    }

    let codes = gtk::FlowBox::new();
    codes.set_selection_mode(gtk::SelectionMode::None);
    codes.set_max_children_per_line(4);
    codes.set_row_spacing(10);
    codes.set_column_spacing(10);
    let scroller = gtk::ScrolledWindow::new();
    scroller.set_vexpand(true);
    scroller.set_child(Some(&codes));
    let print = gtk::Button::with_label(&tr("Print the QR codes"));
    print.set_halign(gtk::Align::Start);
    content.append(&print);
    content.append(&scroller);

    // Cartes : administrateur, puis chaque arbitre des compétitions ouvertes.
    let cards = {
        let app = app.clone();
        move || -> Vec<(String, String, String)> {
            let s = Settings::load(&app);
            if !s.configured() || s.club.is_empty() {
                return Vec::new();
            }
            let mut out = vec![(tr("Administrator"), tr("all the bouts"), s.qr_text(0, &s.admin_key))];
            let pages: Vec<Rc<ContestPage>> = app.pages.borrow().clone();
            for page in pages {
                let list = page
                    .with_contest(|c| {
                        ensure_referee_keys(c);
                        c.referee_passwords()
                            .into_iter()
                            .map(|(r, k)| {
                                let p = c.referees.get(r);
                                let name = p.map(|p| format!("{} {}", p.name(), p.first_name())).unwrap_or_default();
                                let club = p.and_then(|p| p.str_attr("club").map(str::to_string)).unwrap_or_default();
                                (name, club, s.qr_text(r, &k))
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                for item in list {
                    if !out.iter().any(|x| x.2 == item.2) {
                        out.push(item);
                    }
                }
            }
            out
        }
    };
    let fill = {
        let codes = codes.clone();
        let cards = cards.clone();
        move || {
            while let Some(child) = codes.first_child() {
                codes.remove(&child);
            }
            let list = cards();
            if list.is_empty() {
                let l = gtk::Label::new(Some(&tr("Enter the server address and the club API key, then save: the QR codes appear once connected.")));
                l.set_wrap(true);
                codes.insert(&l, -1);
            }
            for (name, detail, text) in list {
                let card = gtk::Box::new(gtk::Orientation::Vertical, 4);
                card.add_css_class("bp-recent-card");
                if let Some(t) = qr_texture(&text) {
                    let image = gtk::Image::from_paintable(Some(&t));
                    image.set_pixel_size(150);
                    card.append(&image);
                }
                let n = gtk::Label::new(None);
                n.set_markup(&format!("<b>{}</b>", glib::markup_escape_text(&name)));
                card.append(&n);
                let d = gtk::Label::new(Some(&detail));
                d.add_css_class("bp-muted");
                card.append(&d);
                codes.insert(&card, -1);
            }
        }
    };
    fill();
    {
        let app = app.clone();
        let fill = fill.clone();
        let status = status.clone();
        save.connect_clicked(move |_| {
            let mut s = Settings::load(&app);
            let new_server = server.text().trim().to_string();
            let new_key = key.text().trim().to_string();
            if new_server != s.server || new_key != s.api_key {
                s.club.clear();
            }
            s.server = new_server;
            s.api_key = new_key;
            s.save(&app);
            if let Some(sp) = app.smartpoule.borrow().as_ref() {
                sp.last_state.borrow_mut().clear();
            }
            status.set_text(&format!("{} : {}", tr("Status"), tr("connecting…")));
            // Laisse le temps de récupérer l'identifiant du club.
            let (app2, fill, status) = (app.clone(), fill.clone(), status.clone());
            glib::timeout_add_local_once(Duration::from_secs(4), move || {
                let current = app2.smartpoule.borrow().as_ref().map(|s| s.status.borrow().clone()).unwrap_or_default();
                status.set_text(&format!("{} : {current}", tr("Status")));
                fill();
            });
        });
    }
    {
        let app = app.clone();
        print.connect_clicked(move |_| {
            let list = cards();
            if list.is_empty() {
                return;
            }
            let result = bp_gtk::print::run_document(Some(&app.window), "QR codes des arbitres", move || Box::new(QrPrint { cards: list.clone() }));
            if let Err(e) = result {
                dialogs::error(Some(&app.window), &e.to_string());
            }
        });
    }
    dialog.content_area().append(&content);
    dialogs::run_dialog(&dialog);
    dialog.destroy();
}

/// Impression des QR codes (12 par page).
struct QrPrint {
    cards: Vec<(String, String, String)>,
}

impl bp_gtk::print::PageSource for QrPrint {
    fn begin(&mut self, _width: f64, _height: f64) -> usize {
        self.cards.len().div_ceil(12).max(1)
    }

    fn page_name(&self, _page: usize) -> String {
        "Arbitres sur téléphone".into()
    }

    fn draw(&mut self, page: usize, cr: &cairo::Context, width: f64, height: f64) {
        let (cols, rows) = (3usize, 4usize);
        let (cw, ch) = (width / cols as f64, height / rows as f64);
        for (i, (name, detail, text)) in self.cards.iter().skip(page * 12).take(12).enumerate() {
            let (x, y) = ((i % cols) as f64 * cw, (i / cols) as f64 * ch);
            cr.set_source_rgb(0.6, 0.6, 0.6);
            cr.set_line_width(0.5);
            cr.rectangle(x + 4.0, y + 4.0, cw - 8.0, ch - 8.0);
            let _ = cr.stroke();
            if let Some(m) = bp_net::flash_code::QrMatrix::encode(text) {
                let size = (cw.min(ch) - 50.0).max(20.0);
                let module = size / m.width as f64;
                let (ox, oy) = (x + (cw - size) / 2.0, y + 10.0);
                cr.set_source_rgb(0.0, 0.0, 0.0);
                for yy in 0..m.width {
                    for xx in 0..m.width {
                        if m.is_dark(xx, yy) {
                            cr.rectangle(ox + xx as f64 * module, oy + yy as f64 * module, module + 0.05, module + 0.05);
                        }
                    }
                }
                let _ = cr.fill();
            }
            let layout = pangocairo::functions::create_layout(cr);
            layout.set_font_description(Some(&pango::FontDescription::from_string("Sans Bold 10")));
            layout.set_text(name);
            layout.set_width(((cw - 16.0) * pango::SCALE as f64) as i32);
            layout.set_alignment(pango::Alignment::Center);
            cr.move_to(x + 8.0, y + ch - 36.0);
            pangocairo::functions::show_layout(cr, &layout);
            layout.set_font_description(Some(&pango::FontDescription::from_string("Sans 8")));
            layout.set_text(detail);
            cr.move_to(x + 8.0, y + ch - 20.0);
            pangocairo::functions::show_layout(cr, &layout);
        }
    }
}

use gtk4::pango;
