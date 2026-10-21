//! Entraînement des jeunes.
//!
//! - Préférences de la phase (`configuration`) : réglages par défaut —
//!   taille des groupes, poules par groupe, constitution des groupes et des
//!   poules, décalage, places tirées.
//! - Écran de la phase : création des groupes, une fois l'appel fait. Chaque
//!   groupe peut changer de taille et de formule de poules (la meilleure est
//!   proposée, comme pour la composition des poules). « Tour suivant » crée
//!   une compétition par groupe.

use std::cell::Cell;
use std::rc::{Rc, Weak};

use bp_core::attribute;
use bp_core::i18n::tr;
use bp_core::player::PlayerRef;
use bp_core::rounds::pool::round::regular_swapping_criteria;
use bp_core::rounds::table::supervisor::FencedPlaces;
use bp_core::rounds::youth::{self, GroupLayout, GroupMode, PoolMode, YouthSettings, YouthTraining};
use bp_core::stage::Stage;
use gtk::glib;
use gtk::prelude::*;
use gtk4 as gtk;

use super::StageScreen;
use crate::contest_page::ContestPage;

struct Snapshot {
    settings: YouthSettings,
    locked: bool,
    customized: bool,
    count: usize,
    /// Composition, et pour chaque groupe « rang. NOM Prénom ».
    groups: Vec<(GroupLayout, Vec<String>)>,
}

/// Réglages (préférences de la phase).
struct SettingsWidgets {
    root: gtk::Box,
    group_size: gtk::SpinButton,
    pools: gtk::SpinButton,
    division: gtk::CheckButton,
    ranking: gtk::CheckButton,
    random: gtk::CheckButton,
    swapping: Vec<(String, gtk::CheckButton)>,
    snake: gtk::CheckButton,
    level: gtk::CheckButton,
    all_places: gtk::CheckButton,
    third_place: gtk::CheckButton,
    no_place: gtk::CheckButton,
}

pub struct YouthScreen {
    page: Weak<ContestPage>,
    weak_self: Weak<Self>,
    index: usize,
    root: gtk::Box,
    settings: SettingsWidgets,
    summary: gtk::Label,
    group_count: gtk::SpinButton,
    reset: gtk::Button,
    groups: gtk::Box,
    updating: Cell<bool>,
    refresh_pending: Cell<bool>,
}

fn bold(title: &str) -> gtk::Label {
    let l = gtk::Label::new(None);
    l.set_markup(&format!("<b>{}</b>", glib::markup_escape_text(title)));
    l.set_xalign(0.0);
    l.set_margin_top(8);
    l
}

fn left(text: &str) -> gtk::Label {
    let l = gtk::Label::new(Some(text));
    l.set_xalign(0.0);
    l
}

/// « 2 poules de 4 et 5 » / « 1 poule de 8 ».
fn pools_label(size: u32, nb_pools: u32) -> String {
    let sizes = youth::pool_sizes(size, nb_pools);
    let (largest, smallest) = (sizes[0], *sizes.last().unwrap_or(&sizes[0]));
    let of = if nb_pools > 1 { tr("pools of") } else { tr("pool of") };
    if largest == smallest {
        format!("{nb_pools} {of} {largest}")
    } else {
        format!("{nb_pools} {of} {smallest} {} {largest}", tr("and"))
    }
}

impl SettingsWidgets {
    fn new() -> SettingsWidgets {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 4);
        root.set_margin_top(8);
        root.set_margin_bottom(8);
        root.set_margin_start(8);
        root.set_margin_end(8);
        root.append(&bold(&tr("Youth training")));

        let grid = gtk::Grid::new();
        grid.set_row_spacing(4);
        grid.set_column_spacing(12);
        let group_size = gtk::SpinButton::with_range(4.0, 24.0, 1.0);
        let pools = gtk::SpinButton::with_range(1.0, 6.0, 1.0);
        grid.attach(&left(&tr("Fencers per group")), 0, 0, 1, 1);
        grid.attach(&group_size, 1, 0, 1, 1);
        grid.attach(&left(&tr("Pools per group")), 0, 1, 1, 1);
        grid.attach(&pools, 1, 1, 1, 1);
        let hint = left(&tr("Preferences: the number of fencers per group is adjusted to the number of entries."));
        hint.add_css_class("dim-label");
        hint.set_wrap(true);
        grid.attach(&hint, 0, 2, 2, 1);
        root.append(&grid);

        root.append(&bold(&tr("Groups")));
        let division = gtk::CheckButton::with_label(&tr("By division (level groups: the best fencers in group 1)"));
        let ranking = gtk::CheckButton::with_label(&tr("By ranking (serpentine, groups of equal level)"));
        let random = gtk::CheckButton::with_label(&tr("At random"));
        ranking.set_group(Some(&division));
        random.set_group(Some(&division));
        for w in [&division, &ranking, &random] {
            root.append(w);
        }
        let swap_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        swap_box.set_margin_start(24);
        swap_box.append(&left(&tr("Swapping")));
        let mut swapping = Vec::new();
        for code in regular_swapping_criteria() {
            let check = gtk::CheckButton::with_label(&tr(attribute::desc(code).user_name));
            swap_box.append(&check);
            swapping.push((code.to_string(), check));
        }
        root.append(&swap_box);

        root.append(&bold(&tr("Pools of a group")));
        let snake = gtk::CheckButton::with_label(&tr("Serpentine (1-4-5-8 / 2-3-6-7)"));
        let level = gtk::CheckButton::with_label(&tr("By level (the best fencers of the group in pool 1)"));
        level.set_group(Some(&snake));
        root.append(&snake);
        root.append(&level);

        root.append(&bold(&tr("Table")));
        let crossing = left(&tr("1st of pool 1 against last of pool 2, 2nd against second to last…"));
        crossing.set_wrap(true);
        root.append(&crossing);
        let all_places = gtk::CheckButton::with_label(&tr("All places fenced"));
        let third_place = gtk::CheckButton::with_label(&tr("Fence off for third place"));
        let no_place = gtk::CheckButton::with_label(&tr("No fence off for third place"));
        third_place.set_group(Some(&all_places));
        no_place.set_group(Some(&all_places));
        for w in [&all_places, &third_place, &no_place] {
            root.append(w);
        }

        SettingsWidgets {
            root,
            group_size,
            pools,
            division,
            ranking,
            random,
            swapping,
            snake,
            level,
            all_places,
            third_place,
            no_place,
        }
    }

    fn read(&self) -> YouthSettings {
        YouthSettings {
            group_size: self.group_size.value_as_int().max(1) as u32,
            pools_per_group: self.pools.value_as_int().max(1) as u32,
            group_mode: if self.ranking.is_active() {
                GroupMode::Ranking
            } else if self.random.is_active() {
                GroupMode::Random
            } else {
                GroupMode::Division
            },
            pool_mode: if self.level.is_active() { PoolMode::Level } else { PoolMode::Snake },
            swapping: self.swapping.iter().filter(|(_, c)| c.is_active()).map(|(code, _)| code.clone()).collect(),
            fenced_places: if self.third_place.is_active() {
                FencedPlaces::ThirdPlaces
            } else if self.no_place.is_active() {
                FencedPlaces::None
            } else {
                FencedPlaces::AllPlaces
            },
        }
    }

    fn show(&self, s: &YouthSettings, locked: bool) {
        self.group_size.set_value(s.group_size as f64);
        self.pools.set_value(s.pools_per_group as f64);
        self.division.set_active(s.group_mode == GroupMode::Division);
        self.ranking.set_active(s.group_mode == GroupMode::Ranking);
        self.random.set_active(s.group_mode == GroupMode::Random);
        for (code, check) in &self.swapping {
            check.set_active(s.swapping.contains(code));
        }
        self.snake.set_active(s.pool_mode == PoolMode::Snake);
        self.level.set_active(s.pool_mode == PoolMode::Level);
        self.all_places.set_active(s.fenced_places == FencedPlaces::AllPlaces);
        self.third_place.set_active(s.fenced_places == FencedPlaces::ThirdPlaces);
        self.no_place.set_active(matches!(s.fenced_places, FencedPlaces::None | FencedPlaces::Quota));
        self.root.set_sensitive(!locked);
    }
}

impl YouthScreen {
    pub fn new(page: &Rc<ContestPage>, index: usize) -> Rc<Self> {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 6);
        root.set_margin_top(12);
        root.set_margin_bottom(12);
        root.set_margin_start(12);
        root.set_margin_end(12);

        let intro = left(&tr(
            "Groups creation. Each group becomes its own competition (pools, then table). Settings are in the preferences of this round. Click \"Next round\" to create the groups.",
        ));
        intro.set_wrap(true);
        root.append(&intro);

        let bar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let summary = left("");
        summary.set_hexpand(true);
        bar.append(&summary);
        bar.append(&left(&tr("Number of groups")));
        let group_count = gtk::SpinButton::with_range(1.0, 50.0, 1.0);
        bar.append(&group_count);
        let reset = gtk::Button::with_label(&tr("Default arrangement"));
        bar.append(&reset);
        root.append(&bar);

        let groups = gtk::Box::new(gtk::Orientation::Vertical, 8);
        let scroll = gtk::ScrolledWindow::new();
        scroll.set_child(Some(&groups));
        scroll.set_vexpand(true);
        root.append(&scroll);

        let screen = Rc::new_cyclic(|weak| YouthScreen {
            page: Rc::downgrade(page),
            weak_self: weak.clone(),
            index,
            root,
            settings: SettingsWidgets::new(),
            summary,
            group_count,
            reset,
            groups,
            updating: Cell::new(false),
            refresh_pending: Cell::new(false),
        });
        screen.connect();
        screen.refresh();
        screen
    }

    fn connect(self: &Rc<Self>) {
        let on_settings = {
            let w = Rc::downgrade(self);
            move || {
                if let Some(s) = w.upgrade() {
                    s.on_settings_changed();
                }
            }
        };
        for spin in [&self.settings.group_size, &self.settings.pools] {
            let f = on_settings.clone();
            spin.connect_value_changed(move |_| f());
        }
        let checks: Vec<&gtk::CheckButton> = [
            &self.settings.division,
            &self.settings.ranking,
            &self.settings.random,
            &self.settings.snake,
            &self.settings.level,
            &self.settings.all_places,
            &self.settings.third_place,
            &self.settings.no_place,
        ]
        .into_iter()
        .chain(self.settings.swapping.iter().map(|(_, c)| c))
        .collect();
        for check in checks {
            let f = on_settings.clone();
            check.connect_toggled(move |_| f());
        }
        let w = Rc::downgrade(self);
        self.group_count.connect_value_changed(move |spin| {
            if let Some(s) = w.upgrade() {
                if !s.updating.get() {
                    let n = spin.value_as_int().max(1) as u32;
                    s.edit(|y| y.set_group_count(n));
                }
            }
        });
        let w = Rc::downgrade(self);
        self.reset.connect_clicked(move |_| {
            if let Some(s) = w.upgrade() {
                s.edit(|y| y.reset_layout());
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
                        let club = p.str_attr("club").unwrap_or_default();
                        format!("{rank}. {} {first}   {club}", p.name())
                    })
                    .unwrap_or_default()
            };
            Some(Snapshot {
                settings: stage.settings.clone(),
                locked: stage.base().locked,
                customized: stage.is_customized(),
                count: stage.base().attendees.len(),
                groups: stage.groups(&c.players).into_iter().map(|g| (g.layout, g.players.iter().map(|r| label(*r)).collect())).collect(),
            })
        })
        .flatten()
    }

    /// Modifie la phase puis redessine (en différé : le widget qui a émis
    /// le signal peut être détruit par le rafraîchissement).
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
        self.queue_refresh();
    }

    fn queue_refresh(&self) {
        if self.refresh_pending.replace(true) {
            return;
        }
        let weak = self.weak_self.clone();
        glib::idle_add_local_once(move || {
            if let Some(s) = weak.upgrade() {
                s.refresh_pending.set(false);
                s.refresh();
            }
        });
    }

    fn on_settings_changed(&self) {
        if self.updating.get() {
            return;
        }
        let settings = self.settings.read();
        self.edit(|y| {
            y.settings = settings;
            y.reset_layout();
        });
    }

    fn group_widget(&self, g: usize, layout: GroupLayout, players: &[String], locked: bool, single: bool, settings: &YouthSettings) -> gtk::Widget {
        let fenced = settings.fenced_places;
        let frame = gtk::Frame::new(None);
        let content = gtk::Box::new(gtk::Orientation::Vertical, 4);
        content.set_margin_top(6);
        content.set_margin_bottom(6);
        content.set_margin_start(8);
        content.set_margin_end(8);

        let line = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        line.append(&bold(&format!("{} {}", tr("Group"), g + 1)));
        line.append(&left(&tr("Fencers")));
        let size = gtk::SpinButton::with_range(youth::MIN_POOL_SIZE as f64, 64.0, 1.0);
        size.set_value(layout.size as f64);
        size.set_sensitive(!locked && !single);
        {
            let w = self.weak_self.clone();
            size.connect_value_changed(move |spin| {
                if let Some(s) = w.upgrade() {
                    if !s.updating.get() {
                        let v = spin.value_as_int().max(1) as u32;
                        s.edit(|y| y.set_group_size(g, v));
                    }
                }
            });
        }
        line.append(&size);
        line.append(&left(&tr("Pools")));
        let combo = gtk::ComboBoxText::new();
        let best = youth::best_pools(layout.size, settings);
        let choices = youth::pool_choices(layout.size);
        for c in &choices {
            let mut text = pools_label(layout.size, c.nb_pools);
            if c.nb_pools == best {
                text = format!("★ {text}");
            }
            combo.append(Some(&c.nb_pools.to_string()), &text);
        }
        combo.set_active_id(Some(&layout.nb_pools.to_string()));
        combo.set_sensitive(!locked);
        {
            let w = self.weak_self.clone();
            combo.connect_changed(move |c| {
                if let (Some(s), Some(id)) = (w.upgrade(), c.active_id()) {
                    if !s.updating.get() {
                        let n = id.parse::<u32>().unwrap_or(1);
                        s.edit(|y| y.set_group_pools(g, n));
                    }
                }
            });
        }
        line.append(&combo);
        let bouts = left(&format!(
            "{} {}",
            youth::bouts_per_fencer(&layout, fenced),
            if fenced == FencedPlaces::AllPlaces { tr("bouts per fencer") } else { tr("bouts minimum per fencer") }
        ));
        bouts.add_css_class("dim-label");
        line.append(&bouts);
        content.append(&line);

        let names = gtk::FlowBox::new();
        names.set_selection_mode(gtk::SelectionMode::None);
        names.set_max_children_per_line(4);
        names.set_min_children_per_line(2);
        for n in players {
            names.insert(&left(n), -1);
        }
        content.append(&names);
        frame.set_child(Some(&content));
        frame.upcast()
    }

    fn apply(&self, s: Snapshot) {
        self.updating.set(true);
        self.settings.show(&s.settings, s.locked);

        let mut text = format!("{} {} → {} {}", s.count, tr("fencers"), s.groups.len(), tr("groups"));
        if s.customized {
            text.push_str(&format!(" ({})", tr("modified")));
        }
        self.summary.set_text(&text);
        self.group_count.set_value(s.groups.len() as f64);
        self.group_count.set_sensitive(!s.locked);
        self.reset.set_sensitive(!s.locked && s.customized);

        while let Some(child) = self.groups.first_child() {
            self.groups.remove(&child);
        }
        let single = s.groups.len() < 2;
        for (g, (layout, players)) in s.groups.iter().enumerate() {
            self.groups.append(&self.group_widget(g, *layout, players, s.locked, single, &s.settings));
        }
        self.updating.set(false);
    }
}

impl StageScreen for YouthScreen {
    fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }

    fn configuration(&self) -> Option<gtk::Widget> {
        Some(self.settings.root.clone().upcast())
    }

    fn refresh(&self) {
        if let Some(s) = self.snapshot() {
            self.apply(s);
        }
    }
}
