//! Marshaller : gestion de la salle (pistes, arbitres, planning) reliée à
//! BellePoule par le réseau local.

// Pas de console en version publiée (comme le C++ sous Windows).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use gtk4 as gtk;
use gtk::prelude::*;
use gtk::{gio, glib};

fn main() -> glib::ExitCode {
    let _ = env_logger_init();
    // Langue : celle choisie dans BellePoule, sinon celle du système.
    let config = bp_core::tournament::user_config::UserConfig::load(&bp_core::tournament::user_config::UserConfig::default_path());
    let lang = config
        .get_string("Tournament", "interface_language")
        .filter(|l| !l.is_empty())
        .unwrap_or_else(system_language);
    let dir = bp_gtk::resources::translations_dir();
    for candidate in [lang.as_str(), lang.split('_').next().unwrap_or("en")] {
        if bp_core::i18n::set_language(&dir, candidate).is_ok() {
            break;
        }
    }

    let application = gtk::Application::builder()
        .application_id("org.bellepoule.Marshaller")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    application.connect_activate(marshaller::ui::Ui::start);
    application.run_with_args::<&str>(&[])
}

fn env_logger_init() -> Option<()> {
    None
}

/// Langue du système (`fr` pour `fr_FR.UTF-8`), « en » si inconnue.
fn system_language() -> String {
    for name in glib::language_names() {
        let name = name.to_string();
        if name == "C" || name == "POSIX" {
            continue;
        }
        let lang = name.split(['.', '@']).next().unwrap_or("").split('_').next().unwrap_or("");
        if !lang.is_empty() {
            return lang.to_ascii_lowercase();
        }
    }
    "en".to_string()
}
