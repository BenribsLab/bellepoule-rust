//! Présentation commune, activée par l'application hôte avant le chargement Glade.
#![allow(deprecated)]
use crate::glade::Glade;
use gtk4::{self as gtk, prelude::*};
use std::cell::Cell;

thread_local! { static ENABLED: Cell<bool> = const { Cell::new(false) }; }

pub fn install() {
    if ENABLED.with(|enabled| enabled.replace(true)) {
        return;
    }
    if let Some(display) = gtk::gdk::Display::default() {
        let provider = gtk::CssProvider::new();
        provider.load_from_string(include_str!("appearance.css"));
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
    let windows = gtk::Window::toplevels();
    let decorate = |model: &gtk::gio::ListModel, start: u32, count: u32| {
        for i in start..start + count {
            if let Some(window) = model.item(i).and_downcast::<gtk::Window>() {
                window.add_css_class("bp-app");
            }
        }
    };
    decorate(&windows, 0, windows.n_items());
    windows.connect_items_changed(move |model, start, _, added| decorate(model, start, added));
}

/// Enlever l'expansion héritée de GTK2 sur les commandes, pas sur les listes
/// et les canevas. Les widgets et leurs signaux sont conservés.
fn compact(widget: &gtk::Widget) {
    widget.set_vexpand(false);
    widget.set_hexpand(false);
    if widget.is::<gtk::Button>()
        || widget.is::<gtk::Entry>()
        || widget.is::<gtk::ComboBox>()
        || widget.is::<gtk::SpinButton>()
    {
        widget.set_valign(gtk::Align::Center);
    }
    if widget.is::<gtk::Entry>() || widget.is::<gtk::ComboBox>() || widget.is::<gtk::SpinButton>() {
        return;
    }
    if let Some(button) = widget.downcast_ref::<gtk::Button>() {
        if let Some(content) = button.child().and_downcast::<gtk::Box>() {
            content.set_orientation(gtk::Orientation::Horizontal);
            content.set_spacing(6);
        }
    }
    if let Some(image) = widget.downcast_ref::<gtk::Image>() {
        image.set_pixel_size(18);
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        child = current.next_sibling();
        compact(&current);
    }
}

fn strip(builder: &gtk::Builder, id: &str) {
    if let Some(widget) = builder.object::<gtk::Widget>(id) {
        compact(&widget);
        widget.set_hexpand(true);
        widget.add_css_class("bp-command-bar");
    }
}

fn wrap_actions(container: &gtk::Box) {
    let flow = gtk::FlowBox::new();
    flow.set_selection_mode(gtk::SelectionMode::None);
    flow.set_min_children_per_line(1);
    flow.set_max_children_per_line(12);
    flow.set_column_spacing(4);
    flow.set_row_spacing(4);
    flow.set_hexpand(true);
    while let Some(child) = container.first_child() {
        container.remove(&child);
        if child.is::<gtk::Separator>() {
            continue;
        }
        flow.insert(&child, -1);
    }
    container.append(&flow);
}

fn checkin(builder: &gtk::Builder) {
    let Some(header) = builder.object::<gtk::Box>("hbox1") else {
        return;
    };
    let actions: gtk::Box = builder.object("toolbar6").expect("checkin toolbar");
    Glade::detach_from_parent(&actions);
    let stats = gtk::FlowBox::new();
    stats.set_selection_mode(gtk::SelectionMode::None);
    stats.set_min_children_per_line(1);
    stats.set_max_children_per_line(4);
    stats.set_column_spacing(8);
    stats.set_row_spacing(8);
    stats.add_css_class("bp-attendance-cards");
    for (label, counter, button, style) in [
        (
            "label2",
            "attending_label",
            "all_present_button",
            "bp-count-present",
        ),
        (
            "label3",
            "absent_label",
            "all_absent_button",
            "bp-count-absent",
        ),
    ] {
        let group = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        group.add_css_class("bp-presence-card");
        let control: gtk::Button = builder.object(button).expect("attendance action");
        Glade::detach_from_parent(&control);
        control.set_child(Some(&gtk::Image::from_icon_name(
            if counter == "attending_label" {
                "object-select-symbolic"
            } else {
                "window-close-symbolic"
            },
        )));
        control.add_css_class(if counter == "attending_label" {
            "bp-presence-icon"
        } else {
            "bp-absence-icon"
        });
        control.set_tooltip_text(Some(&bp_core::i18n::tr(if counter == "attending_label" {
            "Mark everyone present"
        } else {
            "Mark everyone absent"
        })));
        control.set_valign(gtk::Align::Center);
        group.append(&control);
        let value: gtk::Label = builder.object(counter).expect("attendance count");
        Glade::detach_from_parent(&value);
        value.set_attributes(None);
        value.set_width_chars(2);
        value.add_css_class(style);
        group.append(&value);
        let title: gtk::Label = builder.object(label).expect("attendance title");
        Glade::detach_from_parent(&title);
        title.add_css_class("bp-card-caption");
        group.append(&title);
        stats.insert(&group, -1);
    }
    while let Some(child) = header.first_child() {
        header.remove(&child);
    }
    header.set_orientation(gtk::Orientation::Vertical);
    header.set_spacing(4);
    header.add_css_class("bp-checkin-summary");
    header.append(&stats);
    wrap_actions(&actions);
    header.append(&actions);
    strip(builder, "hbox1");
    stats.set_hexpand(true);
    if let Some(root) = builder.object::<gtk::Widget>("root") {
        root.add_css_class("bp-checkin");
    }
    if let Some(add) = builder.object::<gtk::Widget>("add_player_button") {
        add.add_css_class("suggested-action");
    }
    // La barre globale a déjà été compactée ; le FlowBox doit, lui, suivre
    // la largeur disponible pour permettre le retour à la ligne.
    if let Some(flow) = actions.first_child() {
        flow.set_hexpand(true);
    }
    if let Some(scroller) = builder.object::<gtk::ScrolledWindow>("scrolledwindow3") {
        scroller.set_vexpand(true);
        scroller.set_hexpand(true);
        scroller.add_css_class("bp-data-table");
    }
    strip(builder, "team_hbox");
    strip(builder, "tree_control_hbox");
}

pub(crate) fn polish(builder: &gtk::Builder, file: &str) {
    if !ENABLED.with(Cell::get) {
        return;
    }
    for object in builder.objects() {
        let Ok(widget) = object.downcast::<gtk::Widget>() else {
            continue;
        };
        if widget.is::<gtk::Window>() {
            widget.add_css_class("bp-app");
        }
        if widget.has_css_class("toolbar") {
            compact(&widget);
            widget.set_hexpand(true);
            widget.add_css_class("bp-command-bar");
        }
        if let Some(notebook) = widget.downcast_ref::<gtk::Notebook>() {
            notebook.set_scrollable(true);
        }
    }
    // Ces régions ne contiennent que des paramètres ou des actions. Les
    // emplacements recevant les listes et canevas conservent leur expansion.
    match file {
        "checkin.ui" => checkin(builder),
        "pool_allocator.ui" => {
            strip(builder, "toolbar2");
            strip(builder, "hbox4");
            if let Some(actions) = builder.object::<gtk::Box>("toolbar2") {
                wrap_actions(&actions);
            }
            if let Some(options) = builder.object::<gtk::Box>("hbox4") {
                wrap_actions(&options);
            }
        }
        "pool_supervisor.ui" | "table_supervisor.ui" | "barrage.ui" => {
            strip(builder, "hbox1");
            strip(builder, "stage_configuration");
        }
        "schedule.ui" => {
            strip(builder, "toolbar");
            strip(builder, "config_toolbar");
        }
        "contest.ui" => {
            strip(builder, "contest_toolbar");
            strip(builder, "notebook_title");
        }
        _ => {}
    }
}
