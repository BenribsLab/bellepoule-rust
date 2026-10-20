//! Dialogue « Données à afficher » (port de `Filter::SelectAttributes`) :
//! choix des colonnes, de leur aspect et de leur ordre.

use bp_core::attribute::{registry, Look};
use bp_core::filter::{look_image, Filter};
use bp_core::i18n::tr;
use bp_core::tournament::user_config::UserConfig;
use bp_gtk::dialogs;
use gtk4 as gtk;
use gtk::prelude::*;

const LOOKS: [Look; 3] = [Look::LongText, Look::ShortText, Look::Graphical];

/// Affiche le dialogue ; renvoie vrai si le filtre a changé.
pub fn select_attributes(parent: Option<&gtk::Window>, filter: &mut Filter, config: Option<&mut UserConfig>) -> bool {
    let dialog = gtk::Dialog::with_buttons(
        Some(&tr("Data to display")),
        parent,
        gtk::DialogFlags::MODAL | gtk::DialogFlags::DESTROY_WITH_PARENT,
        &[(&tr("_Close"), gtk::ResponseType::None)],
    );
    // code, visible, nom, aspect (texte), indice d'aspect
    let store = gtk::ListStore::new(&[
        gtk::glib::Type::STRING,
        gtk::glib::Type::BOOL,
        gtk::glib::Type::STRING,
        gtk::glib::Type::STRING,
        gtk::glib::Type::U32,
    ]);
    for e in filter.entries() {
        let name = registry().by_code(&e.code_name).map(|d| tr(d.user_name)).unwrap_or_else(|| e.code_name.clone());
        let look_index = LOOKS.iter().position(|l| *l == e.look).unwrap_or(0) as u32;
        store.set(&store.append(), &[(0, &e.code_name), (1, &e.visible), (2, &name), (3, &tr(look_image(e.look))), (4, &look_index)]);
    }

    let view = gtk::TreeView::with_model(&store);
    view.set_reorderable(true);

    let toggle = gtk::CellRendererToggle::new();
    toggle.set_activatable(true);
    {
        let store = store.clone();
        toggle.connect_toggled(move |_, path| {
            if let Some(iter) = store.iter(&path) {
                let v: bool = store.get(&iter, 1);
                store.set_value(&iter, 1, &(!v).to_value());
            }
        });
    }
    view.append_column(&gtk::TreeViewColumn::with_attributes(&tr("Visibility"), &toggle, &[("active", 1)]));
    view.append_column(&gtk::TreeViewColumn::with_attributes(&tr("Data"), &gtk::CellRendererText::new(), &[("text", 2)]));

    let looks = gtk::ListStore::new(&[gtk::glib::Type::STRING]);
    for l in LOOKS {
        looks.set(&looks.append(), &[(0, &tr(look_image(l)))]);
    }
    let combo = gtk::CellRendererCombo::new();
    combo.set_model(Some(&looks));
    combo.set_text_column(0);
    combo.set_editable(true);
    combo.set_has_entry(false);
    {
        let store = store.clone();
        let looks = looks.clone();
        combo.connect_changed(move |_, path, new_iter| {
            let text: String = looks.get(new_iter, 0);
            let index = looks.path(new_iter).indices().first().copied().unwrap_or(0) as u32;
            if let Some(iter) = store.iter(&path) {
                store.set(&iter, &[(3, &text), (4, &index)]);
            }
        });
    }
    view.append_column(&gtk::TreeViewColumn::with_attributes(&tr("Look"), &combo, &[("text", 3)]));

    let scroll = gtk::ScrolledWindow::new();
    scroll.set_child(Some(&view));
    scroll.set_min_content_height(380);
    scroll.set_min_content_width(380);
    dialog.content_area().append(&scroll);
    dialogs::run_dialog(&dialog);

    // Nouvel ordre, visibilités et aspects
    let mut order: Vec<(String, bool, Look)> = Vec::new();
    store.foreach(|_, _, iter| {
        let code: String = store.get(iter, 0);
        let visible: bool = store.get(iter, 1);
        let look = LOOKS[store.get::<u32>(iter, 4) as usize % 3];
        order.push((code, visible, look));
        false
    });
    dialog.destroy();

    let before = filter.clone();
    for (target, (code, visible, look)) in order.iter().enumerate() {
        if let Some(from) = filter.entries().iter().position(|e| &e.code_name == code) {
            if from != target {
                filter.move_entry(from, target, None);
            }
            let index = target;
            filter.set_visibility(index, *visible, None);
            filter.set_look(index, *look, None);
        }
    }
    filter.update_attr_list(config);
    *filter != before
}
