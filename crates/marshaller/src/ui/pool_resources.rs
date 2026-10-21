//! Choix des ressources communes d'une poule planifiée.
use super::Ui;
use crate::slot::JobKey;
use bp_core::i18n::tr;
use bp_gtk::dialogs;
use gtk::prelude::*;
use gtk4 as gtk;
use std::{cell::Cell, rc::Rc};

pub fn edit(ui: &Rc<Ui>, job: JobKey) {
    edit_with_piste(ui, job, None);
}

pub fn edit_with_piste(ui: &Rc<Ui>, job: JobKey, added_piste: Option<crate::slot::PisteKey>) {
    let snapshot = {
        let hall = ui.hall.borrow();
        hall.jobs.get(&job).filter(|j| j.is_pool).and_then(|j| {
            let slot = hall.slots.get(&j.slot?)?;
            let pistes: Vec<_> = hall
                .pistes
                .iter()
                .filter(|p| ui.management_features().pistes || slot.pistes().contains(&p.key))
                .map(|p| {
                    (
                        p.key,
                        p.id,
                        slot.pistes().contains(&p.key) || added_piste == Some(p.key),
                    )
                })
                .collect();
            let mut referees: Vec<_> = hall
                .referees
                .referees
                .iter()
                .filter(|(key, _)| hall.pool_referee_available(job, **key))
                .map(|(key, r)| {
                    (
                        *key,
                        format!("{} {}", r.player.name(), r.player.first_name()),
                        slot.referees.contains(key),
                    )
                })
                .collect();
            referees.sort_by(|a, b| a.1.cmp(&b.1));
            let mapping: Vec<_> = slot
                .pistes()
                .into_iter()
                .zip(slot.referees.iter().copied())
                .filter(|_| slot.pistes().len() <= 1 || slot.pistes().len() == slot.referees.len())
                .collect();
            Some((j.name.clone(), pistes, referees, mapping))
        })
    };
    let Some((name, pistes, referees, mapping)) = snapshot else {
        return;
    };
    let dialog = gtk::Dialog::with_buttons(
        Some(&tr("Pool resources")),
        Some(ui.window()),
        gtk::DialogFlags::MODAL,
        &[
            (&tr("_Cancel"), gtk::ResponseType::Cancel),
            (&tr("_OK"), gtk::ResponseType::Ok),
        ],
    );
    dialog.set_default_size(520, 500);
    let content = dialog.content_area();
    content.set_spacing(8);
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    content.set_margin_start(12);
    content.set_margin_end(12);
    content.append(&gtk::Label::new(Some(&name)));
    let hint = gtk::Label::new(Some(&tr("Select the pistes for this pool. For multiple pistes, choose a different referee beside each selected piste. Matches are scheduled in waves without simultaneous bouts for a fencer.")));
    hint.set_wrap(true);
    content.append(&hint);
    let columns = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    columns.set_vexpand(true);
    let left = gtk::Box::new(gtk::Orientation::Vertical, 4);
    let right = gtk::Box::new(gtk::Orientation::Vertical, 4);
    left.append(&gtk::Label::new(Some(&tr("Pistes"))));
    right.append(&gtk::Label::new(Some(&tr("Referees (single piste)"))));
    let piste_buttons: Vec<_> = pistes
        .into_iter()
        .map(|(key, number, active)| {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            let button =
                gtk::CheckButton::with_label(&tr("Piste %d").replace("%d", &number.to_string()));
            button.set_active(active);
            button.set_sensitive(ui.management_features().pistes);
            let referee = gtk::ComboBoxText::new();
            referee.append(Some(""), &tr("Select a referee"));
            for (r, name, _) in &referees {
                referee.append(Some(&r.to_string()), name);
            }
            referee.set_active(Some(0));
            if let Some((_, r)) = mapping.iter().find(|(p, _)| *p == key) {
                referee.set_active_id(Some(&r.to_string()));
            }
            row.append(&button);
            row.append(&referee);
            left.append(&row);
            (key, button, referee)
        })
        .collect();
    // Ne pas proposer sur une autre piste un arbitre déjà choisi dans ce
    // dialogue. Les références faibles évitent de retenir les widgets en cycle.
    let update_choices: Rc<dyn Fn()> = {
        let controls: Vec<_> = piste_buttons
            .iter()
            .map(|(_, b, c)| (b.downgrade(), c.downgrade()))
            .collect();
        let names: Vec<_> = referees
            .iter()
            .map(|(r, name, _)| (*r, name.clone()))
            .collect();
        let updating = Cell::new(false);
        let weak_ui = Rc::downgrade(ui);
        Rc::new(move || {
            if updating.replace(true) {
                return;
            }
            let selected: Vec<_> = controls
                .iter()
                .map(|(b, c)| {
                    let active = b.upgrade().is_some_and(|b| b.is_active());
                    let referee = c
                        .upgrade()
                        .and_then(|c| c.active_id())
                        .and_then(|id| id.parse::<u32>().ok());
                    (active, referee)
                })
                .collect();
            let available: Vec<_> = weak_ui
                .upgrade()
                .map(|ui| {
                    let hall = ui.hall.borrow();
                    names
                        .iter()
                        .filter(|(r, _)| hall.pool_referee_available(job, *r))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            for (i, (_, c)) in controls.iter().enumerate() {
                let Some(combo) = c.upgrade() else { continue };
                let current = selected[i].1;
                combo.remove_all();
                combo.append(Some(""), &tr("Select a referee"));
                combo.set_active(Some(0));
                for (reference, name) in &available {
                    if !selected
                        .iter()
                        .enumerate()
                        .any(|(j, (active, r))| j != i && *active && *r == Some(*reference))
                    {
                        combo.append(Some(&reference.to_string()), name);
                        if current == Some(*reference) {
                            combo.set_active_id(Some(&reference.to_string()));
                        }
                    }
                }
            }
            updating.set(false);
        })
    };
    for (_, button, combo) in &piste_buttons {
        let update = update_choices.clone();
        button.connect_toggled(move |_| update());
        let update = update_choices.clone();
        combo.connect_changed(move |_| update());
    }
    update_choices();
    for (_, button, _) in &piste_buttons {
        let controls: Vec<_> = piste_buttons
            .iter()
            .map(|(_, b, c)| (b.downgrade(), c.downgrade()))
            .collect();
        let weak_right = right.downgrade();
        let manage_referees = ui.management_features().referees;
        let update = move || {
            let multiple = controls
                .iter()
                .filter_map(|(b, _)| b.upgrade())
                .filter(|b| b.is_active())
                .count()
                > 1;
            if let Some(right) = weak_right.upgrade() {
                right.set_visible(!multiple && manage_referees);
            }
            for (b, c) in &controls {
                if let (Some(b), Some(c)) = (b.upgrade(), c.upgrade()) {
                    c.set_visible(multiple && manage_referees);
                    c.set_sensitive(b.is_active());
                }
            }
        };
        update();
        button.connect_toggled(move |_| update());
    }
    let referee_buttons: Vec<_> = referees
        .into_iter()
        .map(|(key, name, active)| {
            let button = gtk::CheckButton::with_label(&name);
            button.set_active(active);
            right.append(&button);
            (key, button)
        })
        .collect();
    for column in [&left, &right] {
        let scroll = gtk::ScrolledWindow::new();
        scroll.set_hexpand(true);
        scroll.set_vexpand(true);
        scroll.set_child(Some(column));
        columns.append(&scroll);
    }
    content.append(&columns);
    if !ui.management_features().referees {
        right.set_visible(false);
        for (_, _, combo) in &piste_buttons {
            combo.set_visible(false);
        }
    }
    let error = gtk::Label::new(None);
    error.set_wrap(true);
    error.add_css_class("error");
    content.append(&error);
    while dialogs::run_dialog(&dialog) == gtk::ResponseType::Ok {
        let pistes: Vec<_> = piste_buttons
            .iter()
            .filter(|(_, b, _)| b.is_active())
            .map(|(key, _, _)| *key)
            .collect();
        let referees: Vec<_> = if !ui.management_features().referees {
            ui.hall
                .borrow()
                .jobs
                .get(&job)
                .and_then(|j| j.slot)
                .and_then(|sk| ui.hall.borrow().slots.get(&sk).map(|s| s.referees.clone()))
                .unwrap_or_default()
        } else if pistes.len() > 1 {
            piste_buttons
                .iter()
                .filter(|(_, b, _)| b.is_active())
                .filter_map(|(_, _, c)| c.active_id()?.parse::<u32>().ok())
                .collect()
        } else {
            referee_buttons
                .iter()
                .filter(|(_, b)| b.is_active())
                .map(|(key, _)| *key)
                .collect()
        };
        let result = ui.hall.borrow_mut().set_pool_resources_optional(
            job,
            &pistes,
            &referees,
            ui.management_features().referees,
        );
        match result {
            Ok(()) => {
                ui.after_change();
                break;
            }
            Err(message) => {
                error.set_text(&message);
                update_choices();
            }
        }
    }
    dialog.destroy();
}
