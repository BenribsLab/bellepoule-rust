//! Liaison réseau avec le Marshaller (`Net::Ring` côté superviseur).
//!
//! BellePoule rejoint l'anneau comme utilisateur de ressources. Chaque
//! seconde, les colis décrits par les compétitions ouvertes
//! (`Contest::parcels`) sont comparés à ceux déjà diffusés : les nouveaux ou
//! modifiés sont diffusés, ceux qui ont disparu sont rappelés. Les messages
//! reçus (`BellePoule2D::Roadmap`, `Referee`, `EndOfBurst`) sont appliqués
//! aux compétitions (`Tournament::OnMessage`).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bp_net::message::Message;
use bp_net::ring::{Ring, RingConfig, RingEvent, Role};
use gtk4::glib;

use crate::app::App;
use crate::contest_page::ContestPage;

pub struct Network {
    ring: Ring,
    events: Receiver<RingEvent>,
    /// Colis diffusés : (nom, identifiant) → message.
    spread: RefCell<HashMap<(String, u32), Message>>,
    /// Messages reçus pas encore traités (tournoi occupé).
    pending: RefCell<std::collections::VecDeque<Message>>,
    /// Clés des smartphones des arbitres (`Tournament::GetSecretKey`), lues
    /// par les threads réseau.
    keys: Arc<Mutex<HashMap<u32, String>>>,
}

impl Network {
    /// Rejoint l'anneau et lance la relève des événements et la diffusion.
    pub fn start(app: &Rc<App>) -> Rc<Network> {
        let keys: Arc<Mutex<HashMap<u32, String>>> = Arc::new(Mutex::new(HashMap::new()));
        let provider = {
            let keys = keys.clone();
            move |scheme: &str| -> Option<String> {
                let tokens: Vec<&str> = scheme.split('/').collect();
                let reference = tokens.get(2)?.trim().parse::<u32>().ok()?;
                keys.lock().ok()?.get(&reference).cloned()
            }
        };
        let (ring, events) = Ring::join(RingConfig::new(Role::ResourceUser), Arc::new(provider));
        let network = Rc::new(Network {
            ring,
            events,
            spread: RefCell::new(HashMap::new()),
            pending: RefCell::new(Default::default()),
            keys,
        });

        let weak_app = Rc::downgrade(app);
        let weak_net = Rc::downgrade(&network);
        glib::timeout_add_local(Duration::from_millis(250), move || match (weak_app.upgrade(), weak_net.upgrade()) {
            (Some(app), Some(net)) => {
                net.poll(&app);
                glib::ControlFlow::Continue
            }
            _ => glib::ControlFlow::Break,
        });

        let weak_app = Rc::downgrade(app);
        let weak_net = Rc::downgrade(&network);
        glib::timeout_add_local(Duration::from_secs(1), move || match (weak_app.upgrade(), weak_net.upgrade()) {
            (Some(app), Some(net)) => {
                net.publish(&app);
                glib::ControlFlow::Continue
            }
            _ => glib::ControlFlow::Break,
        });
        network
    }

    /// Quitte l'anneau (fermeture de l'application).
    pub fn leave(&self) {
        self.ring.leave();
    }

    pub fn cryptor_key(&self) -> String { self.ring.cryptor_key() }
    pub fn apply_local(&self, app: &Rc<App>, message: &Message) { on_message(app, &self.ring, message); }

    /// Diffuse les colis nouveaux ou modifiés, rappelle les disparus.
    fn publish(&self, app: &Rc<App>) {
        if app.tournament.try_borrow_mut().is_err() {
            return;
        }
        let pages: Vec<Rc<ContestPage>> = app.pages.borrow().clone();
        let mut wanted: Vec<Message> = Vec::new();
        let mut keys = HashMap::new();
        for page in &pages {
            if let Some(list) = page.try_with_contest(|c| c.referee_passwords()) {
                keys.extend(list);
            }
            if let Some(parcels) = page.try_with_contest(|c| c.parcels()) {
                for p in parcels {
                    let mut m = Message::new(p.name);
                    m.set_net_id(p.net_id);
                    for (k, v) in &p.fields {
                        m.set_str(k, v);
                    }
                    wanted.push(m);
                }
            }
        }

        if let Ok(mut k) = self.keys.lock() {
            *k = keys;
        }
        let mut spread = self.spread.borrow_mut();
        let keys: Vec<(String, u32)> = wanted.iter().map(key).collect();
        let gone: Vec<(String, u32)> = spread.keys().filter(|k| !keys.contains(k)).cloned().collect();
        // Rappel dans l'ordre inverse de la diffusion (poules avant leur lot…).
        for k in gone.iter().rev() {
            if let Some(mut m) = spread.remove(k) {
                self.ring.recall_message(&mut m);
            }
        }
        for mut m in wanted {
            let k = key(&m);
            let changed = spread.get(&k).map(|old| old.parcel() != with_fitness(&m)).unwrap_or(true);
            if changed {
                self.ring.spread_message(&mut m);
                spread.insert(k, m);
            }
        }
    }

    /// Relève les événements de l'anneau.
    fn poll(&self, app: &Rc<App>) {
        while let Ok(event) = self.events.try_recv() {
            if let RingEvent::Message(m) = event {
                self.pending.borrow_mut().push_back(m);
            }
        }
        // Tournoi occupé (boîte de dialogue ouverte) : on réessaiera.
        while app.tournament.try_borrow_mut().is_ok() {
            let Some(m) = self.pending.borrow_mut().pop_front() else { break };
            if app.management.borrow().as_ref().is_some_and(|m| m.enabled())
                && (m.is("BellePoule2D::Referee") || m.is("BellePoule2D::Roadmap") || m.is("BellePoule2D::EndOfBurst")) { continue; }
            on_message(app, &self.ring, &m);
        }
    }
}

fn key(m: &Message) -> (String, u32) {
    (m.name().unwrap_or_default().to_string(), m.net_id())
}

/// Texte du message tel qu'il est mémorisé après diffusion (fitness 1).
fn with_fitness(m: &Message) -> String {
    let mut c = m.clone();
    c.set_fitness(1);
    c.parcel()
}

/// `Tournament::OnMessage`.
fn on_message(app: &Rc<App>, ring: &Ring, m: &Message) {
    let pages: Vec<Rc<ContestPage>> = app.pages.borrow().clone();
    let competition = m.get_uint("competition");
    let of_competition = |p: &Rc<ContestPage>| p.with_contest(|c| c.net_id == competition).unwrap_or(false);

    if m.is("BellePoule2D::Referee") {
        let Some(xml) = m.get_str("xml") else { return };
        if m.fitness() == 0 {
            let Ok(doc) = bp_core::xml::parse(&xml) else { return };
            let mut referee = bp_core::Player::new(bp_core::PlayerClass::Referee); referee.load(doc.root_element());
            for page in &pages {
                let removed = page.with_contest(|c| {
                    let exists = c.referees.get(referee.reference).is_some();
                    if exists { c.referees.remove(referee.reference); } exists
                }).unwrap_or(false);
                if removed { touched(page); }
            }
            return;
        }
        let key = ring.cryptor_key();
        let iv = "AAAAAAAAAAAAAAAAAAAAAA==".to_string();
        let decrypt = |cypher: &str| bp_net::cryptor::Cryptor::decrypt(cypher, &iv, &key);
        for page in &pages {
            if page.with_contest(|c| c.manage_referee_xml(&xml, &decrypt)).unwrap_or(false) {
                touched(page);
            }
        }
    } else if m.is("BellePoule2D::Roadmap") {
        for page in pages.iter().filter(|p| of_competition(p)) {
            let applied = page
                .with_contest(|c| {
                    let pistes = m.get_list("pistes");
                    let pistes = if pistes.is_empty() { vec![m.get_uint("piste")] } else { pistes };
                    c.apply_pool_roadmap(
                        m.get_uint("source"),
                        m.fitness(),
                        &pistes,
                        m.get_str("start_date").as_deref(),
                        m.get_str("start_time").as_deref(),
                        &m.get_list("referees"),
                    ) || c.apply_table_roadmap(
                        m.get_uint("batch"),
                        m.get_uint("source"),
                        m.fitness(),
                        m.get_uint("piste"),
                        m.get_str("start_date").as_deref(),
                        m.get_str("start_time").as_deref(),
                        m.get_uint("duration_span"),
                        &m.get_list("referees"),
                    )
                })
                .unwrap_or(false);
            if applied {
                touched(page);
            }
        }
    } else if m.is("SmartPoule::ScoreSheetCall") {
        // Feuille de match demandée par le smartphone d'un arbitre.
        let (stage, batch, bout) = (m.get_uint("stage"), m.get_str("batch").unwrap_or_default(), m.get_str("bout").unwrap_or_default());
        let referee = m.get_uint("referee_id");
        let Some(address) = m.get_str("address").filter(|a| !a.is_empty()) else { return };
        for page in pages.iter().filter(|p| of_competition(p)) {
            let reply = page.with_contest(|c| {
                let xml = c.smartpoule_score_sheet(stage, &batch, &bout)?;
                let password = c.referee_password(referee)?;
                Some((xml, password, c.net_id))
            });
            if let Some(Some((xml, password, net_id))) = reply {
                let mut response = Message::new("BellePoule::ScoreSheet");
                response.set_uint("competition", net_id);
                response.set_uint("stage", stage);
                response.set_str("batch", &batch);
                if !bout.is_empty() {
                    response.set_str("bout", &bout);
                }
                response.set_str("xml", &xml);
                ring.stamp_sender(&mut response);
                response.set_passphrase256(Some(&password));
                let mut uploader = bp_net::message_uploader::MessageUploader::new(&address, None);
                uploader.push_message(&response);
                uploader.stop();
            }
        }
    } else if m.is("SmartPoule::Score") {
        // Score saisi sur le smartphone.
        let (stage, batch, bout) = (m.get_uint("stage"), m.get_str("batch").unwrap_or_default(), m.get_str("bout").unwrap_or_default());
        let Some(xml) = m.get_str("xml") else { return };
        for page in pages.iter().filter(|p| of_competition(p)) {
            if page.with_contest(|c| c.smartpoule_score(stage, &batch, &bout, &xml)).unwrap_or(false) {
                touched(page);
            }
        }
    } else if m.is("BellePoule2D::ClockOffset") {
        bp_core::bout::set_clock_offset(m.get_int("offset") as i64);
    } else if m.is("BellePoule2D::EndOfBurst") {
        for page in pages.iter().filter(|p| of_competition(p)) {
            if let Some(view) = page.schedule.borrow().clone() {
                view.refresh_screens();
            }
        }
    }
}

/// Compétition modifiée par le réseau : enregistrement et réaffichage.
fn touched(page: &Rc<ContestPage>) {
    page.mark_dirty();
    let view = page.schedule.borrow().clone();
    if let Some(view) = view {
        view.refresh_screens();
    }
}
