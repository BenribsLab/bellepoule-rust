//! Fenêtre d'attente pendant l'ouverture d'une compétition : la fenêtre
//! principale est floutée, une barre de progression avance à chaque étape
//! (lecture du fichier, puis chaque écran construit).
//!
//! La construction des écrans se fait sur le fil de l'interface : entre deux
//! étapes, la boucle d'événements tourne un instant pour que la fenêtre
//! d'attente se redessine. Elle est modale : aucun clic n'arrive à
//! l'application pendant ce temps.

use std::cell::RefCell;
use std::time::{Duration, Instant};

use bp_core::i18n::tr;
use gtk::glib;
use gtk::prelude::*;
use gtk4 as gtk;

struct Loading {
    window: gtk::Window,
    parent: gtk::Window,
    bar: gtk::ProgressBar,
    detail: gtk::Label,
    done: usize,
    total: usize,
}

thread_local! {
    static CURRENT: RefCell<Option<Loading>> = const { RefCell::new(None) };
}

/// Laisse la boucle d'événements dessiner (environ une image).
fn pump(at_least: Duration) {
    let context = glib::MainContext::default();
    let until = Instant::now() + at_least;
    loop {
        while context.pending() {
            context.iteration(false);
        }
        if Instant::now() >= until {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Ouvre la fenêtre d'attente au-dessus de `parent`.
pub fn begin(parent: &gtk::Window, title: &str) {
    end();
    let window = gtk::Window::new();
    window.set_transient_for(Some(parent));
    window.set_modal(true);
    window.set_decorated(false);
    window.set_resizable(false);
    window.set_default_size(420, -1);
    window.add_css_class("bp-loading");
    let content = gtk::Box::new(gtk::Orientation::Vertical, 10);
    content.set_margin_top(22);
    content.set_margin_bottom(22);
    content.set_margin_start(26);
    content.set_margin_end(26);
    let head = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let spinner = gtk::Spinner::new();
    spinner.set_size_request(24, 24);
    spinner.start();
    head.append(&spinner);
    let heading = gtk::Label::new(Some(&tr("Opening the competition…")));
    heading.add_css_class("bp-loading-title");
    heading.set_xalign(0.0);
    head.append(&heading);
    content.append(&head);
    let name = gtk::Label::new(Some(title));
    name.set_xalign(0.0);
    name.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    name.add_css_class("bp-muted");
    content.append(&name);
    let bar = gtk::ProgressBar::new();
    bar.set_fraction(0.04);
    content.append(&bar);
    let detail = gtk::Label::new(Some(&tr("Reading the file…")));
    detail.set_xalign(0.0);
    detail.add_css_class("bp-muted");
    content.append(&detail);
    window.set_child(Some(&content));
    parent.add_css_class("bp-busy");
    window.present();
    // La fenêtre doit être affichée avant que le travail ne commence.
    let start = Instant::now();
    while !window.is_mapped() && start.elapsed() < Duration::from_millis(500) {
        pump(Duration::from_millis(5));
    }
    pump(Duration::from_millis(40));
    CURRENT.with(|c| *c.borrow_mut() = Some(Loading { window, parent: parent.clone(), bar, detail, done: 0, total: 1 }));
}

/// Nombre d'étapes restantes, une fois le fichier lu.
pub fn set_steps(steps: usize) {
    CURRENT.with(|c| {
        if let Some(l) = c.borrow_mut().as_mut() {
            l.done = 1;
            l.total = steps + 1;
            l.bar.set_fraction(1.0 / l.total as f64);
        }
    });
    pump(Duration::from_millis(16));
}

/// Une étape de plus est faite ; `next` décrit la suivante.
pub fn step(next: &str) {
    let active = CURRENT.with(|c| {
        let mut c = c.borrow_mut();
        let Some(l) = c.as_mut() else { return false };
        l.done = (l.done + 1).min(l.total);
        l.bar.set_fraction(l.done as f64 / l.total.max(1) as f64);
        l.detail.set_text(next);
        true
    });
    if active {
        pump(Duration::from_millis(16));
    }
}

/// Ferme la fenêtre d'attente et rend la main à l'application.
pub fn end() {
    if let Some(l) = CURRENT.with(|c| c.borrow_mut().take()) {
        l.bar.set_fraction(1.0);
        l.parent.remove_css_class("bp-busy");
        l.window.destroy();
    }
}
