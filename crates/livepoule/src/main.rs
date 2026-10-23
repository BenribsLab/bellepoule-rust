//! LivePoule : programme principal (port de `LivePoule/main.cpp`).
//!
//! Option `--smoke-test` : ouvre l'écran puis quitte après 1 s.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gtk4 as gtk;
use gtk4::prelude::*;
use gtk4::{gio, glib};

use livepoule::scoring_machine::SgMachine;
use livepoule::screen::Screen;

const ESC: &str = "\x1b[0m";
const RED: &str = "\x1b[1;31m";
const BLUE: &str = "\x1b[1;34m";

/// `LogHandler` : `[domaine] message`, en rouge pour les avertissements et
/// erreurs, en bleu sinon.
struct Logger;

impl log::Log for Logger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Info
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let color = if record.level() <= log::Level::Warn { RED } else { BLUE };
        println!("{color}[LivePoule]{ESC} {}", record.args());
    }

    fn flush(&self) {}
}

static LOGGER: Logger = Logger;

fn main() -> glib::ExitCode {
    let _ = log::set_logger(&LOGGER).map(|()| log::set_max_level(log::LevelFilter::Info));

    let smoke_test = std::env::args().any(|a| a == "--smoke-test");

    // `--live <adresse>` : liaison au serveur BellePoule Web (adresse affichée
    // par BellePoule › BellePoule2D › SmartPoule…) ; `--piste <n>` : piste de
    // l'écran. Les deux sont mémorisés pour les lancements suivants.
    {
        let args: Vec<String> = std::env::args().collect();
        let value = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned();
        let mut settings = livepoule::web_link::Settings::load();
        let mut changed = false;
        if let Some(url) = value("--live") {
            match livepoule::web_link::LiveAddress::parse(&url) {
                Some(a) => {
                    settings.address = Some(a);
                    changed = true;
                }
                None => eprintln!("Adresse de liaison invalide : {url}"),
            }
        }
        if let Some(p) = value("--piste").and_then(|p| p.parse::<u32>().ok()).filter(|p| *p > 0) {
            settings.piste = Some(p);
            changed = true;
        }
        if changed {
            settings.save();
        }
    }

    let app = gtk::Application::builder()
        .application_id("org.bellepoule.LivePoule")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();

    // L'écran vit aussi longtemps que l'application (`screen->Release ()` à la fin)
    let screen: Rc<RefCell<Option<Rc<Screen>>>> = Rc::default();
    {
        let screen = screen.clone();
        app.connect_activate(move |app| {
            let s = Screen::new(app);
            s.manage_scoring_machine(&SgMachine);
            *screen.borrow_mut() = Some(s);

            if smoke_test {
                let app = app.clone();
                glib::timeout_add_local_once(Duration::from_secs(1), move || {
                    println!("smoke-test OK");
                    app.quit();
                });
            }
        });
    }

    // Les options sont traitées ici et non par GTK
    let program = std::env::args().next().unwrap_or_else(|| "livepoule".into());
    let code = app.run_with_args(&[program]);
    screen.borrow_mut().take();
    code
}
