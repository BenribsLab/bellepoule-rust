//! BellePoule : logiciel de gestion de compétitions d'escrime.
//!
//! Options : `bellepoule [fichier]` ouvre un fichier ; `--smoke-test` ouvre
//! la fenêtre principale, puis quitte au bout d'une seconde (vérification
//! automatique).

#![allow(deprecated)]

mod app;
mod contest_page;
mod language;
mod schedule_view;
mod stages;
mod widgets;

use std::path::PathBuf;

use gtk4 as gtk;
use gtk::prelude::*;
use gtk::{gio, glib};

fn main() -> glib::ExitCode {
    env_logger::init();

    let mut file: Option<PathBuf> = None;
    let mut smoke_test = false;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--smoke-test" => smoke_test = true,
            a if !a.starts_with('-') => file = Some(PathBuf::from(a)),
            _ => {}
        }
    }

    let application = gtk::Application::builder()
        .application_id("org.bellepoule.BellePoule")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    application.connect_activate(move |a| app::App::start(a, file.clone(), smoke_test));
    application.run_with_args::<&str>(&[])
}
