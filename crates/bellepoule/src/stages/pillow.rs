//! Dialogues de présence pendant la composition des poules.
//!
//! Les modifications restent dans une copie jusqu'à OK : Annuler et une
//! formule impossible ne doivent jamais altérer la compétition sauvegardée.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use bp_core::attribute::{self, Value};
use bp_core::contest::players::new_player;
use bp_core::i18n::tr;
use bp_core::player::{AttrId, PlayerClass, PlayerRef, PlayerStore};
use bp_core::rounds::pool::round::{PillowKind, PoolRound};
use bp_gtk::{dialogs, glade::Glade};
use gtk::prelude::*;
use gtk4 as gtk;

use crate::widgets::form::{FormEvent, PlayerForm};

#[derive(Clone)]
pub struct Changes {
    pub round: PoolRound,
    pub players: PlayerStore,
    pub touched: Vec<PlayerRef>,
}

fn append_fencer(
    list: &gtk::Box,
    changes: &Rc<RefCell<Changes>>,
    warning: &gtk::Widget,
    kind: PillowKind,
    player: PlayerRef,
) {
    let data = changes.borrow();
    let Some(p) = data.players.get(player) else {
        return;
    };
    let check = gtk::CheckButton::with_label(&format!(
        "{} {}   {}",
        p.name(),
        p.first_name(),
        p.str_attr("club").unwrap_or("")
    ));
    check.set_active(p.attending());
    drop(data);
    list.append(&check);
    let updating = Cell::new(false);
    let changes = changes.clone();
    let warning = warning.clone();
    check.connect_toggled(move |check| {
        if updating.get() {
            return;
        }
        let mut data = changes.borrow_mut();
        let before_round = data.round.clone();
        let before_players = data.players.clone();
        let nb_pools = data.round.nb_pools() as u32;
        let Changes { round, players, .. } = &mut *data;
        let accepted =
            round.on_attendee_toggled(players, kind, player, check.is_active(), nb_pools);
        if accepted {
            if !data.touched.contains(&player) {
                data.touched.push(player);
            }
        } else {
            data.round = before_round;
            data.players = before_players;
            updating.set(true);
            check.set_active(!check.is_active());
            updating.set(false);
        }
        warning.set_visible(!accepted);
    });
}

pub fn run(
    glade: &Glade,
    parent: Option<&gtk::Window>,
    round: PoolRound,
    players: PlayerStore,
    candidates: Vec<PlayerRef>,
    attributes: Vec<String>,
    kind: PillowKind,
) -> Option<Changes> {
    let prefix = if kind == PillowKind::Latecomer {
        "latecomer"
    } else {
        "absent"
    };
    let dialog: gtk::MessageDialog = glade.get(&format!("{prefix}_dialog"));
    dialog.set_transient_for(parent);
    let viewport: gtk::Viewport = glade.get(&format!("{prefix}_list"));
    let list = gtk::Box::new(gtk::Orientation::Vertical, 4);
    viewport.set_child(Some(&list));
    let warning: gtk::Widget = glade.get(&format!("{prefix}_warning_box"));
    let label: gtk::Label = glade.get(&format!("{prefix}_warning_label"));
    label.set_text(&tr("Impossible to apply with this configuration of pools"));
    warning.set_visible(false);
    let changes = Rc::new(RefCell::new(Changes {
        round,
        players,
        touched: Vec::new(),
    }));
    let mut candidates = candidates;
    candidates.sort_by_key(|r| {
        changes
            .borrow()
            .players
            .get(*r)
            .map(|p| (p.name(), p.first_name()))
            .unwrap_or_default()
    });
    for r in candidates {
        append_fencer(&list, &changes, &warning, kind, r);
    }

    let form = PlayerForm::new().expect("form.ui");
    form.add_page(PlayerClass::Fencer, "Fencer", &attributes);
    form.set_close_on_add(true);
    let new_button: gtk::Button = glade.get("new_latecomer");
    let new_handler = {
        let form = form.clone();
        let parent: gtk::Window = dialog.clone().upcast();
        new_button.connect_clicked(move |_| form.show(Some(&parent), None))
    };
    {
        let changes = changes.clone();
        let list = list.clone();
        let warning = warning.clone();
        form.connect_event(move |event| {
            if let FormEvent::New(class, values) = event {
                let mut player = new_player(class);
                for (code, value) in values {
                    if let (Some(desc), Some(v)) = (attribute::registry().by_code(&code), value) {
                        player.set(&AttrId::global(&code), Value::parse(desc.ty, &v));
                    }
                }
                // Le nouveau venu est proposé à la sélection, comme les absents.
                player.set(&AttrId::global("attending"), Value::Bool(false));
                let r = {
                    let mut data = changes.borrow_mut();
                    let r = data.players.add(player);
                    data.touched.push(r);
                    r
                };
                append_fencer(&list, &changes, &warning, kind, r);
            }
        });
    }
    let accepted = dialogs::run_dialog(&dialog) == gtk::ResponseType::Ok;
    dialog.hide();
    form.window().destroy();
    new_button.disconnect(new_handler);
    viewport.set_child(gtk::Widget::NONE);
    if accepted {
        Some(changes.borrow().clone())
    } else {
        None
    }
}
