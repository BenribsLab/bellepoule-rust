//! Entraînement des jeunes : placer un groupe entier, et confirmation des
//! arbitres du même club que les tireurs.
use super::Ui;
use crate::groups::ClubConflict;
use bp_core::i18n::tr;
use bp_gtk::dialogs;
use gtk4::{self as gtk, prelude::*};
use std::rc::Rc;

/// Arbitres du même club qu'un tireur : à confirmer. Vrai si l'on continue.
pub fn confirm_conflicts(ui: &Rc<Ui>, conflicts: &[ClubConflict]) -> bool {
    if conflicts.is_empty() {
        return true;
    }
    let mut lines: Vec<String> = conflicts.iter().map(|c| format!("• {}", c.message())).collect();
    lines.dedup();
    dialogs::confirm(
        Some(ui.window()),
        &format!("<b>{}</b>", gtk::glib::markup_escape_text(&tr("Referee of the same club as a fencer"))),
        Some(&format!("{}\n\n{}", lines.join("\n"), tr("Assign anyway?"))),
        Some(&tr("Referees")),
    )
}

/// Arbitres libres pour le groupe (nom, club) : ceux déjà pris pendant le
/// groupe ne sont pas proposés.
fn referee_choices(ui: &Rc<Ui>, competition: u32, batch: u32, group: u32) -> Vec<(u32, String)> {
    let hall = ui.hall.borrow();
    let mut list: Vec<_> = hall
        .referees
        .referees
        .iter()
        .filter(|(key, _)| hall.group_referee_available(competition, batch, group, **key))
        .map(|(key, r)| {
            let club = r.player.str_attr("club").unwrap_or_default().to_string();
            let name = format!("{} {}", r.player.name(), r.player.first_name());
            (*key, if club.is_empty() { name } else { format!("{name} ({club})") })
        })
        .collect();
    list.sort_by(|a, b| a.1.cmp(&b.1));
    list
}

fn check_list(title: &str, items: &[(u32, String, bool)]) -> (gtk::Widget, gtk::Box, Vec<(u32, gtk::CheckButton)>) {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 4);
    let label = gtk::Label::new(None);
    label.set_markup(&format!("<b>{}</b>", gtk::glib::markup_escape_text(title)));
    label.set_halign(gtk::Align::Start);
    column.append(&label);
    let list = gtk::Box::new(gtk::Orientation::Vertical, 2);
    let buttons: Vec<_> = items
        .iter()
        .map(|(key, name, active)| {
            let b = gtk::CheckButton::with_label(name);
            b.set_active(*active);
            list.append(&b);
            (*key, b)
        })
        .collect();
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_vexpand(true);
    scroll.set_hexpand(true);
    scroll.set_min_content_height(220);
    scroll.set_child(Some(&list));
    column.append(&scroll);
    (column.upcast(), list, buttons)
}

/// « Placer un groupe… » : le groupe, ses pistes et ses arbitres.
pub fn place_group(ui: &Rc<Ui>, competition: u32, batch: u32, selected: Option<crate::slot::JobKey>) {
    let (groups, pistes) = {
        let hall = ui.hall.borrow();
        let groups = hall.batch_groups(competition, batch);
        let pistes: Vec<_> = hall.pistes.iter().filter(|p| !p.blocked).map(|p| (p.key, tr("Piste %d").replace("%d", &p.id.to_string()), false)).collect();
        (groups, pistes)
    };
    if groups.is_empty() {
        return;
    }
    let current = selected
        .and_then(|j| ui.hall.borrow().jobs.get(&j).and_then(|j| j.group))
        .and_then(|g| groups.iter().position(|x| *x == g))
        .unwrap_or(0);
    let dialog = gtk::Dialog::with_buttons(
        Some(&tr("Place a group")),
        Some(ui.window()),
        gtk::DialogFlags::MODAL,
        &[(&tr("_Cancel"), gtk::ResponseType::Cancel), (&tr("_OK"), gtk::ResponseType::Ok)],
    );
    dialog.set_default_size(560, 520);
    let content = dialog.content_area();
    content.set_spacing(8);
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    content.set_margin_start(12);
    content.set_margin_end(12);
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.append(&gtk::Label::new(Some(&tr("Group"))));
    let labels: Vec<String> = groups.iter().map(|g| format!("{} {g}", tr("Group"))).collect();
    let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    let group_list = gtk::DropDown::from_strings(&refs);
    group_list.set_selected(current as u32);
    row.append(&group_list);
    content.append(&row);
    let hint = gtk::Label::new(Some(&tr(
        "One piste: the pools of the group follow one another. As many pistes as pools (or more): the pools are fenced at the same time. The referees are shared between the pistes; on a piste with several referees, they take turns bout after bout, avoiding fencers of their club.",
    )));
    hint.set_wrap(true);
    hint.set_xalign(0.0);
    content.append(&hint);
    let manage_pistes = ui.management_features().pistes;
    let manage_referees = ui.management_features().referees;
    let columns = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    columns.set_vexpand(true);
    let (piste_box, _, piste_buttons) = check_list(&tr("Pistes"), &pistes);
    let referees: Vec<_> = referee_choices(ui, competition, batch, groups[current]).into_iter().map(|(k, n)| (k, n, false)).collect();
    let (referee_box, referee_list, referee_buttons) = check_list(&tr("Referees"), &referees);
    let referee_buttons = Rc::new(std::cell::RefCell::new(referee_buttons));
    // Changer de groupe recalcule les arbitres libres.
    {
        let ui = ui.clone();
        let groups = groups.clone();
        let referee_buttons = referee_buttons.clone();
        group_list.connect_selected_notify(move |list| {
            let group = groups[(list.selected() as usize).min(groups.len() - 1)];
            let mut buttons = referee_buttons.borrow_mut();
            let parent = &referee_list;
            for (_, b) in buttons.drain(..) {
                parent.remove(&b);
            }
            for (key, name) in referee_choices(&ui, competition, batch, group) {
                let b = gtk::CheckButton::with_label(&name);
                parent.append(&b);
                buttons.push((key, b));
            }
        });
    }
    piste_box.set_visible(manage_pistes);
    referee_box.set_visible(manage_referees);
    columns.append(&piste_box);
    columns.append(&referee_box);
    content.append(&columns);
    let error = gtk::Label::new(None);
    error.set_wrap(true);
    error.add_css_class("error");
    content.append(&error);
    while dialogs::run_dialog(&dialog) == gtk::ResponseType::Ok {
        let group = groups[(group_list.selected() as usize).min(groups.len() - 1)];
        let chosen_pistes: Vec<_> = piste_buttons.iter().filter(|(_, b)| b.is_active()).map(|(k, _)| *k).collect();
        let chosen_referees: Vec<_> = referee_buttons.borrow().iter().filter(|(_, b)| b.is_active()).map(|(k, _)| *k).collect();
        let conflicts = ui.hall.borrow().group_club_conflicts(competition, batch, group, &chosen_referees);
        if !confirm_conflicts(ui, &conflicts) {
            continue;
        }
        let result = ui.hall.borrow_mut().place_group(competition, batch, group, &chosen_pistes, &chosen_referees);
        match result {
            Ok(()) => {
                ui.after_change();
                break;
            }
            Err(e) => error.set_text(&e),
        }
    }
    dialog.destroy();
}
