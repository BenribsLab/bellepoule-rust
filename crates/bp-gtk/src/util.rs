//! Petits utilitaires : `SensitivityTrigger`, `Timekeeper`, `TreeModelIndex`
//! (ports de `common/util/{sensitivity_trigger,timekeeper,tree_model_index}.cpp`).

#![allow(deprecated)]

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::Hash;
use std::rc::Rc;
use std::time::Instant;

use gtk4 as gtk;
use gtk::prelude::*;

const LOCK_KEY: &str = "SensitivityTrigger::lock";

fn lock_of(w: &gtk::Widget) -> i32 {
    // SAFETY : la donnée associée à cette clé est toujours un i32.
    unsafe { w.data::<i32>(LOCK_KEY).map(|p| *p.as_ref()).unwrap_or(0) }
}

fn set_lock(w: &gtk::Widget, v: i32) {
    // SAFETY : même type (i32) pour toutes les écritures de cette clé.
    unsafe { w.set_data::<i32>(LOCK_KEY, v) }
}

/// Active ou désactive un ensemble de widgets, avec un compteur de verrous
/// par widget : un widget partagé par plusieurs déclencheurs reste inactif
/// tant qu'un seul l'a désactivé.
#[derive(Default, Clone)]
pub struct SensitivityTrigger {
    widgets: Rc<RefCell<Vec<gtk::Widget>>>,
}

impl SensitivityTrigger {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_widget(&self, w: &impl IsA<gtk::Widget>) {
        let w = w.as_ref().clone();
        w.connect_realize(|w| {
            w.set_sensitive(lock_of(w) <= 0);
        });
        self.widgets.borrow_mut().push(w);
    }

    pub fn switch_on(&self) {
        for w in self.widgets.borrow().iter() {
            let lock = lock_of(w) - 1;
            set_lock(w, lock);
            if lock == 0 {
                w.set_sensitive(true);
            }
        }
    }

    pub fn switch_off(&self) {
        for w in self.widgets.borrow().iter() {
            let lock = lock_of(w) + 1;
            set_lock(w, lock);
            if lock == 1 {
                w.set_sensitive(false);
            }
        }
    }
}

/// Chronomètre de mise au point (`Timekeeper::PrintElapsed`).
pub struct Timekeeper {
    last: Instant,
}

impl Default for Timekeeper {
    fn default() -> Self {
        Timekeeper { last: Instant::now() }
    }
}

impl Timekeeper {
    pub fn new() -> Self {
        Self::default()
    }

    /// Affiche le temps écoulé depuis l'appel précédent ; le renvoie en µs.
    pub fn print_elapsed(&mut self, tag: &str) -> u128 {
        let now = Instant::now();
        let d = now - self.last;
        let us = d.as_micros();
        println!("\x1b[34m{tag} \x1b[0m{us} ({}/{})", d.as_secs(), d.subsec_micros());
        self.last = now;
        us
    }
}

/// Index clé -> ligne d'un `GtkTreeModel` (`TreeModelIndex`) : les
/// références de lignes suivent les insertions et suppressions.
pub struct TreeModelIndex<K: Eq + Hash> {
    model: gtk::TreeModel,
    rows: HashMap<K, gtk::TreeRowReference>,
}

impl<K: Eq + Hash> TreeModelIndex<K> {
    pub fn new(model: &impl IsA<gtk::TreeModel>) -> Self {
        TreeModelIndex { model: model.as_ref().clone(), rows: HashMap::new() }
    }

    pub fn set_iter_to_key(&mut self, iter: &gtk::TreeIter, key: K) {
        let path = self.model.path(iter);
        if let Some(r) = gtk::TreeRowReference::new(&self.model, &path) {
            self.rows.insert(key, r);
        }
    }

    pub fn iter_from_key(&self, key: &K) -> Option<gtk::TreeIter> {
        let r = self.rows.get(key)?;
        let path = r.path()?;
        self.model.iter(&path)
    }

    pub fn remove(&mut self, key: &K) {
        self.rows.remove(key);
    }

    pub fn clear(&mut self) {
        self.rows.clear();
    }
}

/// La molette ne change pas la valeur d'une liste déroulante : elle fait
/// défiler la page qui la contient. On ne change la valeur qu'en cliquant.
pub fn scroll_passes_through(widget: &impl IsA<gtk::Widget>) {
    let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::BOTH_AXES);
    scroll.set_propagation_phase(gtk::PropagationPhase::Capture);
    let target = widget.as_ref().downgrade();
    scroll.connect_scroll(move |_, dx, dy| {
        if let Some(w) = target.upgrade() {
            if let Some(sw) = w.ancestor(gtk::ScrolledWindow::static_type()).and_downcast::<gtk::ScrolledWindow>() {
                for (adj, delta) in [(sw.vadjustment(), dy), (sw.hadjustment(), dx)] {
                    if delta != 0.0 {
                        let step = (adj.page_size() / 10.0).max(20.0);
                        adj.set_value((adj.value() + delta * step).clamp(adj.lower(), adj.upper() - adj.page_size()));
                    }
                }
            }
        }
        gtk::glib::Propagation::Stop
    });
    widget.as_ref().add_controller(scroll);
}
