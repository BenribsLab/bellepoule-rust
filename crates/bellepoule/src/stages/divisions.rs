//! Poules de division : réglages (formule), répartition des tireurs en
//! divisions, montées et descentes après un tour, modifiables à la main.

use bp_core::{
    i18n::tr,
    player::PlayerRef,
    rounds::divisions::{DivisionRound, DivisionSettings, DivisionStart, Move},
    stage::Stage,
};
use gtk::prelude::*;
use gtk4 as gtk;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use super::StageScreen;
use crate::contest_page::ContestPage;
use crate::widgets::{
    list_document,
    players_list::{Cell as ListCell, CellKind, ColumnSpec, PlayersListView},
    render,
};

pub struct DivisionScreen {
    page: Weak<ContestPage>,
    index: usize,
    root: gtk::Box,
    configuration: gtk::Widget,
    list: PlayersListView,
    summary: gtk::Label,
    assign: gtk::DropDown,
    assign_count: Cell<u32>,
    divisions: gtk::SpinButton,
    pools: gtk::SpinButton,
    up: gtk::SpinButton,
    down: gtk::SpinButton,
    start: gtk::DropDown,
    updating: Cell<bool>,
    me: RefCell<Weak<DivisionScreen>>,
}

fn spin(min: f64, max: f64) -> gtk::SpinButton {
    let s = gtk::SpinButton::with_range(min, max, 1.0);
    s.set_digits(0);
    bp_gtk::util::scroll_passes_through(&s);
    s
}

fn labelled(text: &str, widget: &impl IsA<gtk::Widget>) -> gtk::Box {
    let line = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let label = gtk::Label::new(Some(text));
    label.set_xalign(0.0);
    label.set_size_request(220, -1);
    line.append(&label);
    line.append(widget);
    line
}

impl DivisionScreen {
    pub fn new(page: &Rc<ContestPage>, index: usize) -> Rc<Self> {
        // Réglages (préférences de la phase, dans la formule).
        let config = gtk::Box::new(gtk::Orientation::Vertical, 6);
        config.set_margin_top(10);
        config.set_margin_start(10);
        config.set_margin_end(10);
        let divisions = spin(1.0, 26.0);
        let pools = spin(1.0, 20.0);
        let up = spin(0.0, 50.0);
        let down = spin(0.0, 50.0);
        let start_items = [tr("by ranking (best in D1)"), tr("at random")];
        let start_refs: Vec<&str> = start_items.iter().map(String::as_str).collect();
        let start = gtk::DropDown::from_strings(&start_refs);
        bp_gtk::util::scroll_passes_through(&start);
        config.append(&labelled(&tr("Divisions"), &divisions));
        config.append(&labelled(&tr("Pools per division"), &pools));
        config.append(&labelled(&tr("Promoted after each round"), &up));
        config.append(&labelled(&tr("Relegated after each round"), &down));
        config.append(&labelled(&tr("First distribution"), &start));
        let help = gtk::Label::new(Some(&tr("Add another \"Division pools\" phase for each new round: promotions and relegations are then proposed from the previous round.")));
        help.set_wrap(true);
        help.set_xalign(0.0);
        help.add_css_class("bp-muted");
        config.append(&help);
        let frame = gtk::Frame::new(None);
        frame.set_child(Some(&config));

        // Écran de la phase.
        let root = gtk::Box::new(gtk::Orientation::Vertical, 6);
        let bar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        bar.add_css_class("toolbar");
        bar.set_margin_start(8);
        bar.set_margin_top(4);
        let summary = gtk::Label::new(None);
        summary.set_hexpand(true);
        summary.set_xalign(0.0);
        bar.append(&summary);
        bar.append(&gtk::Label::new(Some(&tr("Move the selection to"))));
        let assign = gtk::DropDown::from_strings(&["D1"]);
        bp_gtk::util::scroll_passes_through(&assign);
        bar.append(&assign);
        let place = gtk::Button::with_label(&tr("Move"));
        bar.append(&place);
        root.append(&bar);
        let scrolled = gtk::ScrolledWindow::new();
        scrolled.set_vexpand(true);
        root.append(&scrolled);
        let list = PlayersListView::in_scrolled(scrolled);

        let screen = Rc::new(Self {
            page: Rc::downgrade(page),
            index,
            root,
            configuration: frame.upcast(),
            list,
            summary,
            assign,
            assign_count: Cell::new(1),
            divisions,
            pools,
            up,
            down,
            start,
            updating: Cell::new(false),
            me: RefCell::new(Weak::new()),
        });
        *screen.me.borrow_mut() = Rc::downgrade(&screen);
        for s in [&screen.divisions, &screen.pools, &screen.up, &screen.down] {
            let w = Rc::downgrade(&screen);
            s.connect_value_changed(move |_| {
                if let Some(s) = w.upgrade() {
                    s.on_settings_changed();
                }
            });
        }
        let w = Rc::downgrade(&screen);
        screen.start.connect_selected_notify(move |_| {
            if let Some(s) = w.upgrade() {
                s.on_settings_changed();
            }
        });
        let w = Rc::downgrade(&screen);
        place.connect_clicked(move |_| {
            if let Some(s) = w.upgrade() {
                let targets = s.list.selected();
                s.assign_to(&targets, s.assign.selected() + 1);
            }
        });
        screen.refresh();
        screen
    }

    fn with_round<R>(&self, f: impl FnOnce(&mut DivisionRound) -> R) -> Option<R> {
        let page = self.page.upgrade()?;
        page.with_contest(|c| {
            let stage = c.schedule.stage_mut(self.index)?.as_any_mut().downcast_mut::<DivisionRound>()?;
            Some(f(stage))
        })
        .flatten()
    }

    fn assign_to(&self, targets: &[PlayerRef], division: u32) {
        let changed = self.with_round(|r| {
            if r.base().locked {
                return false;
            }
            for t in targets {
                r.set_division(*t, division);
            }
            true
        });
        if changed == Some(true) {
            if let Some(p) = self.page.upgrade() {
                p.mark_dirty();
            }
            self.refresh();
        }
    }

    fn on_settings_changed(&self) {
        if self.updating.get() {
            return;
        }
        let settings = DivisionSettings {
            divisions: self.divisions.value_as_int().max(1) as u32,
            pools: self.pools.value_as_int().max(1) as u32,
            up: self.up.value_as_int().max(0) as u32,
            down: self.down.value_as_int().max(0) as u32,
            start: if self.start.selected() == 1 { DivisionStart::Random } else { DivisionStart::Ranking },
            swapping: self.with_round(|r| r.settings.swapping.clone()).unwrap_or_default(),
        };
        let changed = self.with_round(|r| {
            if r.base().locked || r.settings == settings {
                return false;
            }
            r.set_settings(settings);
            true
        });
        if changed == Some(true) {
            if let Some(p) = self.page.upgrade() {
                p.mark_dirty();
            }
            self.refresh();
        }
    }
}

impl StageScreen for DivisionScreen {
    fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }

    fn configuration(&self) -> Option<gtk::Widget> {
        Some(self.configuration.clone())
    }

    fn refresh(&self) {
        let Some(page) = self.page.upgrade() else { return };
        let data = page
            .with_contest(|c| {
                let r = c.schedule.stage(self.index)?.as_any().downcast_ref::<DivisionRound>()?;
                let base = r.base();
                let filter = bp_core::filter::Filter::new("divisions", base.filter.clone());
                let mut specs = render::column_specs(filter.layout(), false);
                let mut rows = list_document::rows_for(&c.players, &base.attendees, &specs, base.owner());
                let text = |code: &str, title: String| ColumnSpec {
                    code: code.into(),
                    title,
                    look: bp_core::attribute::Look::LongText,
                    kind: CellKind::Text,
                    editable: false,
                };
                specs.insert(0, text("division", tr("Division")));
                specs.insert(1, text("movement", tr("Movement")));
                for row in rows.iter_mut() {
                    let d = r.division_of(row.player);
                    let movement = match (r.movement(row.player), r.origin_of(row.player)) {
                        (Move::Up, Some(o)) => format!("↑ {} D{o}", tr("from")),
                        (Move::Down, Some(o)) => format!("↓ {} D{o}", tr("from")),
                        (Move::Stays, _) => "=".into(),
                        _ => String::new(),
                    };
                    row.cells.insert(0, ListCell::Text(format!("D{d}")));
                    row.cells.insert(1, ListCell::Text(movement));
                    row.sort.insert(0, Some(bp_core::attribute::Value::UInt(d)));
                    row.sort.insert(1, None);
                }
                let sizes: Vec<String> = (1..=r.settings.divisions).map(|d| format!("D{d} : {}", r.members(d).len())).collect();
                Some((specs, rows, base.locked, r.settings.clone(), sizes))
            })
            .flatten();
        let Some((specs, rows, locked, settings, sizes)) = data else { return };
        self.updating.set(true);
        self.list.set_columns(specs);
        self.list.set_rows(rows);
        self.list.set_locked(locked);
        self.summary.set_text(&sizes.join("   "));
        self.divisions.set_value(settings.divisions as f64);
        self.pools.set_value(settings.pools as f64);
        self.up.set_value(settings.up as f64);
        self.down.set_value(settings.down as f64);
        self.start.set_selected(if settings.start == DivisionStart::Random { 1 } else { 0 });
        if self.assign_count.get() != settings.divisions {
            let names: Vec<String> = (1..=settings.divisions).map(|d| format!("D{d}")).collect();
            let refs: Vec<&str> = names.iter().map(String::as_str).collect();
            self.assign.set_model(Some(&gtk::StringList::new(&refs)));
            self.assign_count.set(settings.divisions);
        }
        self.assign.set_sensitive(!locked);
        self.configuration.set_sensitive(!locked);
        self.updating.set(false);
    }
}
