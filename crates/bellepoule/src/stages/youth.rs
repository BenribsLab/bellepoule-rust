//! Écran « Entraînement des jeunes » : réglages des groupes et des poules,
//! répartitions proposées (la meilleure en premier) et aperçu des groupes.
//! « Tour suivant » crée une compétition par groupe.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use bp_core::i18n::tr;
use bp_core::player::PlayerRef;
use bp_core::rounds::youth::{self, GroupMode, PoolMode, Proposal, YouthSettings, YouthTraining};
use bp_core::stage::Stage;
use gtk::prelude::*;
use gtk4 as gtk;

use super::StageScreen;
use crate::contest_page::ContestPage;

struct Snapshot {
    settings: YouthSettings,
    locked: bool,
    proposals: Vec<Proposal>,
    /// Pour chaque groupe : tailles des poules et « rang. NOM Prénom ».
    groups: Vec<(Vec<u32>, Vec<String>)>,
    count: usize,
}

pub struct YouthScreen {
    page: Weak<ContestPage>,
    index: usize,
    root: gtk::Box,
    group_size: gtk::SpinButton,
    pools: gtk::SpinButton,
    division: gtk::CheckButton,
    ranking: gtk::CheckButton,
    snake: gtk::CheckButton,
    level: gtk::CheckButton,
    summary: gtk::Label,
    proposals: gtk::ListBox,
    proposal_data: RefCell<Vec<Proposal>>,
    groups: gtk::FlowBox,
    updating: Cell<bool>,
}

fn section(title: &str) -> gtk::Label {
    let l = gtk::Label::new(None);
    l.set_markup(&format!("<b>{}</b>", gtk::glib::markup_escape_text(title)));
    l.set_xalign(0.0);
    l.set_margin_top(6);
    l
}

/// « 3 groupes de 8 · 2 poules de 4 · 6 matchs par tireur ».
fn describe(p: &Proposal) -> String {
    let groups = if p.smallest == p.largest {
        format!("{} {} {}", p.nb_groups, tr("groups of"), p.largest)
    } else {
        format!("{} {} {} {} {}", p.nb_groups, tr("groups of"), p.smallest, tr("to"), p.largest)
    };
    let sizes = youth::pool_sizes(p.largest, p.pools_per_group);
    let pool = if sizes.first() == sizes.last() {
        sizes.first().copied().unwrap_or(0).to_string()
    } else {
        format!("{} {} {}", sizes.last().copied().unwrap_or(0), tr("or"), sizes.first().copied().unwrap_or(0))
    };
    format!(
        "{groups} · {} {} {pool} · {} {}",
        p.pools_per_group,
        if p.pools_per_group > 1 { tr("pools of") } else { tr("pool of") },
        p.bouts_per_fencer,
        tr("bouts per fencer")
    )
}

impl YouthScreen {
    pub fn new(page: &Rc<ContestPage>, index: usize) -> Rc<Self> {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 6);
        root.set_margin_top(12);
        root.set_margin_bottom(12);
        root.set_margin_start(12);
        root.set_margin_end(12);

        let intro = gtk::Label::new(Some(&tr(
            "Each group becomes its own competition: pools, then a table where every place is fenced (one winner per group). Click \"Next round\" to create the groups.",
        )));
        intro.set_wrap(true);
        intro.set_xalign(0.0);
        root.append(&intro);

        // Réglages
        let grid = gtk::Grid::new();
        grid.set_row_spacing(6);
        grid.set_column_spacing(12);
        let group_size = gtk::SpinButton::with_range(4.0, 16.0, 1.0);
        let pools = gtk::SpinButton::with_range(1.0, 4.0, 1.0);
        let label = |t: &str| {
            let l = gtk::Label::new(Some(t));
            l.set_xalign(0.0);
            l
        };
        grid.attach(&label(&tr("Fencers per group")), 0, 0, 1, 1);
        grid.attach(&group_size, 1, 0, 1, 1);
        grid.attach(&label(&tr("Pools per group")), 0, 1, 1, 1);
        grid.attach(&pools, 1, 1, 1, 1);
        let division = gtk::CheckButton::with_label(&tr("By division (the best fencers in group 1)"));
        let ranking = gtk::CheckButton::with_label(&tr("By ranking (groups of equal level, serpentine)"));
        ranking.set_group(Some(&division));
        grid.attach(&label(&tr("Groups")), 0, 2, 1, 1);
        grid.attach(&division, 1, 2, 1, 1);
        grid.attach(&ranking, 1, 3, 1, 1);
        let snake = gtk::CheckButton::with_label(&tr("Serpentine (1-4-5-8 / 2-3-6-7)"));
        let level = gtk::CheckButton::with_label(&tr("By level (the best fencers of the group in pool 1)"));
        level.set_group(Some(&snake));
        grid.attach(&label(&tr("Pools")), 0, 4, 1, 1);
        grid.attach(&snake, 1, 4, 1, 1);
        grid.attach(&level, 1, 5, 1, 1);
        let crossing = label(&tr("Table: 1st of pool 1 against last of pool 2, 2nd against second to last… All places are fenced."));
        crossing.set_wrap(true);
        grid.attach(&crossing, 0, 6, 2, 1);
        root.append(&section(&tr("Settings")));
        root.append(&grid);

        let summary = gtk::Label::new(None);
        summary.set_xalign(0.0);
        summary.set_wrap(true);
        root.append(&summary);

        // Propositions
        root.append(&section(&tr("Suggested arrangements")));
        let proposals = gtk::ListBox::new();
        proposals.set_selection_mode(gtk::SelectionMode::None);
        proposals.add_css_class("boxed-list");
        let proposals_scroll = gtk::ScrolledWindow::new();
        proposals_scroll.set_child(Some(&proposals));
        proposals_scroll.set_min_content_height(120);
        proposals_scroll.set_max_content_height(180);
        proposals_scroll.set_propagate_natural_height(true);
        root.append(&proposals_scroll);

        // Aperçu des groupes
        root.append(&section(&tr("Groups")));
        let groups = gtk::FlowBox::new();
        groups.set_selection_mode(gtk::SelectionMode::None);
        groups.set_valign(gtk::Align::Start);
        groups.set_max_children_per_line(6);
        groups.set_column_spacing(12);
        groups.set_row_spacing(12);
        let groups_scroll = gtk::ScrolledWindow::new();
        groups_scroll.set_child(Some(&groups));
        groups_scroll.set_vexpand(true);
        root.append(&groups_scroll);

        let screen = Rc::new(YouthScreen {
            page: Rc::downgrade(page),
            index,
            root,
            group_size,
            pools,
            division,
            ranking,
            snake,
            level,
            summary,
            proposals,
            proposal_data: RefCell::new(Vec::new()),
            groups,
            updating: Cell::new(false),
        });
        screen.connect();
        screen.refresh();
        screen
    }

    fn connect(self: &Rc<Self>) {
        for spin in [&self.group_size, &self.pools] {
            let w = Rc::downgrade(self);
            spin.connect_value_changed(move |_| {
                if let Some(s) = w.upgrade() {
                    s.on_settings_changed();
                }
            });
        }
        for check in [&self.division, &self.ranking, &self.snake, &self.level] {
            let w = Rc::downgrade(self);
            check.connect_toggled(move |b| {
                if b.is_active() {
                    if let Some(s) = w.upgrade() {
                        s.on_settings_changed();
                    }
                }
            });
        }
        let w = Rc::downgrade(self);
        self.proposals.connect_row_activated(move |_, row| {
            if let Some(s) = w.upgrade() {
                let p = s.proposal_data.borrow().get(row.index() as usize).copied();
                if let Some(p) = p {
                    s.edit(|y| y.apply(&p));
                }
            }
        });
    }

    fn snapshot(&self) -> Option<Snapshot> {
        let page = self.page.upgrade()?;
        page.with_contest(|c| {
            let stage = c.schedule.stage(self.index)?.as_any().downcast_ref::<YouthTraining>()?;
            let label = |r: PlayerRef| {
                c.players
                    .get(r)
                    .map(|p| {
                        let rank = p.attr("ranking").map(|v| v.as_uint().to_string()).unwrap_or_default();
                        let first = p.str_attr("first_name").unwrap_or_default();
                        format!("{rank}. {} {first}", p.name())
                    })
                    .unwrap_or_default()
            };
            Some(Snapshot {
                settings: stage.settings,
                locked: stage.base().locked,
                proposals: stage.proposals(),
                groups: stage.groups().into_iter().map(|g| (g.pool_sizes, g.players.iter().map(|r| label(*r)).collect())).collect(),
                count: stage.base().attendees.len(),
            })
        })
        .flatten()
    }

    fn edit(&self, f: impl FnOnce(&mut YouthTraining)) {
        let Some(page) = self.page.upgrade() else { return };
        let index = self.index;
        page.with_contest(|c| {
            if let Some(stage) = c.schedule.stage_mut(index).and_then(|s| s.as_any_mut().downcast_mut::<YouthTraining>()) {
                if !stage.base().locked {
                    f(stage);
                }
            }
        });
        page.mark_dirty();
        self.refresh();
    }

    fn on_settings_changed(&self) {
        if self.updating.get() {
            return;
        }
        let settings = YouthSettings {
            group_size: self.group_size.value_as_int().max(1) as u32,
            pools_per_group: self.pools.value_as_int().max(1) as u32,
            group_mode: if self.ranking.is_active() { GroupMode::Ranking } else { GroupMode::Division },
            pool_mode: if self.level.is_active() { PoolMode::Level } else { PoolMode::Snake },
        };
        self.edit(|y| y.settings = settings);
    }

    fn apply(&self, s: Snapshot) {
        self.updating.set(true);
        self.group_size.set_value(s.settings.group_size as f64);
        self.pools.set_value(s.settings.pools_per_group as f64);
        self.division.set_active(s.settings.group_mode == GroupMode::Division);
        self.ranking.set_active(s.settings.group_mode == GroupMode::Ranking);
        self.snake.set_active(s.settings.pool_mode == PoolMode::Snake);
        self.level.set_active(s.settings.pool_mode == PoolMode::Level);
        for w in [
            self.group_size.upcast_ref::<gtk::Widget>(),
            self.pools.upcast_ref(),
            self.division.upcast_ref(),
            self.ranking.upcast_ref(),
            self.snake.upcast_ref(),
            self.level.upcast_ref(),
            self.proposals.upcast_ref(),
        ] {
            w.set_sensitive(!s.locked);
        }

        // Résumé et avertissements
        let bad_pool = s.groups.iter().flat_map(|(sizes, _)| sizes.iter()).any(|p| *p < youth::MIN_POOL_SIZE || *p > youth::MAX_POOL_SIZE);
        let mut text = format!("{} {} → {} {}", s.count, tr("fencers"), s.groups.len(), tr("groups"));
        if let Some((sizes, players)) = s.groups.first() {
            text.push_str(&format!(
                " · {} {}",
                youth::bouts_per_fencer(players.len() as u32, sizes.len() as u32),
                tr("bouts per fencer")
            ));
        }
        if bad_pool {
            text.push_str(&format!(
                "\n⚠ {}",
                tr("Some pools have fewer than 3 or more than 6 fencers: choose another arrangement.")
            ));
        }
        self.summary.set_text(&text);

        // Propositions : la meilleure en premier, la courante cochée
        while let Some(row) = self.proposals.row_at_index(0) {
            self.proposals.remove(&row);
        }
        for (i, p) in s.proposals.iter().take(8).enumerate() {
            let current = p.group_size == s.settings.group_size && p.pools_per_group == s.settings.pools_per_group;
            let mut text = describe(p);
            if i == 0 {
                text = format!("★ {text}   ({})", tr("recommended"));
            }
            let label = gtk::Label::new(None);
            label.set_markup(&if current {
                format!("<b>{}</b>  ✔", gtk::glib::markup_escape_text(&text))
            } else {
                gtk::glib::markup_escape_text(&text).to_string()
            });
            label.set_xalign(0.0);
            label.set_margin_top(4);
            label.set_margin_bottom(4);
            label.set_margin_start(8);
            let row = gtk::ListBoxRow::new();
            row.set_child(Some(&label));
            row.set_activatable(true);
            self.proposals.append(&row);
        }
        *self.proposal_data.borrow_mut() = s.proposals.iter().take(8).copied().collect();

        // Groupes
        while let Some(child) = self.groups.first_child() {
            self.groups.remove(&child);
        }
        for (i, (sizes, players)) in s.groups.iter().enumerate() {
            let frame = gtk::Frame::new(Some(&format!(
                "{} {} — {} · {}",
                tr("Group"),
                i + 1,
                players.len(),
                sizes.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(" + ")
            )));
            let list = gtk::Box::new(gtk::Orientation::Vertical, 2);
            list.set_margin_top(4);
            list.set_margin_bottom(4);
            list.set_margin_start(8);
            list.set_margin_end(8);
            for name in players {
                let l = gtk::Label::new(Some(name));
                l.set_xalign(0.0);
                list.append(&l);
            }
            frame.set_child(Some(&list));
            self.groups.insert(&frame, -1);
        }
        self.updating.set(false);
    }
}

impl StageScreen for YouthScreen {
    fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }

    fn refresh(&self) {
        if let Some(s) = self.snapshot() {
            self.apply(s);
        }
    }
}
