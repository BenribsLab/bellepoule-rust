//! Éléments visuels partagés de la nouvelle interface.
use bp_core::i18n::tr;
use bp_gtk::resources;
use gtk4::{self as gtk, prelude::*};

pub fn icon(name: &str, size: i32) -> gtk::Image {
    let image = gtk::Image::from_file(
        resources::resources_dir()
            .join("visuals/icons")
            .join(format!("{name}.svg")),
    );
    image.set_pixel_size(size);
    image
}

pub fn action(icon_name: &str, label: &str) -> gtk::Button {
    let button = gtk::Button::new();
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 9);
    content.append(&icon(icon_name, 19));
    content.append(&gtk::Label::new(Some(&tr(label))));
    button.set_child(Some(&content));
    button
}

/// Illustrations utilisées en fond (CSS) : une image de fond ne réserve
/// aucune place, contrairement à `GtkPicture` dont la taille naturelle est
/// celle du fichier (c'est ce qui faisait occuper la moitié de la fenêtre
/// au bandeau et à la barre latérale).
pub fn install_backgrounds() {
    thread_local! {
        static DONE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }
    if DONE.with(|d| d.replace(true)) {
        return;
    }
    let uri = |file: &str| gtk::gio::File::for_path(resources::resources_dir().join("visuals").join(file)).uri();
    let css = format!(
        ".bp-hero-art {{ background-image: url(\"{}\"); background-size: cover; background-position: right center; background-repeat: no-repeat; }}\n\
         .bp-sidebar-art {{ background-image: url(\"{}\"); background-size: contain; background-position: center; background-repeat: no-repeat; }}\n",
        uri("fencing-header.png"),
        uri("fencers-sidebar.png")
    );
    let provider = gtk::CssProvider::new();
    provider.load_from_data(&css);
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(&display, &provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
    }
}

/// Zone décorative de hauteur fixe dont l'image est un fond CSS.
pub fn background_art(class: &str, height: i32) -> gtk::Box {
    install_backgrounds();
    let area = gtk::Box::new(gtk::Orientation::Vertical, 0);
    area.add_css_class(class);
    area.set_height_request(height);
    area.set_hexpand(false);
    area.set_vexpand(false);
    area
}


pub struct CompetitionHeader {
    pub root: gtk::Box,
    pub name: gtk::Label,
    pub subtitle: gtk::Label,
    pub meta: gtk::Label,
    pub phase: gtk::Label,
}

impl CompetitionHeader {
    pub fn new() -> Self {
        // Bandeau compact : hauteur naturelle du texte, photo en fond CSS
        // (elle ne peut plus imposer sa taille à la page).
        install_backgrounds();
        let root = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        root.add_css_class("bp-hero");
        root.add_css_class("bp-hero-art");
        root.set_overflow(gtk::Overflow::Hidden);
        root.set_vexpand(false);
        root.set_hexpand(true);
        let content = gtk::Box::new(gtk::Orientation::Horizontal, 14);
        content.set_margin_start(18);
        content.set_margin_end(18);
        content.set_margin_top(10);
        content.set_margin_bottom(10);
        content.set_valign(gtk::Align::Center);
        content.set_hexpand(true);
        let emblem = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        emblem.add_css_class("bp-emblem");
        emblem.set_valign(gtk::Align::Center);
        emblem.append(&icon("swords", 24));
        content.append(&emblem);
        let copy = gtk::Box::new(gtk::Orientation::Vertical, 2);
        copy.set_hexpand(true);
        let name = gtk::Label::new(None);
        name.set_xalign(0.0);
        name.set_ellipsize(gtk::pango::EllipsizeMode::End);
        name.add_css_class("bp-hero-title");
        copy.append(&name);
        let subtitle = gtk::Label::new(None);
        subtitle.set_xalign(0.0);
        subtitle.add_css_class("bp-hero-subtitle");
        copy.append(&subtitle);
        let meta = gtk::Label::new(None);
        meta.set_xalign(0.0);
        meta.set_ellipsize(gtk::pango::EllipsizeMode::End);
        meta.add_css_class("bp-muted");
        copy.append(&meta);
        content.append(&copy);
        let phase = gtk::Label::new(None);
        phase.add_css_class("bp-phase-pill");
        phase.set_valign(gtk::Align::Start);
        content.append(&phase);
        root.append(&content);
        Self {
            root,
            name,
            subtitle,
            meta,
            phase,
        }
    }
}
