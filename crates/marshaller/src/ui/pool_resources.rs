//! Choix des ressources communes d'une poule planifiée : ses pistes et ses
//! arbitres (autant qu'on veut ; répartis sur les pistes, ils alternent
//! match après match).
use super::Ui;
use crate::slot::JobKey;
use bp_core::i18n::tr;
use bp_gtk::dialogs;
use gtk::prelude::*;
use gtk4 as gtk;
use std::rc::Rc;

pub fn edit(ui: &Rc<Ui>, job: JobKey) {
    edit_with_piste(ui, job, None);
}

fn check_list(title: &str, items: Vec<(u32, String, bool)>, sensitive: bool) -> (gtk::Widget, Vec<(u32, gtk::CheckButton)>) {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 4);
    let label = gtk::Label::new(None);
    label.set_markup(&format!("<b>{}</b>", gtk::glib::markup_escape_text(title)));
    label.set_halign(gtk::Align::Start);
    column.append(&label);
    let list = gtk::Box::new(gtk::Orientation::Vertical, 2);
    let buttons: Vec<_> = items
        .into_iter()
        .map(|(key, name, active)| {
            let b = gtk::CheckButton::with_label(&name);
            b.set_active(active);
            b.set_sensitive(sensitive);
            list.append(&b);
            (key, b)
        })
        .collect();
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_hexpand(true);
    scroll.set_vexpand(true);
    scroll.set_child(Some(&list));
    column.append(&scroll);
    (column.upcast(), buttons)
}

pub fn edit_with_piste(ui: &Rc<Ui>, job: JobKey, added_piste: Option<crate::slot::PisteKey>) {
    let manage_pistes = ui.management_features().pistes;
    let manage_referees = ui.management_features().referees;
    let snapshot = {
        let hall = ui.hall.borrow();
        hall.jobs.get(&job).filter(|j| j.is_pool).and_then(|j| {
            let slot = hall.slots.get(&j.slot?)?;
            let pistes: Vec<_> = hall
                .pistes
                .iter()
                .filter(|p| manage_pistes || slot.pistes().contains(&p.key))
                .map(|p| (p.key, tr("Piste %d").replace("%d", &p.id.to_string()), slot.pistes().contains(&p.key) || added_piste == Some(p.key)))
                .collect();
            let label = |key: u32| {
                hall.referees.get(key).map(|r| {
                    let club = r.player.str_attr("club").unwrap_or_default().to_string();
                    let name = format!("{} {}", r.player.name(), r.player.first_name());
                    if club.is_empty() { name } else { format!("{name} ({club})") }
                })
            };
            // Les arbitres de la poule d'abord, dans leur ordre (un par piste).
            let mut referees: Vec<_> = slot.referees.iter().filter_map(|r| label(*r).map(|n| (*r, n, true))).collect();
            let mut others: Vec<_> = hall
                .referees
                .referees
                .keys()
                .filter(|key| !slot.referees.contains(key) && hall.pool_referee_available(job, **key))
                .filter_map(|key| label(*key).map(|n| (*key, n, false)))
                .collect();
            others.sort_by(|a, b| a.1.cmp(&b.1));
            referees.extend(others);
            Some((j.name.clone(), pistes, referees))
        })
    };
    let Some((name, pistes, referees)) = snapshot else {
        return;
    };
    let dialog = gtk::Dialog::with_buttons(
        Some(&tr("Pool resources")),
        Some(ui.window()),
        gtk::DialogFlags::MODAL,
        &[(&tr("_Cancel"), gtk::ResponseType::Cancel), (&tr("_OK"), gtk::ResponseType::Ok)],
    );
    dialog.set_default_size(560, 500);
    let content = dialog.content_area();
    content.set_spacing(8);
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    content.set_margin_start(12);
    content.set_margin_end(12);
    let title = gtk::Label::new(None);
    title.set_markup(&format!("<b>{}</b>", gtk::glib::markup_escape_text(&name)));
    content.append(&title);
    let hint = gtk::Label::new(Some(&tr(
        "Select the pistes and the referees of this pool. With several referees, they are shared between the pistes and take turns bout after bout, avoiding fencers of their club. Matches are scheduled in waves without simultaneous bouts for a fencer.",
    )));
    hint.set_wrap(true);
    hint.set_xalign(0.0);
    content.append(&hint);
    let columns = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    columns.set_vexpand(true);
    let (piste_box, piste_buttons) = check_list(&tr("Pistes"), pistes, manage_pistes);
    let (referee_box, referee_buttons) = check_list(&tr("Referees"), referees, true);
    referee_box.set_visible(manage_referees);
    columns.append(&piste_box);
    columns.append(&referee_box);
    content.append(&columns);
    let error = gtk::Label::new(None);
    error.set_wrap(true);
    error.add_css_class("error");
    content.append(&error);
    while dialogs::run_dialog(&dialog) == gtk::ResponseType::Ok {
        let pistes: Vec<_> = piste_buttons.iter().filter(|(_, b)| b.is_active()).map(|(key, _)| *key).collect();
        let referees: Vec<_> = if manage_referees {
            referee_buttons.iter().filter(|(_, b)| b.is_active()).map(|(key, _)| *key).collect()
        } else {
            let hall = ui.hall.borrow();
            hall.jobs.get(&job).and_then(|j| j.slot).and_then(|sk| hall.slots.get(&sk).map(|s| s.referees.clone())).unwrap_or_default()
        };
        let conflicts = ui.hall.borrow().club_conflicts(job, &referees);
        if manage_referees && !super::group_dialog::confirm_conflicts(ui, &conflicts) {
            continue;
        }
        let result = ui.hall.borrow_mut().set_pool_resources_optional(job, &pistes, &referees, manage_referees);
        match result {
            Ok(()) => {
                ui.after_change();
                break;
            }
            Err(message) => error.set_text(&message),
        }
    }
    dialog.destroy();
}
