//! Écran de piste GTK4 (port de `LivePoule/screen.cpp`).
//!
//! Commandes clavier (comme le C++) :
//! - `Échap` : quitte le plein écran ;
//! - `Entrée` / `Entrée` du pavé numérique : affiche/masque le QR code ;
//! - `+` / `-` du pavé numérique : piste suivante/précédente.
//!
//! GPIO simulé (hors Raspberry Pi) : maintenir la touche `0` à `8` met la
//! broche WiringPi correspondante au niveau bas (0-5 : sorties de l'appareil
//! de score ; 6 : bouton QR code ; 7 : piste +1 ; 8 : piste -1).

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::time::Duration;

use gtk4 as gtk;
use gtk4::prelude::*;
use gtk4::{gdk, glib, pango};

use crate::button::{screen_buttons, Button, ButtonAction, DEBOUNCE_DELAY};
use crate::gpio::{default_backend, GpioHub, PinId};
use crate::light::LightOutput;
use crate::model::ScreenModel;
use crate::network::{HttpNetwork, LocalNetwork, Network, SecretKeyProvider};
use crate::scoring_machine::{LightBoard, ScoringMachine};
use crate::timer::{real_time_us, TimerCommand};
use crate::wifi_code::{claim_ip_port, render_qr, WifiCode};
use crate::wpa::Wpa;

/// Interface (conversion GTK4 de `LivePoule.glade`), embarquée dans le binaire.
pub const UI: &str = include_str!("../../../resources/ui/livepoule/LivePoule.ui");

/// Fond des widgets autres que les étiquettes (`Rescale`).
const BACKGROUND: &str = "#333333";

/// Période de relève des événements réseau, GPIO et Wi-Fi (équivalent des
/// `g_idle_add` du C++).
const POLL_PERIOD: Duration = Duration::from_millis(20);

struct Widgets {
    window: gtk::Window,
    strip: gtk::Label,
    title: gtk::Label,
    red_fencer: gtk::Label,
    red_score: gtk::Label,
    green_fencer: gtk::Label,
    green_score: gtk::Label,
    timer: gtk::Label,
    code_image: gtk::Picture,
    spinner: gtk::Spinner,
}

/// L'écran.
pub struct Screen {
    builder: gtk::Builder,
    w: Widgets,
    css: gtk::CssProvider,
    /// Couleur de fond par nom de widget.
    backgrounds: RefCell<BTreeMap<String, String>>,
    model: RefCell<ScreenModel>,
    gpio: RefCell<GpioHub>,
    lights: RefCell<LightBoard>,
    buttons: RefCell<Vec<Button>>,
    network: RefCell<Box<dyn Network>>,
    wifi_code: WifiCode,
    wpa: Wpa,
    wpa_done: RefCell<Option<Receiver<()>>>,
    timer_source: RefCell<Option<glib::SourceId>>,
}

fn object<T: IsA<glib::Object>>(builder: &gtk::Builder, id: &str) -> T {
    builder.object(id).unwrap_or_else(|| panic!("LivePoule.ui : objet « {id} » introuvable"))
}

impl Screen {
    /// `Screen::Screen` : construit l'écran, l'affiche en plein écran et
    /// démarre le serveur HTTP.
    pub fn new(app: &gtk::Application) -> Rc<Screen> {
        let builder = gtk::Builder::from_string(UI);
        let w = Widgets {
            window: object(&builder, "root"),
            strip: object(&builder, "strip"),
            title: object(&builder, "title"),
            red_fencer: object(&builder, "red_fencer"),
            red_score: object(&builder, "red_score"),
            green_fencer: object(&builder, "green_fencer"),
            green_score: object(&builder, "green_score"),
            timer: object(&builder, "timer"),
            code_image: object(&builder, "code_image"),
            spinner: object(&builder, "spinner"),
        };
        w.window.set_application(Some(app));

        // Les noms de widgets reprennent les identifiants (sélecteurs CSS)
        let mut backgrounds = BTreeMap::new();
        for obj in builder.objects() {
            if let Ok(widget) = obj.downcast::<gtk::Widget>() {
                if let Some(id) = widget.buildable_id() {
                    widget.set_widget_name(&id);
                    if !widget.is::<gtk::Label>() {
                        backgrounds.insert(id.to_string(), BACKGROUND.to_string());
                    }
                }
            }
        }

        let css = gtk::CssProvider::new();
        if let Some(display) = gdk::Display::default() {
            gtk::style_context_add_provider_for_display(&display, &css, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1);
        }

        let wifi_code = WifiCode::new("Piste");
        let network: Box<dyn Network> = {
            let key = wifi_code.secret_key();
            let provider: SecretKeyProvider = Arc::new(move |_scheme| Some(key.get()));
            match HttpNetwork::start(claim_ip_port(), provider) {
                Ok(n) => Box::new(n),
                Err(e) => {
                    log::warn!("Serveur HTTP : {e}");
                    Box::new(LocalNetwork::default())
                }
            }
        };

        let screen = Rc::new(Screen {
            builder,
            w,
            css,
            backgrounds: RefCell::new(backgrounds),
            model: RefCell::new(ScreenModel::new()),
            gpio: RefCell::new(GpioHub::new(default_backend())),
            lights: RefCell::new(LightBoard::new()),
            buttons: RefCell::new(screen_buttons()),
            network: RefCell::new(network),
            wifi_code,
            wpa: Wpa::new(),
            wpa_done: RefCell::new(None),
            timer_source: RefCell::new(None),
        });

        // Boutons
        {
            let mut gpio = screen.gpio.borrow_mut();
            for b in screen.buttons.borrow().iter() {
                gpio.register(b.pin());
            }
        }

        screen.connect_signals();

        // Affichage plein écran puis mise à l'échelle
        screen.w.window.fullscreen();
        screen.rescale_to_monitor();
        screen.w.window.present();

        screen.sync_view();
        screen.start_polling();
        screen
    }

    /// `Screen::ManageScoringMachine`.
    pub fn manage_scoring_machine(self: &Rc<Self>, machine: &dyn ScoringMachine) {
        log::info!("Appareil de score : {}", machine.class_name());
        machine.connect_to_lights(&mut self.lights.borrow_mut());
        {
            let pins = self.lights.borrow().wired_pins();
            let mut gpio = self.gpio.borrow_mut();
            for p in pins {
                gpio.register(p);
            }
        }
        self.on_light_event();
    }

    pub fn window(&self) -> &gtk::Window {
        &self.w.window
    }

    fn connect_signals(self: &Rc<Self>) {
        // on_root_delete_event
        self.w.window.connect_close_request(|window| {
            if let Some(app) = window.application() {
                app.quit();
            }
            glib::Propagation::Proceed
        });

        // on_root_key_press_event
        let keys = gtk::EventControllerKey::new();
        {
            let screen = Rc::downgrade(self);
            keys.connect_key_pressed(move |_, key, _, _| match screen.upgrade() {
                Some(s) if s.on_key_pressed(key) => glib::Propagation::Stop,
                _ => glib::Propagation::Proceed,
            });
        }
        {
            let screen = Rc::downgrade(self);
            keys.connect_key_released(move |_, key, _, _| {
                if let (Some(s), Some(pin)) = (screen.upgrade(), simulated_pin(key)) {
                    s.gpio.borrow_mut().simulate(pin, 1);
                }
            });
        }
        self.w.window.add_controller(keys);
    }

    /// `Screen::OnKeyPressed`.
    fn on_key_pressed(self: &Rc<Self>, key: gdk::Key) -> bool {
        if key == gdk::Key::Escape {
            self.w.window.unfullscreen();
            true
        } else if key == gdk::Key::KP_Enter || key == gdk::Key::Return {
            self.toggle_wifi_code();
            true
        } else if key == gdk::Key::KP_Add {
            self.change_strip_id(1);
            true
        } else if key == gdk::Key::KP_Subtract {
            self.change_strip_id(-1);
            true
        } else if let Some(pin) = simulated_pin(key) {
            self.gpio.borrow_mut().simulate(pin, 0)
        } else {
            false
        }
    }

    /// `Screen::Rescale` : facteur entier (division entière du C++) entre la
    /// taille du moniteur et la taille naturelle de la fenêtre.
    fn rescale_to_monitor(&self) {
        let Some(display) = gdk::Display::default() else { return };
        let Some(monitor) = display.monitors().item(0).and_then(|m| m.downcast::<gdk::Monitor>().ok()) else {
            return;
        };
        let geometry = monitor.geometry();
        let (_, natural) = self.w.window.preferred_size();
        if natural.width() <= 0 || natural.height() <= 0 {
            return;
        }
        let factor = (geometry.width() / natural.width()).min(geometry.height() / natural.height());
        // Écart : un facteur nul (fenêtre plus grande que l'écran) rendrait
        // le texte invisible ; on garde au moins l'échelle d'origine.
        self.rescale(factor.max(1) as f64);
    }

    fn rescale(&self, factor: f64) {
        for obj in self.builder.objects() {
            if let Ok(label) = obj.downcast::<gtk::Label>() {
                let Some(attrs) = label.attributes() else { continue };
                let mut scale = None;
                let _ = attrs.filter(|a| {
                    if let Some(f) = a.downcast_ref::<pango::AttrFloat>() {
                        if a.type_() == pango::AttrType::Scale {
                            scale = Some(f.value());
                        }
                    }
                    false
                });
                if let Some(value) = scale {
                    let copy = attrs.copy().unwrap_or_default();
                    copy.change(pango::AttrFloat::new_scale(value * factor));
                    label.set_attributes(Some(&copy));
                }
            }
        }
    }

    /// Met à jour le fond d'un widget (`gtk_widget_override_background_color`).
    fn set_background(&self, widget: &str, color: Option<&str>) {
        let css_color = match color {
            None => "transparent".to_string(),
            Some(c) => match gdk::RGBA::parse(c) {
                Ok(rgba) => rgba.to_string(),
                Err(_) => {
                    log::warn!("Couleur invalide : {c}");
                    return;
                }
            },
        };
        self.backgrounds.borrow_mut().insert(widget.to_string(), css_color);
    }

    fn apply_css(&self) {
        let mut css = String::new();
        for (name, color) in self.backgrounds.borrow().iter() {
            css.push_str(&format!("#{name} {{ background-color: {color}; background-image: none; }}\n"));
        }
        self.css.load_from_string(&css);
    }

    /// Recopie le modèle dans les widgets.
    fn sync_view(&self) {
        let m = self.model.borrow();
        self.w.strip.set_text(&m.strip_text());
        self.w.title.set_text(&m.title);
        self.w.red_fencer.set_text(&m.red_fencer);
        self.w.red_score.set_text(&m.red_score);
        self.w.green_fencer.set_text(&m.green_fencer);
        self.w.green_score.set_text(&m.green_score);
        self.w.timer.set_text(&m.timer.text());
        self.w.code_image.set_visible(m.code_visible);
        self.w.spinner.set_visible(m.spinner_visible);
        let color = m.color.clone();
        drop(m);
        self.set_background("title_box", Some(&color));
        self.apply_css();
    }

    /// `Screen::OnLightEvent` : rafraîchit toutes les lampes.
    fn on_light_event(&self) {
        let outputs = {
            let gpio = self.gpio.borrow();
            self.lights.borrow().refresh(|p| gpio.voltage(p))
        };
        for (widget, output) in outputs {
            match output {
                LightOutput::Color(c) => self.set_background(&widget, Some(&c)),
                LightOutput::Transparent => self.set_background(&widget, None),
                LightOutput::Unchanged => {}
            }
        }
        self.apply_css();
    }

    /// `Screen::ChangeStripId`.
    fn change_strip_id(&self, step: i32) {
        self.model.borrow_mut().change_strip_id(step);
        self.sync_view();
    }

    /// `Screen::ToggleWifiCode`.
    fn toggle_wifi_code(&self) {
        let start = self.model.borrow_mut().toggle_wifi_code();
        if start {
            let (tx, rx) = channel();
            *self.wpa_done.borrow_mut() = Some(rx);
            self.wpa.configure_network(tx);
        }
        self.sync_view();
    }

    /// `Screen::OnNetworkConfigured` : nouvelle clé et nouveau QR code.
    fn on_network_configured(&self) {
        self.wifi_code.reset_key();
        let text = {
            let net = self.network.borrow();
            self.wifi_code.text(&net.ip_v4_address(), net.port())
        };
        log::info!("QR code : {text}");
        if let Some(img) = render_qr(&text, 3) {
            let bytes = glib::Bytes::from_owned(img.rgb);
            let texture = gdk::MemoryTexture::new(
                img.size as i32,
                img.size as i32,
                gdk::MemoryFormat::R8g8b8,
                &bytes,
                img.size * 3,
            );
            self.w.code_image.set_paintable(Some(&texture));
        }
        self.model.borrow_mut().network_configured();
        self.sync_view();
    }

    /// `Screen::OnMessage`.
    fn on_message(self: &Rc<Self>, message: &bp_net::Message) {
        let outcome = self.model.borrow_mut().on_message(message, real_time_us());
        match outcome.timer {
            TimerCommand::StartTicking => self.start_ticking(),
            TimerCommand::StopTicking => self.stop_ticking(),
            TimerCommand::None => {}
        }
        self.sync_view();
    }

    fn stop_ticking(&self) {
        if let Some(id) = self.timer_source.borrow_mut().take() {
            id.remove();
        }
    }

    /// `g_timeout_add_seconds (1, Timer::OnTimeout)`.
    fn start_ticking(self: &Rc<Self>) {
        self.stop_ticking();
        let screen = Rc::downgrade(self);
        let id = glib::timeout_add_seconds_local(1, move || {
            let Some(s) = screen.upgrade() else { return glib::ControlFlow::Break };
            s.model.borrow_mut().timer.tick(real_time_us());
            s.w.timer.set_text(&s.model.borrow().timer.text());
            glib::ControlFlow::Continue
        });
        *self.timer_source.borrow_mut() = Some(id);
    }

    /// Fronts GPIO : lampes ou boutons.
    fn on_gpio_event(self: &Rc<Self>, pin: PinId) {
        if self.lights.borrow().wired_pins().contains(&pin) {
            self.on_light_event();
        }
        let indices: Vec<usize> = self
            .buttons
            .borrow()
            .iter()
            .enumerate()
            .filter(|(_, b)| b.pin() == pin)
            .map(|(i, _)| i)
            .collect();
        for i in indices {
            // Button::OnButtonEvent : anti-rebond de 50 ms
            if self.buttons.borrow_mut()[i].on_event() {
                let screen = Rc::downgrade(self);
                glib::timeout_add_local_once(DEBOUNCE_DELAY, move || {
                    if let Some(s) = screen.upgrade() {
                        let voltage = s.gpio.borrow().voltage(pin);
                        let action = s.buttons.borrow_mut()[i].on_debounce_stop(voltage);
                        match action {
                            Some(ButtonAction::QrCode) => s.toggle_wifi_code(),
                            Some(ButtonAction::StripPlus) => s.change_strip_id(1),
                            Some(ButtonAction::StripMinus) => s.change_strip_id(-1),
                            None => {}
                        }
                    }
                });
            }
        }
    }

    fn start_polling(self: &Rc<Self>) {
        let screen = Rc::downgrade(self);
        glib::timeout_add_local(POLL_PERIOD, move || {
            let Some(s) = screen.upgrade() else { return glib::ControlFlow::Break };
            loop {
                let message = s.network.borrow_mut().try_recv();
                match message {
                    Some(m) => s.on_message(&m),
                    None => break,
                }
            }
            let events = s.gpio.borrow().pending_events();
            for pin in events {
                s.on_gpio_event(pin);
            }
            let done = s.wpa_done.borrow().as_ref().map(|rx| rx.try_recv().is_ok()).unwrap_or(false);
            if done {
                *s.wpa_done.borrow_mut() = None;
                s.on_network_configured();
            }
            glib::ControlFlow::Continue
        });
    }
}

/// Touche du GPIO simulé : `0` à `8` -> broche WiringPi.
fn simulated_pin(key: gdk::Key) -> Option<PinId> {
    let c = key.to_unicode()?;
    match c {
        '0'..='8' => Some(c as u8 - b'0'),
        _ => None,
    }
}
