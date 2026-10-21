//! Dialogues communs.
//!
//! GTK4 a supprimé `gtk_dialog_run` ; le code de BellePoule en dépend
//! beaucoup (dialogues modaux dont on lit la réponse). [`run_dialog`] le
//! réimplémente avec une boucle principale imbriquée, exactement comme le
//! faisait GTK2.

#![allow(deprecated)]

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk4 as gtk;
use gtk::gio;
use gtk::glib;
use gtk::prelude::*;

use bp_core::i18n::tr;

thread_local! {
    static MAIN_WINDOW: RefCell<Option<glib::WeakRef<gtk::Window>>> = const { RefCell::new(None) };
}

/// Mémorise la fenêtre principale (`Module::_main_window`).
pub fn set_main_window(window: &impl IsA<gtk::Window>) {
    let weak = window.as_ref().downgrade();
    MAIN_WINDOW.with(|m| *m.borrow_mut() = Some(weak));
}

/// Sous Windows, fermer ou masquer une fenêtre secondaire rend la main à la
/// fenêtre d'une autre application : BellePoule semble planté. Chaque
/// fenêtre de l'application, dès qu'elle apparaît, ramène donc au premier
/// plan sa fenêtre parente (ou la fenêtre principale) quand elle disparaît.
pub fn keep_focus_in_app() {
    let toplevels = gtk::Window::toplevels();
    let watch = |model: &gio::ListModel, from: u32, count: u32| {
        for i in from..from + count {
            let Some(window) = model.item(i).and_downcast::<gtk::Window>() else { continue };
            if unsafe { window.data::<bool>("bp-focus-watch").is_some() } {
                continue;
            }
            unsafe { window.set_data("bp-focus-watch", true) };
            // `hide` pour une fenêtre masquée, `unmap` pour une fenêtre
            // détruite (aperçu d'impression, dialogues créés par le code).
            window.connect_hide(|w| refocus_after(w));
            window.connect_unmap(|w| refocus_after(w));
        }
    };
    watch(&toplevels, 0, toplevels.n_items());
    toplevels.connect_items_changed(move |model, position, _removed, added| watch(model, position, added));
}

/// Fenêtre à remettre au premier plan quand `w` disparaît : sa parente
/// visible, sinon la fenêtre principale.
fn focus_target(w: &gtk::Window) -> Option<gtk::Window> {
    let parent = w.transient_for().filter(|p| p.is_visible());
    parent.or_else(main_window).filter(|t| t != w && t.is_visible())
}

/// Remet la parente au premier plan **avant** que `w` ne disparaisse :
/// BellePoule a encore la main, Windows accepte donc le changement de
/// fenêtre active (après coup, il peut le refuser et activer une autre
/// application).
pub fn hand_focus_back(w: &impl IsA<gtk::Window>) {
    if let Some(target) = focus_target(w.as_ref()) {
        target.present();
    }
}

/// Après la disparition d'une fenêtre : seconde tentative, au cas où la
/// fenêtre n'est pas passée par `run_dialog`.
fn refocus_after(w: &gtk::Window) {
    let Some(target) = focus_target(w) else { return };
    // Après la fin du traitement en cours (destruction…).
    glib::idle_add_local_once(move || {
        // Le modèle GTK peut déclarer GObject comme type d'élément, même si
        // ses éléments sont des fenêtres : iter::<Window>() paniquerait.
        let windows = gtk::Window::toplevels();
        let other_active = (0..windows.n_items()).any(|i| {
            windows
                .item(i)
                .and_downcast::<gtk::Window>()
                .is_some_and(|o| o != target && o.is_visible() && o.is_active())
        });
        if target.is_visible() && !other_active {
            target.present();
        }
    });
}

/// Fenêtre principale, si elle existe encore.
pub fn main_window() -> Option<gtk::Window> {
    MAIN_WINDOW.with(|m| m.borrow().as_ref().and_then(|w| w.upgrade()))
}

/// `gtk_dialog_run` : affiche le dialogue en modal et attend sa réponse.
pub fn run_dialog(dialog: &impl IsA<gtk::Dialog>) -> gtk::ResponseType {
    let dialog = dialog.as_ref();
    if dialog.transient_for().is_none() {
        if let Some(main) = main_window() {
            if main.upcast_ref::<gtk::Window>() != dialog.upcast_ref::<gtk::Window>() {
                dialog.set_transient_for(Some(&main));
            }
        }
    }
    let was_modal = dialog.is_modal();
    dialog.set_modal(true);

    let main_loop = glib::MainLoop::new(None, false);
    let response = Rc::new(Cell::new(gtk::ResponseType::None));

    let h1 = {
        let main_loop = main_loop.clone();
        let response = response.clone();
        dialog.connect_response(move |_, r| {
            response.set(r);
            if main_loop.is_running() {
                main_loop.quit();
            }
        })
    };
    let h2 = {
        let main_loop = main_loop.clone();
        let response = response.clone();
        dialog.connect_close_request(move |_| {
            response.set(gtk::ResponseType::DeleteEvent);
            if main_loop.is_running() {
                main_loop.quit();
            }
            glib::Propagation::Stop
        })
    };
    let h3 = {
        let main_loop = main_loop.clone();
        dialog.connect_destroy(move |_| {
            if main_loop.is_running() {
                main_loop.quit();
            }
        })
    };

    dialog.present();
    main_loop.run();

    // La parente reprend la main avant que l'appelant masque ou détruise
    // le dialogue.
    hand_focus_back(dialog);
    dialog.disconnect(h1);
    dialog.disconnect(h2);
    dialog.disconnect(h3);
    dialog.set_modal(was_modal);
    response.get()
}

/// `Module::Raise` : présente un dialogue (non bloquant) au-dessus de `over`.
pub fn raise(window: &impl IsA<gtk::Window>, over: Option<&gtk::Window>) {
    let window = window.as_ref();
    match over {
        None => {
            if let Some(main) = main_window() {
                if &main != window {
                    window.set_transient_for(Some(&main));
                }
            }
        }
        Some(o) => {
            window.set_modal(true);
            window.set_transient_for(Some(o));
        }
    }
    window.present();
}

fn message_dialog(
    parent: Option<&gtk::Window>,
    kind: gtk::MessageType,
    buttons: gtk::ButtonsType,
    markup: &str,
    secondary: Option<&str>,
    title: Option<&str>,
) -> gtk::MessageDialog {
    let parent = parent.cloned().or_else(main_window);
    let d = gtk::MessageDialog::builder()
        .message_type(kind)
        .buttons(buttons)
        .use_markup(true)
        .text(markup)
        .modal(true)
        .destroy_with_parent(true)
        .build();
    if let Some(p) = parent.as_ref() {
        d.set_transient_for(Some(p));
    }
    if let Some(s) = secondary {
        d.set_secondary_text(Some(s));
    }
    if let Some(t) = title {
        d.set_title(Some(t));
    }
    d
}

/// `gtk_message_dialog_new_with_markup` + `gtk_dialog_run` + destruction.
pub fn run_message(
    parent: Option<&gtk::Window>,
    kind: gtk::MessageType,
    buttons: gtk::ButtonsType,
    markup: &str,
    secondary: Option<&str>,
    title: Option<&str>,
) -> gtk::ResponseType {
    let d = message_dialog(parent, kind, buttons, markup, secondary, title);
    let r = run_dialog(&d);
    d.destroy();
    r
}

/// Question OK/Annuler : vrai si l'utilisateur a validé.
pub fn confirm(parent: Option<&gtk::Window>, markup: &str, secondary: Option<&str>, title: Option<&str>) -> bool {
    run_message(parent, gtk::MessageType::Question, gtk::ButtonsType::OkCancel, markup, secondary, title)
        == gtk::ResponseType::Ok
}

/// Question Oui/Non : vrai pour « Oui ».
pub fn ask_yes_no(parent: Option<&gtk::Window>, markup: &str, secondary: Option<&str>, title: Option<&str>) -> bool {
    run_message(parent, gtk::MessageType::Question, gtk::ButtonsType::YesNo, markup, secondary, title)
        == gtk::ResponseType::Yes
}

/// Message d'information.
pub fn info(parent: Option<&gtk::Window>, markup: &str, secondary: Option<&str>) {
    run_message(parent, gtk::MessageType::Info, gtk::ButtonsType::Ok, markup, secondary, None);
}

/// Message d'erreur non bloquant (`Module::Print` en cas d'échec).
pub fn error(parent: Option<&gtk::Window>, text: &str) {
    let d = message_dialog(parent, gtk::MessageType::Error, gtk::ButtonsType::Close, &glib::markup_escape_text(text), None, None);
    d.connect_response(|d, _| d.destroy());
    d.present();
}

/// `Object::ShowUri` : ouvre une adresse ou un fichier avec l'application
/// par défaut du système.
pub fn show_uri(uri: &str) {
    let uri = if uri.contains("://") {
        uri.to_string()
    } else {
        let p = Path::new(uri);
        let abs = if p.is_absolute() { p.to_path_buf() } else { std::env::current_dir().unwrap_or_default().join(p) };
        glib::filename_to_uri(&abs, None).map(|s| s.to_string()).unwrap_or_else(|_| uri.to_string())
    };
    let launcher = gtk::UriLauncher::new(&uri);
    launcher.launch(main_window().as_ref(), gio::Cancellable::NONE, |r| {
        if let Err(e) = r {
            log::warn!("ShowUri : {e}");
        }
    });
}

/// `Tournament::GetFileFilter` : filtre avec motifs en minuscules et
/// majuscules ; le nom affiché reprend la liste des motifs.
pub fn file_filter(name: Option<&str>, patterns: &[&str]) -> gtk::FileFilter {
    let filter = gtk::FileFilter::new();
    for p in patterns {
        filter.add_pattern(p);
        filter.add_pattern(&p.to_ascii_uppercase());
    }
    if let Some(n) = name {
        filter.set_name(Some(&format!("{n} ({})", patterns.join(", "))));
    }
    filter
}

/// Filtre « Tous les fichiers ».
pub fn all_files_filter() -> gtk::FileFilter {
    let f = gtk::FileFilter::new();
    f.set_name(Some(&tr("All files")));
    f.add_pattern("*");
    f
}

/// Paramètres d'un sélecteur de fichiers.
pub struct FileChooser<'a> {
    pub title: &'a str,
    pub action: gtk::FileChooserAction,
    pub filters: Vec<gtk::FileFilter>,
    pub current_folder: Option<PathBuf>,
    pub current_name: Option<String>,
    /// Boutons (libellé, réponse) dans l'ordre d'affichage.
    pub buttons: Vec<(String, gtk::ResponseType)>,
}

impl<'a> FileChooser<'a> {
    pub fn new(title: &'a str, action: gtk::FileChooserAction) -> Self {
        let accept = match action {
            gtk::FileChooserAction::Save => tr("_Save"),
            gtk::FileChooserAction::SelectFolder => tr("_Apply"),
            _ => tr("_Open"),
        };
        FileChooser {
            title,
            action,
            filters: Vec::new(),
            current_folder: None,
            current_name: None,
            buttons: vec![(tr("_Cancel"), gtk::ResponseType::Cancel), (accept, gtk::ResponseType::Accept)],
        }
    }

    /// Affiche le dialogue ; renvoie la réponse et le fichier choisi.
    pub fn run(self) -> (gtk::ResponseType, Option<PathBuf>) {
        let buttons: Vec<(&str, gtk::ResponseType)> = self.buttons.iter().map(|(l, r)| (l.as_str(), *r)).collect();
        let d = gtk::FileChooserDialog::new(Some(self.title), main_window().as_ref(), self.action, &buttons);
        for f in &self.filters {
            d.add_filter(f);
        }
        if let Some(dir) = self.current_folder.as_ref().filter(|d| d.is_dir()) {
            let _ = d.set_current_folder(Some(&gio::File::for_path(dir)));
        }
        if let Some(n) = &self.current_name {
            d.set_current_name(n);
        }
        let r = run_dialog(&d);
        let file = d.file().and_then(|f| f.path());
        d.destroy();
        (r, file)
    }
}

/// Fait tourner la boucle principale jusqu'à épuisement des événements
/// (`while (gtk_events_pending ()) gtk_main_iteration ()`).
pub fn flush_events() {
    let ctx = glib::MainContext::default();
    while ctx.pending() {
        ctx.iteration(false);
    }
}
