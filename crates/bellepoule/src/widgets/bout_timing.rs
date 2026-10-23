//! Réglage du chrono dans les préférences de la phase ; pour une compétition
//! par équipes, les rencontres en relais (type, taille des équipes, points
//! par relais, temps maximum d'un relais).
use std::rc::Rc;
use gtk4::{self as gtk, prelude::*};
use bp_core::officiating::BoutTiming;
use bp_core::relay::{RelayFormat, RelayKind};
use crate::contest_page::ContestPage;

pub fn wrap(configuration: &gtk::Widget, page: &Rc<ContestPage>, index: usize) -> gtk::Widget {
    let container = gtk::Box::new(gtk::Orientation::Vertical, 12);
    container.append(configuration);
    let Some((timing, locked, team)) = page
        .with_contest(|c| {
            c.schedule.stage(index).map(|s| {
                let mut t = s.bout_timing();
                t.relay = c.relay_format(index);
                (t, s.base().locked, c.settings.team_event)
            })
        })
        .flatten()
    else {
        return container.upcast();
    };
    let grid = gtk::Grid::new();
    grid.set_row_spacing(6);
    grid.set_column_spacing(10);
    grid.set_margin_start(12);
    grid.set_margin_end(12);
    grid.set_margin_bottom(12);
    let label = |text: &str| {
        let l = gtk::Label::new(Some(text));
        l.set_xalign(1.0);
        l
    };
    let seconds = gtk::SpinButton::with_range(1.0, 3600.0, 1.0);
    seconds.set_value(timing.period_seconds as f64);
    let periods = gtk::SpinButton::with_range(1.0, 20.0, 1.0);
    periods.set_value(timing.periods as f64);
    let kind = gtk::DropDown::from_strings(&["Chacun contre chacun", "Relais simple (un match par tireur)"]);
    kind.set_selected(if timing.relay.kind == RelayKind::Single { 1 } else { 0 });
    let points = gtk::SpinButton::with_range(1.0, 15.0, 1.0);
    points.set_value(timing.relay.points as f64);
    let summary = gtk::Label::new(None);
    summary.set_xalign(0.0);
    summary.set_wrap(true);
    if team {
        let title = gtk::Label::new(None);
        title.set_markup("<b>Rencontres par équipes</b>");
        title.set_xalign(0.0);
        grid.attach(&title, 0, 0, 2, 1);
        grid.attach(&label("Type de rencontre"), 0, 1, 1, 1);
        grid.attach(&kind, 1, 1, 1, 1);
        let team = gtk::Label::new(Some(&format!("{} tireurs et {} remplaçant(s) par équipe (réglés à l'appel)", timing.relay.team_size, timing.relay.substitutes)));
        team.set_xalign(0.0);
        grid.attach(&label("Équipes"), 0, 2, 1, 1);
        grid.attach(&team, 1, 2, 1, 1);
        grid.attach(&label("Points par relais"), 0, 3, 1, 1);
        grid.attach(&points, 1, 3, 1, 1);
        grid.attach(&label("Temps maximum d'un relais (secondes)"), 0, 4, 1, 1);
        grid.attach(&seconds, 1, 4, 1, 1);
        grid.attach(&summary, 0, 5, 2, 1);
        let blank = gtk::Button::with_label("Feuilles de match vierges…");
        blank.set_halign(gtk::Align::Start);
        {
            let page = Rc::downgrade(page);
            blank.connect_clicked(move |_| {
                if let Some(page) = page.upgrade() {
                    crate::stages::relay_print::print(&page, index, None, true);
                }
            });
        }
        grid.attach(&blank, 0, 6, 2, 1);
    } else {
        grid.attach(&label("Durée d’une manche (secondes)"), 0, 0, 1, 1);
        grid.attach(&seconds, 1, 0, 1, 1);
        grid.attach(&label("Nombre de manches"), 0, 1, 1, 1);
        grid.attach(&periods, 1, 1, 1, 1);
    }
    grid.set_sensitive(!locked);

    let read: Rc<dyn Fn() -> BoutTiming> = {
        let (seconds, periods, kind, points) = (seconds.downgrade(), periods.downgrade(), kind.downgrade(), points.downgrade());
        Rc::new(move || {
            let mut t = timing;
            if let (Some(s), Some(p)) = (seconds.upgrade(), periods.upgrade()) {
                t.period_seconds = s.value_as_int() as u32;
                t.periods = p.value_as_int() as u32;
            }
            if let (Some(k), Some(p)) = (kind.upgrade(), points.upgrade()) {
                // Tireurs et remplaçants : réglage de la compétition (appel).
                t.relay = RelayFormat {
                    kind: if k.selected() == 1 { RelayKind::Single } else { RelayKind::EachVsEach },
                    points: p.value_as_int() as u32,
                    ..timing.relay
                };
            }
            t
        })
    };
    let show: Rc<dyn Fn(&BoutTiming)> = {
        let summary = summary.downgrade();
        Rc::new(move |t: &BoutTiming| {
            if let Some(summary) = summary.upgrade() {
                let order = t.relay.bouts().iter().map(|(a, b)| format!("{a}-{b}")).collect::<Vec<_>>().join(" / ");
                let [top, bottom] = t.relay.numbers();
                let numbers = |l: &[u32]| l.iter().map(u32::to_string).collect::<Vec<_>>().join("-");
                summary.set_text(&format!(
                    "Haut : {} · Bas : {}\nOrdre des relais : {order}\nScore de la rencontre : {} touches",
                    numbers(&top),
                    numbers(&bottom),
                    t.relay.total()
                ));
            }
        })
    };
    show(&timing);
    let apply = {
        let page = Rc::downgrade(page);
        let read = read.clone();
        let show = show.clone();
        move || {
            let Some(page) = page.upgrade() else { return };
            let value = read();
            show(&value);
            let changed = page.with_contest(|c| c.set_stage_timing(index, value).unwrap_or(false)).unwrap_or(false);
            if changed {
                page.mark_dirty();
            }
        }
    };
    for spin in [&seconds, &periods, &points] {
        let apply = apply.clone();
        spin.connect_value_changed(move |_| apply());
    }
    {
        let apply = apply.clone();
        kind.connect_selected_notify(move |_| apply());
    }
    container.append(&grid);
    container.upcast()
}
