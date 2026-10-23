//! Composants graphiques partagés par les écrans des phases.

pub mod filter_dialog;
pub mod form;
pub mod players_list;
pub mod render;
pub mod list_document;
pub mod score_sheet;
pub mod bout_timing;

use gtk4 as gtk;
use gtk::prelude::*;

/// Un `GtkEntry` délègue le focus à son `GtkText` interne : `has_focus()` de
/// l'entrée est donc toujours faux.
pub fn entry_has_focus(entry: &gtk::Entry) -> bool {
    entry.has_focus()
        || entry.delegate().and_then(|d| d.dynamic_cast::<gtk::Widget>().ok()).map(|w| w.has_focus()).unwrap_or(false)
}

/// Donne le focus à une entrée fraîchement créée : tant qu'elle n'est pas
/// affichée, `grab_focus` échoue, on réessaie donc brièvement.
pub fn focus_entry_when_ready(entry: &gtk::Entry, select_all: bool, cursor: i32) {
    fn attempt(entry: gtk::Entry, tries: u32, select_all: bool, cursor: i32) {
        if entry.is_mapped() && entry.grab_focus() {
            if select_all {
                entry.select_region(0, -1);
            } else {
                entry.set_position(cursor);
            }
        } else if tries > 0 {
            gtk::glib::timeout_add_local_once(std::time::Duration::from_millis(15), move || {
                attempt(entry, tries - 1, select_all, cursor)
            });
        }
    }
    attempt(entry.clone(), 20, select_all, cursor);
}

/// Style des feuilles de poule : cases contournées, cases grisées (un
/// tireur ne tire pas contre lui-même).
pub fn install_sheet_css() {
    thread_local! {
        static DONE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }
    if DONE.with(|d| d.replace(true)) {
        return;
    }
    let Some(display) = gtk::gdk::Display::default() else { return };
    let css = gtk::CssProvider::new();
    css.load_from_data(
        ".bp-cell { border: 1px solid #6e6e6e; padding: 3px 8px; margin: 0 -1px -1px 0; background: #ffffff; color: #000000; }\n\
         .bp-cell entry { border: none; box-shadow: none; outline: none; background: transparent; min-height: 0; padding: 0 2px; }\n\
         .bp-cell entry.error { background: #f4b6b6; }\n\
         .bp-cell.bp-head { background: #ececec; font-weight: bold; }\n\
         .bp-cell.bp-grey { background: #b4b4b4; }\n",
    );
    gtk::style_context_add_provider_for_display(&display, &css, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
}
