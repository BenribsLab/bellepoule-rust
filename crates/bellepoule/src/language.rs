//! Langue de l'interface (port de `application/language.cpp`).
//!
//! La langue vient des préférences (`Tournament/interface_language`), sinon
//! de la langue du système. Un changement n'est pris en compte qu'au
//! redémarrage, comme dans le C++.

use bp_core::tournament::user_config::UserConfig;

/// Une langue proposée (`resources/translations/index.txt`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Language {
    /// Code (`fr`, `pt_BR`…).
    pub code: String,
    /// Nom dans la langue elle-même.
    pub name: String,
}

/// Langues proposées, dans l'ordre du fichier.
pub fn available() -> Vec<Language> {
    let path = bp_gtk::resources::resource_path("translations/index.txt");
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            let (code, name) = l.trim().split_once(';')?;
            Some(Language { code: code.to_string(), name: name.trim().to_string() })
        })
        .collect()
}

/// Langue du système (`fr` pour `fr_FR.UTF-8`), « en » si inconnue.
pub fn system_language() -> String {
    for name in gtk4::glib::language_names() {
        let name = name.to_string();
        if name == "C" || name == "POSIX" {
            continue;
        }
        let base = name.split(['.', '@']).next().unwrap_or("");
        let lang = base.split('_').next().unwrap_or("");
        if !lang.is_empty() {
            return lang.to_ascii_lowercase();
        }
    }
    "en".to_string()
}

/// Langue choisie : préférences, sinon système.
pub fn current(config: &UserConfig) -> String {
    config.get_string("Tournament", "interface_language").filter(|l| !l.is_empty()).unwrap_or_else(system_language)
}

/// Active la langue (catalogue `.po`). Une langue sans catalogue laisse
/// l'interface en anglais.
pub fn activate(code: &str) {
    let dir = bp_gtk::resources::translations_dir();
    // `pt_BR` puis `pt`
    for candidate in [code, code.split('_').next().unwrap_or(code)] {
        if bp_core::i18n::set_language(&dir, candidate).is_ok() {
            return;
        }
    }
    let _ = bp_core::i18n::set_language(&dir, "en");
}

/// Mémorise le choix de l'utilisateur.
pub fn remember(config: &mut UserConfig, code: &str) {
    config.set_string("Tournament", "interface_language", code);
}

/// Remplit le menu des langues (`Language::Populate`) : une entrée à choix
/// exclusif par langue, la langue courante en titre du menu. `on_choose`
/// mémorise le choix ; il ne prend effet qu'au redémarrage.
pub fn populate_menu(glade: &bp_gtk::glade::Glade, current: &str, on_choose: impl Fn(&str) + 'static) {
    use gtk4::{gio, glib, prelude::*};

    let Some((parent, pos)) = glade.find_menu_item("locale_menuitem") else { return };
    let Some(submenu) = parent.item_link(pos, "submenu").and_then(|m| m.downcast::<gio::Menu>().ok()) else { return };

    let section = gio::Menu::new();
    for language in available() {
        let item = gio::MenuItem::new(Some(&language.name), None);
        item.set_action_and_target_value(Some("tournament.locale"), Some(&language.code.to_variant()));
        section.append_item(&item);
    }
    submenu.insert_section(0, None, &section);

    let action = gio::SimpleAction::new_stateful("locale", Some(glib::VariantTy::STRING), &current.to_variant());
    action.connect_activate(move |action, parameter| {
        if let Some(code) = parameter.and_then(|p| p.get::<String>()) {
            action.set_state(&code.to_variant());
            on_choose(&code);
        }
    });
    glade.actions().add_action(&action);

    glade.set_menu_item_label("locale_menuitem", current);
}
