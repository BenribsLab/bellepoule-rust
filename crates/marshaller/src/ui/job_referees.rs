//! Affecter des arbitres avec ou sans piste.
use super::Ui;
use crate::slot::JobKey;
use bp_core::i18n::tr;
use bp_gtk::dialogs;
use gtk4::{self as gtk, prelude::*};
use std::rc::Rc;

pub fn edit(ui: &Rc<Ui>, job: JobKey) {
    let snapshot = {
        let hall = ui.hall.borrow();
        hall.jobs.get(&job).map(|j| {
            let selected = j
                .slot
                .and_then(|sk| hall.slots.get(&sk))
                .map(|s| s.referees.clone())
                .unwrap_or_default();
            let mut refs: Vec<_> = hall
                .referees
                .referees
                .iter()
                .filter(|(r, _)| hall.pool_referee_available(job, **r))
                .map(|(r, p)| {
                    (
                        *r,
                        format!("{} {}", p.player.name(), p.player.first_name()),
                        selected.contains(r),
                    )
                })
                .collect();
            refs.sort_by(|a, b| a.1.cmp(&b.1));
            (j.name.clone(), refs)
        })
    };
    let Some((name, referees)) = snapshot else {
        return;
    };
    let dialog = gtk::Dialog::with_buttons(
        Some(&tr("Assign referees")),
        Some(ui.window()),
        gtk::DialogFlags::MODAL,
        &[
            (&tr("_Cancel"), gtk::ResponseType::Cancel),
            (&tr("_OK"), gtk::ResponseType::Ok),
        ],
    );
    dialog.set_default_size(400, 420);
    let content = dialog.content_area();
    content.set_spacing(8);
    content.append(&gtk::Label::new(Some(&name)));
    let list = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let buttons: Vec<_> = referees
        .into_iter()
        .map(|(r, name, active)| {
            let button = gtk::CheckButton::with_label(&name);
            button.set_active(active);
            list.append(&button);
            (r, button)
        })
        .collect();
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_vexpand(true);
    scroll.set_child(Some(&list));
    content.append(&scroll);
    let error = gtk::Label::new(None);
    error.set_wrap(true);
    error.add_css_class("error");
    content.append(&error);
    while dialogs::run_dialog(&dialog) == gtk::ResponseType::Ok {
        let refs: Vec<_> = buttons
            .iter()
            .filter(|(_, b)| b.is_active())
            .map(|(r, _)| *r)
            .collect();
        let conflicts = ui.hall.borrow().club_conflicts(job, &refs);
        if !super::group_dialog::confirm_conflicts(ui, &conflicts) {
            continue;
        }
        let result = ui.hall.borrow_mut().set_job_referees(job, &refs);
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
