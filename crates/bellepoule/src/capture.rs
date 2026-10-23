//! Captures d'écran de la documentation (`--doc-capture`) : ouvre une
//! compétition, affiche une vue, enregistre l'image de la fenêtre telle que
//! GTK la dessine, puis quitte. Les impressions (feuilles de match par
//! équipes, pages des groupes) sont rendues directement en image.
//!
//! ```text
//! managecomp --doc-capture sortie.png --view stage --stage poules --size 1400x900 fichier.cotcot
//! ```
//!
//! Vues : `home`, `stage` (avec `--stage appel|composition|poules|tableau|
//! jeunes|classement`), `organisation`, `properties`, `print-relay`,
//! `print-groups`.

use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use bp_core::stage::StageKind;
use gtk4::{self as gtk, glib, prelude::*};

use crate::app::App;

#[derive(Debug, Clone)]
pub struct Shot {
    pub out: PathBuf,
    pub view: String,
    pub stage: Option<String>,
    pub size: (i32, i32),
}

impl Shot {
    /// Lit les options `--doc-capture`, `--view`, `--stage`, `--size`.
    pub fn from_args(args: &[String]) -> Option<Shot> {
        let value = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned();
        let out = PathBuf::from(value("--doc-capture")?);
        let size = value("--size")
            .and_then(|s| s.split_once('x').and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?))))
            .unwrap_or((1400, 900));
        Some(Shot { out, view: value("--view").unwrap_or_else(|| "stage".into()), stage: value("--stage"), size })
    }

    /// Options à ne pas prendre pour un fichier à ouvrir.
    pub fn takes_value(flag: &str) -> bool {
        matches!(flag, "--doc-capture" | "--view" | "--stage" | "--size")
    }
}

fn stage_kinds(name: &str) -> &'static [StageKind] {
    match name {
        "appel" => &[StageKind::Checkin],
        "composition" => &[StageKind::PoolAllocator],
        "poules" => &[StageKind::PoolSupervisor],
        "tableau" => &[StageKind::TableSupervisor],
        "jeunes" => &[StageKind::YouthTraining],
        "classement" => &[StageKind::GeneralClassification],
        _ => &[],
    }
}

/// Image d'une fenêtre, dessinée par son propre moteur de rendu.
fn save_window(window: &gtk::Window, path: &std::path::Path) -> Result<(), String> {
    let (w, h) = (window.width(), window.height());
    let paintable = gtk::WidgetPaintable::new(Some(window));
    let snapshot = gtk::Snapshot::new();
    paintable.snapshot(&snapshot, w as f64, h as f64);
    let node = snapshot.to_node().ok_or("fenêtre vide")?;
    let renderer = window.renderer().ok_or("pas de moteur de rendu")?;
    let viewport = gtk::graphene::Rect::new(0.0, 0.0, w as f32, h as f32);
    let texture = renderer.render_texture(&node, Some(&viewport));
    texture.save_to_png(path).map_err(|e| e.to_string())
}

/// Dernière fenêtre ouverte (dialogue), sinon la fenêtre principale.
fn top_window(app: &App) -> gtk::Window {
    gtk::Window::list_toplevels()
        .into_iter()
        .filter_map(|w| w.downcast::<gtk::Window>().ok())
        .filter(|w| w.is_visible() && *w != app.window)
        .last()
        .unwrap_or_else(|| app.window.clone())
}

/// Prépare la vue, attend que GTK l'ait dessinée, capture et quitte.
pub fn run(app: &Rc<App>, shot: Shot) {
    app.window.set_default_size(shot.size.0, shot.size.1);
    app.window.set_size_request(shot.size.0, shot.size.1);
    let app = app.clone();
    glib::timeout_add_local_once(Duration::from_millis(1500), move || {
        let page = app.pages.borrow().last().cloned();
        let printed = matches!(shot.view.as_str(), "print-relay" | "print-groups");
        // La capture est programmée avant d'afficher la vue : un dialogue
        // modal (propriétés) bloque jusqu'à sa fermeture.
        if !printed {
            let app2 = app.clone();
            let out = shot.out.clone();
            glib::timeout_add_local_once(Duration::from_millis(1500), move || {
                let window = top_window(&app2);
                match save_window(&window, &out) {
                    Ok(()) => println!("capture : {}", out.display()),
                    Err(e) => eprintln!("capture impossible : {e}"),
                }
                std::process::exit(0);
            });
        }
        match shot.view.as_str() {
            "stage" => {
                if let (Some(page), Some(stage)) = (&page, &shot.stage) {
                    if let Some(view) = page.schedule.borrow().clone() {
                        view.show_stage(stage_kinds(stage));
                    }
                }
            }
            "organisation" => app.open_management(),
            "properties" => {
                if let Some(page) = &page {
                    // Formule : préférences de la phase demandée, une fois le dialogue affiché.
                    if let (Some(stage), Some(view)) = (shot.stage.clone(), page.schedule.borrow().clone()) {
                        let page = page.clone();
                        glib::timeout_add_local_once(Duration::from_millis(500), move || {
                            page.select_properties_tab(1);
                            if let Some(kind) = stage_kinds(&stage).first() {
                                view.select_stage_kind(*kind);
                            }
                        });
                    }
                    page.show_properties();
                }
            }
            "print-relay" | "print-groups" => {
                if let Some(page) = &page {
                    let source = if shot.view == "print-relay" {
                        let table = page
                            .with_contest(|c| (0..c.schedule.len()).find(|i| c.schedule.stage(*i).is_some_and(|s| s.kind() == StageKind::TableSupervisor)))
                            .flatten();
                        table.and_then(|i| crate::stages::relay_print::source(page, i))
                    } else {
                        let pools = page.with_contest(|c| c.youth_pool_stage()).flatten();
                        pools.and_then(|i| crate::stages::group_print::source(page, i))
                    };
                    if let Some(mut source) = source {
                        let header = if shot.view == "print-groups" { page.print_header() } else { None };
                        match bp_gtk::print::export_png(source.as_mut(), header.as_ref(), 0, &shot.out) {
                            Ok(()) => println!("capture : {}", shot.out.display()),
                            Err(e) => eprintln!("capture impossible : {e}"),
                        }
                    }
                }
                std::process::exit(0);
            }
            _ => {}
        }
    });
}
