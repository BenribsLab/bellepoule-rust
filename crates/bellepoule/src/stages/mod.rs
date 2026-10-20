//! Écrans des phases d'une compétition.
//!
//! Chaque phase a un écran qui affiche son état et reçoit les saisies ; il
//! relit le modèle (`Contest`) à chaque `refresh`.

use std::rc::Rc;

use bp_core::stage::StageKind;
use gtk4 as gtk;
use gtk::prelude::*;

use crate::contest_page::ContestPage;

pub trait StageScreen {
    /// Widget affiché dans l'onglet de la phase.
    fn widget(&self) -> gtk::Widget;
    /// Relit le modèle et met l'affichage à jour.
    fn refresh(&self);
}

/// Écran d'une phase (`index` : position dans la suite des phases).
pub fn build(page: &Rc<ContestPage>, index: usize) -> Rc<dyn StageScreen> {
    let kind = page.with_contest(|c| c.schedule.stage(index).map(|s| s.kind())).flatten();
    match kind {
        _ => Rc::new(Pending::new(page, index)),
    }
}

/// Écran provisoire d'une phase dont l'interface n'est pas encore portée.
struct Pending {
    label: gtk::Label,
}

impl Pending {
    fn new(page: &Rc<ContestPage>, index: usize) -> Pending {
        let name = page
            .with_contest(|c| c.schedule.stage(index).map(|s| bp_core::i18n::tr(s.kind().class_name())))
            .flatten()
            .unwrap_or_default();
        let label = gtk::Label::new(Some(&name));
        label.set_vexpand(true);
        label.set_hexpand(true);
        Pending { label }
    }
}

impl StageScreen for Pending {
    fn widget(&self) -> gtk::Widget {
        self.label.clone().upcast()
    }

    fn refresh(&self) {}
}

#[allow(dead_code)]
fn _kinds(_: StageKind) {}
