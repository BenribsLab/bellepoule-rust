//! Disposition compacte pour l'organisation intégrée à BellePoule.
use bp_core::i18n::tr;
use bp_gtk::glade::Glade;
use gtk4::{self as gtk, prelude::*};

pub fn button(button: &gtk::Button, icon: &str, label: &str) {
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    let image = gtk::Image::from_icon_name(icon);
    image.set_pixel_size(16);
    content.append(&image);
    content.append(&gtk::Label::new(Some(&tr(label))));
    button.set_child(Some(&content));
    button.set_vexpand(false);
    button.set_hexpand(false);
    button.set_valign(gtk::Align::Center);
}

pub fn referees(glade: &Glade) {
    let header: gtk::Widget = glade.get("tab_header");
    header.set_halign(gtk::Align::Start);
    header.set_hexpand(false);
    for id in ["attending_tab", "absent_tab"] {
        glade.get::<gtk::Widget>(id).set_hexpand(false);
    }
    let panel: gtk::Box = glade.get("edit_panel");
    panel.set_orientation(gtk::Orientation::Vertical);
    panel.set_vexpand(false);
    // Conserver les objets et leurs signaux, mais retirer les conteneurs GTK2
    // dont les propriétés d'expansion étiraient les boutons sur toute la page.
    let stats = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    for (id, icon, label, counter) in [
        (
            "all_present_button",
            "object-select-symbolic",
            "Presents",
            "attending_label",
        ),
        (
            "all_absent_button",
            "edit-clear-symbolic",
            "Absents",
            "absent_label",
        ),
    ] {
        let control: gtk::Button = glade.get(id);
        Glade::detach_from_parent(&control);
        button(&control, icon, label);
        let count: gtk::Label = glade.get(counter);
        Glade::detach_from_parent(&count);
        count.set_vexpand(false);
        count.set_hexpand(false);
        count.set_width_chars(2);
        stats.append(&control);
        stats.append(&count);
    }
    let actions = gtk::FlowBox::new();
    actions.set_selection_mode(gtk::SelectionMode::None);
    actions.set_min_children_per_line(1);
    actions.set_max_children_per_line(3);
    actions.set_column_spacing(2);
    for (id, icon, label) in [
        ("add_player_button", "list-add-symbolic", "Add"),
        ("remove_player_button", "list-remove-symbolic", "Remove"),
        ("import_toolbutton", "document-open-symbolic", "Import"),
    ] {
        let control: gtk::Button = glade.get(id);
        Glade::detach_from_parent(&control);
        button(&control, icon, label);
        actions.insert(&control, -1);
    }
    while let Some(child) = panel.first_child() {
        panel.remove(&child);
    }
    panel.set_spacing(4);
    panel.append(&stats);
    panel.append(&actions);
    let scroller: gtk::ScrolledWindow = glade.get("scrolledwindow1");
    scroller.set_propagate_natural_width(false);
    scroller.set_min_content_width(180);
}

pub fn organisation(glade: &Glade) {
    let outer: gtk::Paned = glade.get("hpaned");
    outer.set_wide_handle(true);
    outer.set_shrink_start_child(false);
    outer.set_shrink_end_child(false);
    outer.set_resize_start_child(false);
    outer.set_resize_end_child(true);
    let hall: gtk::Widget = glade.get("hall_viewport");
    let matches: gtk::Widget = glade.get("batch_vbox");
    Glade::detach_from_parent(&hall);
    Glade::detach_from_parent(&matches);
    let split = gtk::Paned::new(gtk::Orientation::Vertical);
    split.set_wide_handle(true);
    split.set_shrink_start_child(false);
    split.set_shrink_end_child(false);
    // La liste des matchs à placer garde une vraie place sous la salle.
    split.set_resize_start_child(true);
    split.set_resize_end_child(true);
    split.set_position(300);
    split.set_start_child(Some(&hall));
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_min_content_height(240);
    scroll.set_min_content_width(240);
    scroll.set_child(Some(&matches));
    split.set_end_child(Some(&scroll));
    outer.set_end_child(Some(&split));
    let notebook: gtk::Notebook = glade.get("batch_notebook");
    notebook.set_tab_pos(gtk::PositionType::Top);
    notebook.set_scrollable(true);
    glade
        .get::<gtk::Notebook>("referee_notebook")
        .set_scrollable(true);
}

pub fn hall(glade: &Glade) {
    // Petites commandes sur une ligne ; les alertes et l'horloge peuvent
    // revenir à la ligne quand la fenêtre est étroite.
    for (id, icon, label) in [
        ("add_piste_button", "list-add-symbolic", "Add"),
        ("remove_piste_button", "list-remove-symbolic", "Remove"),
        (
            "rotate_piste_button",
            "object-rotate-right-symbolic",
            "Rotate",
        ),
        ("cone_togglebutton", "changes-prevent-symbolic", "Block"),
        ("print_toolbutton", "document-print-symbolic", "Print"),
    ] {
        if let Some(control) = glade.object::<gtk::Button>(id) {
            button(&control, icon, label);
        }
    }
    let toolbar: gtk::Box = glade.get("toolbar");
    let flow = gtk::FlowBox::new();
    flow.set_selection_mode(gtk::SelectionMode::None);
    flow.set_min_children_per_line(1);
    flow.set_max_children_per_line(6);
    flow.set_row_spacing(2);
    flow.set_column_spacing(2);
    let tools: gtk::Box = glade.get("toolbar2");
    while let Some(child) = tools.first_child() {
        tools.remove(&child);
        if child.is::<gtk::Separator>() {
            continue;
        }
        child.set_vexpand(false);
        child.set_hexpand(false);
        flow.insert(&child, -1);
    }
    toolbar.remove(&tools);
    while let Some(child) = toolbar.first_child() {
        toolbar.remove(&child);
        child.set_vexpand(false);
        flow.insert(&child, -1);
    }
    toolbar.append(&flow);
    toolbar.set_vexpand(false);
    glade.get::<gtk::Widget>("clock_button").set_vexpand(false);
}
