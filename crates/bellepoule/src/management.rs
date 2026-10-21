//! Organisation locale : une vue intégrée et des échanges en mémoire.
use crate::app::App;
use bp_core::i18n::tr;
use bp_net::message::Message;
use gtk4::{self as gtk, glib, prelude::*};
use marshaller::{
    model::NetCommand,
    ui::{ManagementFeatures, Ui},
};
use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, VecDeque},
    rc::{Rc, Weak},
    time::Duration,
};

pub struct Management {
    app: Weak<App>,
    root: gtk::Box,
    content: gtk::Box,
    hint: gtk::Label,
    referees: gtk::CheckButton,
    pistes: gtk::CheckButton,
    ui: RefCell<Option<Rc<Ui>>>,
    pending: Rc<RefCell<VecDeque<NetCommand>>>,
    inputs: RefCell<HashMap<(String, u32), Message>>,
    polling: Cell<bool>,
}

impl Management {
    pub fn new(app: &Rc<App>) -> Rc<Self> {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 8);
        root.add_css_class("bp-organisation-page");
        let options = gtk::Box::new(gtk::Orientation::Horizontal, 18);
        options.set_margin_start(12);
        options.set_margin_top(8);
        let referees = gtk::CheckButton::with_label(&tr("Manage referees"));
        let pistes = gtk::CheckButton::with_label(&tr("Manage pistes"));
        {
            let t = app.tournament.borrow();
            referees.set_active(t.config.get_integer("Organisation", "referees") != 0);
            pistes.set_active(t.config.get_integer("Organisation", "pistes") != 0);
        }
        options.append(&referees);
        options.append(&pistes);
        let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        options.append(&spacer);
        let close = gtk::Button::with_label(&tr("Close organisation"));
        close.set_margin_end(12);
        let weak = Rc::downgrade(app);
        close.connect_clicked(move |_| {
            if let Some(app) = weak.upgrade() {
                app.close_management();
            }
        });
        options.append(&close);
        root.append(&options);
        let hint = gtk::Label::new(Some(&tr("Optional organisation tools. Enable referees, pistes, or both. Your competitions remain available in their tabs.")));
        hint.set_wrap(true);
        hint.set_margin_start(12);
        hint.set_margin_end(12);
        root.append(&hint);
        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        content.set_vexpand(true);
        root.append(&content);
        let manager = Rc::new(Self {
            app: Rc::downgrade(app),
            root,
            content,
            hint,
            referees,
            pistes,
            ui: RefCell::new(None),
            pending: Rc::new(RefCell::new(VecDeque::new())),
            inputs: RefCell::new(HashMap::new()),
            polling: Cell::new(false),
        });
        for button in [&manager.referees, &manager.pistes] {
            let weak = Rc::downgrade(&manager);
            button.connect_toggled(move |_| {
                if let Some(m) = weak.upgrade() {
                    m.configure();
                }
            });
        }
        manager.configure();
        let weak = Rc::downgrade(&manager);
        glib::timeout_add_local(Duration::from_millis(400), move || {
            let Some(manager) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            manager.poll();
            glib::ControlFlow::Continue
        });
        manager
    }

    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }
    pub fn enabled(&self) -> bool {
        self.features().enabled()
    }
    fn features(&self) -> ManagementFeatures {
        ManagementFeatures {
            referees: self.referees.is_active(),
            pistes: self.pistes.is_active(),
        }
    }

    fn configure(&self) {
        let Some(app) = self.app.upgrade() else {
            return;
        };
        let features = self.features();
        if let Some(ui) = self.ui.borrow().as_ref() {
            self.drain_pending(&app, ui.management_features());
        }
        {
            let mut t = app.tournament.borrow_mut();
            t.config
                .set_integer("Organisation", "referees", features.referees as i64);
            t.config
                .set_integer("Organisation", "pistes", features.pistes as i64);
            let _ = t.save_config();
        }
        if features.enabled() && self.ui.borrow().is_none() {
            let key = app
                .network
                .borrow()
                .as_ref()
                .map(|n| n.cryptor_key())
                .unwrap_or_else(bp_net::flash_code::get_key256);
            let pending = self.pending.clone();
            match Ui::embedded(
                &app.window,
                &key,
                Rc::new(move |command| pending.borrow_mut().push_back(command)),
            ) {
                Ok(ui) => {
                    self.content.append(&ui.widget());
                    *self.ui.borrow_mut() = Some(ui);
                }
                Err(e) => {
                    bp_gtk::dialogs::error(Some(&app.window), &e.to_string());
                    return;
                }
            }
        }
        if let Some(ui) = self.ui.borrow().as_ref() {
            ui.set_features(features);
            ui.widget().set_visible(features.enabled());
        }
        self.hint.set_visible(!features.enabled());
    }

    pub fn flush(&self) {
        self.poll();
        if let Some(app) = self.app.upgrade() {
            self.drain_pending(&app, self.features());
        }
    }

    fn poll(&self) {
        if !self.enabled() || self.polling.replace(true) {
            return;
        }
        self.poll_inner();
        self.polling.set(false);
    }

    fn poll_inner(&self) {
        let Some(app) = self.app.upgrade() else {
            return;
        };
        let Some(ui) = self.ui.borrow().clone() else {
            return;
        };
        let mut wanted = Vec::new();
        let mut weapons = Vec::new();
        {
            let Ok(t) = app.tournament.try_borrow() else {
                return;
            };
            for page in app.pages.borrow().iter() {
                if let Some(contest) = t.contest(page.id) {
                    if !weapons.contains(&contest.weapon.xml_image) {
                        weapons.push(contest.weapon.xml_image);
                    }
                    for parcel in contest.parcels() {
                        let mut m = Message::new(parcel.name);
                        m.set_net_id(parcel.net_id);
                        m.set_fitness(1);
                        for (key, value) in parcel.fields {
                            m.set_str(&key, &value);
                        }
                        wanted.push(m);
                    }
                }
            }
        }
        // Ne pas dépendre de la publication des matchs : l'arme doit être
        // visible dès l'ouverture, même avant la composition des poules.
        ui.set_visible_weapons(&weapons);
        let mut changed = false;
        {
            let mut inputs = self.inputs.borrow_mut();
            let keys: Vec<_> = wanted.iter().map(key).collect();
            let gone: Vec<_> = inputs
                .keys()
                .filter(|k| !keys.contains(k))
                .cloned()
                .collect();
            let mut hall = ui.hall.borrow_mut();
            // Retirer d'abord les matchs, puis les lots et enfin les compétitions.
            let mut removed: Vec<_> = gone.into_iter().filter_map(|k| inputs.remove(&k)).collect();
            removed.sort_by_key(|m| {
                if m.is("BellePoule::Job") {
                    0
                } else if m.is("BellePoule::Batch") {
                    1
                } else {
                    2
                }
            });
            for mut m in removed {
                m.set_fitness(0);
                changed |= hall.on_message(&m);
            }
            for m in wanted {
                let k = key(&m);
                if inputs.get(&k).is_none_or(|old| old.parcel() != m.parcel()) {
                    changed |= hall.on_message(&m);
                    inputs.insert(k, m);
                }
            }
        }
        if changed {
            ui.after_change();
        }
        self.drain_pending(&app, self.features());
    }

    fn drain_pending(&self, app: &Rc<App>, features: ManagementFeatures) {
        let network = app.network.borrow().clone();
        loop {
            let command = {
                let mut queue = self.pending.borrow_mut();
                queue.pop_front()
            };
            let Some(command) = command else { break };
            let m = match command {
                NetCommand::Spread(mut m) => {
                    m.set_fitness(1);
                    m
                }
                NetCommand::Recall(mut m) => {
                    m.set_fitness(0);
                    m
                }
                _ => continue,
            };
            if m.is("BellePoule2D::Referee") && !features.referees {
                continue;
            }
            if let Some(network) = &network {
                network.apply_local(app, &m);
            }
        }
    }
}

fn key(m: &Message) -> (String, u32) {
    (m.name().unwrap_or_default().to_string(), m.net_id())
}
