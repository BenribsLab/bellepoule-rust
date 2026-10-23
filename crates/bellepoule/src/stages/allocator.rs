//! Écran de la composition des poules (port de `pool_allocator.cpp`, parties
//! interface) : formules, poules, glisser-déposer des tireurs, critères de
//! séparation, liste des tireurs.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use bp_core::attribute;
use bp_core::filter::Filter;
use bp_core::i18n::tr;
use bp_core::player::PlayerRef;
use bp_core::rounds::pool::round::{
    extra_swapping_criteria, regular_swapping_criteria, FilterKind, PillowKind,
};
use bp_core::rounds::pool::PoolAllocatorStage;
use bp_core::stage::Stage;
use bp_gtk::dialogs;
use bp_gtk::glade::Glade;
use gtk::gdk;
use gtk::glib;
use gtk::prelude::*;
use gtk4 as gtk;

use super::StageScreen;
use crate::contest_page::ContestPage;
use crate::widgets::filter_dialog;
use crate::widgets::players_list::{Cell as ListCell, PlayersListView, RowData};
use crate::widgets::render;

mod print;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mark {
    None,
    Moved,
    Error,
}

struct FencerLine {
    player: PlayerRef,
    cells: Vec<ListCell>,
    mark: Mark,
    absent: bool,
}

struct PoolView {
    name: String,
    balanced: bool,
    piste: Option<String>,
    time: Option<String>,
    referees: Vec<Vec<ListCell>>,
    fencers: Vec<FencerLine>,
}

struct Snapshot {
    /// (taille, nombre de poules) de chaque formule.
    configs: Vec<(String, String)>,
    selected: Option<usize>,
    best: Option<usize>,
    pools: Vec<PoolView>,
    locked: bool,
    /// Entraînement des jeunes : poules fixées groupe par groupe.
    youth: bool,
    swap_errors: usize,
    criteria: Vec<String>,
    adjustable: bool,
    list_rows: Vec<RowData>,
    list_specs: Vec<crate::widgets::players_list::ColumnSpec>,
}

pub struct AllocatorScreen {
    weak_self: Weak<Self>,
    page: Weak<ContestPage>,
    index: usize,
    glade: Glade,
    root: gtk::Widget,
    pools_box: gtk::Grid,
    list: PlayersListView,
    regular: RefCell<Vec<(String, gtk::CheckButton)>>,
    extra_codes: Vec<String>,
    updating: Cell<bool>,
}

impl AllocatorScreen {
    pub fn new(page: &Rc<ContestPage>, index: usize) -> Rc<AllocatorScreen> {
        let glade = Glade::new("pool_allocator.ui").expect("pool_allocator.ui");
        let root: gtk::Widget = glade.get("root");
        Glade::detach_from_parent(&root);

        let scrolled: gtk::ScrolledWindow = glade.get("canvas_scrolled_window");
        let pools_box = gtk::Grid::new();
        pools_box.set_row_spacing(24);
        pools_box.set_column_spacing(48);
        pools_box.set_margin_top(12);
        pools_box.set_margin_bottom(12);
        pools_box.set_margin_start(12);
        pools_box.set_margin_end(12);
        pools_box.set_valign(gtk::Align::Start);
        scrolled.set_child(Some(&pools_box));
        scrolled.set_hexpand(true);
        scrolled.set_vexpand(true);
        scrolled.set_min_content_width(300);

        // Liste des tireurs (masquée tant que le bouton n'est pas enfoncé)
        let list_scrolled = gtk::ScrolledWindow::new();
        list_scrolled.set_min_content_width(380);
        list_scrolled.set_vexpand(true);
        let hook: gtk::Viewport = glade.get("fencer_list_hook");
        hook.set_child(Some(&list_scrolled));
        hook.set_visible(false);
        hook.set_hexpand(false);
        let list = PlayersListView::in_scrolled(list_scrolled);

        // Critères de séparation « réguliers »
        let regular_box: gtk::Box = glade.get("regular_swapping_hbox");
        let mut regular = Vec::new();
        for code in regular_swapping_criteria() {
            let desc = attribute::desc(code);
            let check = gtk::CheckButton::with_label(&tr(desc.user_name));
            check.set_margin_end(6);
            regular_box.append(&check);
            regular.push((code.to_string(), check));
        }
        // Critère supplémentaire
        let extra_store: gtk::ListStore = glade.get("swapping_liststore");
        let mut extra_codes = Vec::new();
        for code in extra_swapping_criteria() {
            let desc = attribute::desc(code);
            let iter = extra_store.append();
            extra_store.set(&iter, &[(1, &tr(desc.user_name))]);
            extra_codes.push(code.to_string());
        }
        let extra_combo: gtk::ComboBox = glade.get("extra_combobox");
        extra_combo.set_active(Some(0));

        let screen = Rc::new_cyclic(|weak_self| AllocatorScreen {
            weak_self: weak_self.clone(),
            page: Rc::downgrade(page),
            index,
            glade,
            root,
            pools_box,
            list,
            regular: RefCell::new(regular),
            extra_codes,
            updating: Cell::new(false),
        });
        screen.connect();
        screen.refresh();
        screen
    }

    fn page(&self) -> Option<Rc<ContestPage>> {
        self.page.upgrade()
    }

    fn connect(self: &Rc<Self>) {
        macro_rules! on {
            ($name:expr, $method:ident) => {{
                let w = Rc::downgrade(self);
                self.glade.connect_simple($name, move |_| {
                    if let Some(s) = w.upgrade() {
                        s.$method();
                    }
                });
            }};
        }
        on!("on_nb_pools_combobox_changed", on_nb_pools_changed);
        on!("on_pool_size_combobox_changed", on_pool_size_changed);
        on!("on_filter_button_clicked", on_filter);
        on!("on_print_toolbutton_clicked", on_print);
        on!("on_fencer_list_toggled", on_fencer_list_toggled);
        on!("on_latecomer_toolbutton_clicked", on_latecomer);
        on!("on_absent_toolbutton_clicked", on_absent);

        for (_, check) in self.regular.borrow().iter() {
            let w = Rc::downgrade(self);
            check.connect_toggled(move |_| {
                if let Some(s) = w.upgrade() {
                    s.on_swapping_changed();
                }
            });
        }
        let extra_toggle: gtk::CheckButton = self.glade.get("extra_toggle");
        {
            let w = Rc::downgrade(self);
            extra_toggle.connect_toggled(move |_| {
                if let Some(s) = w.upgrade() {
                    s.on_swapping_changed();
                }
            });
        }
        let extra_combo: gtk::ComboBox = self.glade.get("extra_combobox");
        {
            let w = Rc::downgrade(self);
            extra_combo.connect_changed(move |_| {
                if let Some(s) = w.upgrade() {
                    s.on_swapping_changed();
                }
            });
        }
    }

    // ----------------------------------------------------------------------
    // Contenu

    fn stage<R>(
        c: &mut bp_core::contest::Contest,
        index: usize,
        f: impl FnOnce(&PoolAllocatorStage, &bp_core::contest::Contest) -> R,
    ) -> Option<R> {
        let c: &bp_core::contest::Contest = c;
        let stage = c.schedule.stage(index)?;
        let allocator = stage.as_any().downcast_ref::<PoolAllocatorStage>()?;
        Some(f(allocator, c))
    }

    fn snapshot(c: &mut bp_core::contest::Contest, index: usize) -> Option<Snapshot> {
        let team_event = c.settings.team_event;
        Self::stage(c, index, |allocator, c| {
            let round = allocator.round();
            let store = &c.players;
            let owner = round.allocator_owner();
            let locked = allocator.base().locked;

            let mut filter = Filter::including("allocator", &FilterKind::Allocator.available());
            filter.apply_list(&round.allocator_filter);
            let specs = render::column_specs(filter.layout(), false);
            let image = |p: Option<&bp_core::player::Player>| -> Vec<ListCell> {
                p.map(|p| {
                    specs
                        .iter()
                        .map(|spec| render::cell_for(p, spec, owner))
                        .collect()
                })
                .unwrap_or_default()
            };

            let configs = round
                .configs()
                .iter()
                .map(|c| (c.size_label(), c.nb_pool.to_string()))
                .collect();
            let pools = round
                .pools()
                .iter()
                .enumerate()
                .map(|(i, pool)| PoolView {
                    name: pool.name().to_string(),
                    balanced: round.pool_is_balanced(i),
                    piste: pool.piste_label(),
                    time: pool.start_time_image(),
                    referees: pool
                        .referees()
                        .iter()
                        .map(|r| image(c.referee(*r)))
                        .collect(),
                    fencers: pool
                        .fencers()
                        .iter()
                        .map(|f| {
                            let absent = store
                                .get(*f)
                                .and_then(|p| p.str_attr("incident"))
                                .map(|s| s.starts_with('A'))
                                .unwrap_or(false);
                            let mark = if round.swapper().is_an_error(*f) {
                                Mark::Error
                            } else if round.is_moved(store, *f) {
                                Mark::Moved
                            } else {
                                Mark::None
                            };
                            FencerLine {
                                player: *f,
                                cells: image(store.get(*f)),
                                mark,
                                absent,
                            }
                        })
                        .collect(),
                })
                .collect();

            // Liste des tireurs
            let mut list_filter =
                Filter::including("allocator_list", &FilterKind::List.available());
            list_filter.apply_list(&round.list_filter);
            let list_specs = render::column_specs(list_filter.layout(), false);
            let list_rows = round
                .attendees()
                .iter()
                .filter_map(|r| {
                    let p = store.get(*r)?;
                    Some(RowData {
                        player: *r,
                        cells: list_specs
                            .iter()
                            .map(|s| render::cell_for(p, s, owner))
                            .collect(),
                        sort: render::sort_values(p, &list_specs, owner),
                        children: Vec::new(),
                        dimmed: false,
                    })
                })
                .collect();

            Snapshot {
                youth: round.youth_groups.is_some(),
                configs,
                selected: round.selected_config(),
                best: round.best_config(),
                pools,
                locked,
                swap_errors: round.swapper().moved(),
                criteria: round.swapping_criteria().to_vec(),
                adjustable: !team_event && allocator.base().id == 1,
                list_rows,
                list_specs,
            }
        })
    }

    fn apply(&self, s: Snapshot) {
        self.updating.set(true);

        // Formules
        let store: gtk::ListStore = self.glade.get("combo_liststore");
        store.clear();
        for (i, (size, nb)) in s.configs.iter().enumerate() {
            let iter = store.append();
            let icon = if Some(i) == s.best { "help-about-symbolic" } else { "" };
            store.set(&iter, &[(0, size), (1, nb), (2, &icon)]);
        }
        let nb_combo: gtk::ComboBox = self.glade.get("nb_pools_combobox");
        let size_combo: gtk::ComboBox = self.glade.get("pool_size_combobox");
        nb_combo.set_active(s.selected.map(|i| i as u32));
        size_combo.set_active(s.selected.map(|i| i as u32));
        nb_combo.set_sensitive(!s.locked && !s.youth);
        size_combo.set_sensitive(!s.locked && !s.youth);
        for id in ["extra_toggle", "extra_combobox", "regular_swapping_hbox"] {
            self.glade.get::<gtk::Widget>(id).set_sensitive(!s.locked);
        }
        for id in ["latecomer_toolbutton", "absent_toolbutton"] {
            self.glade.get::<gtk::Widget>(id).set_visible(s.adjustable);
            self.glade.get::<gtk::Widget>(id).set_sensitive(!s.locked);
        }

        // Critères de séparation
        for (code, check) in self.regular.borrow().iter() {
            check.set_active(s.criteria.contains(code));
        }
        {
            let extra_toggle: gtk::CheckButton = self.glade.get("extra_toggle");
            let extra_combo: gtk::ComboBox = self.glade.get("extra_combobox");
            let extra = s.criteria.iter().find(|c| self.extra_codes.contains(c));
            extra_toggle.set_active(extra.is_some());
            if let Some(i) = extra.and_then(|c| self.extra_codes.iter().position(|e| e == c)) {
                extra_combo.set_active(Some(i as u32));
            }
        }
        let tooltip = format!("{} {}", s.swap_errors, tr("error(s)"));
        let swapping_box: gtk::Widget = self.glade.get("swapping_box");
        swapping_box.set_tooltip_text(Some(&tooltip));
        if s.swap_errors > 0 {
            swapping_box.add_css_class("bp-swap-error");
        } else {
            swapping_box.remove_css_class("bp-swap-error");
        }

        // Poules
        while let Some(child) = self.pools_box.first_child() {
            self.pools_box.remove(&child);
        }
        let grid = &self.pools_box;
        for (i, pool) in s.pools.iter().enumerate() {
            let widget = self.pool_widget(pool, s.locked);
            grid.attach(&widget, (i % 2) as i32, (i / 2) as i32, 1, 1);
        }

        // Liste des tireurs
        self.list.set_columns(s.list_specs);
        self.list.set_rows(s.list_rows);

        self.updating.set(false);
    }

    fn pool_widget(&self, pool: &PoolView, locked: bool) -> gtk::Widget {
        let frame = gtk::Box::new(gtk::Orientation::Vertical, 4);
        frame.add_css_class("bp-pool");

        let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let icon = gtk::Image::from_icon_name(if pool.balanced {
            "object-select-symbolic"
        } else {
            "dialog-error-symbolic"
        });
        header.append(&icon);
        let name = gtk::Label::new(None);
        name.set_markup(&format!(
            "<span size='large' weight='bold'>{}</span>",
            glib::markup_escape_text(&pool.name)
        ));
        header.append(&name);
        for text in [pool.piste.as_ref(), pool.time.as_ref()]
            .into_iter()
            .flatten()
        {
            let l = gtk::Label::new(None);
            l.set_markup(&format!(
                "<span foreground='darkgreen' style='italic' weight='bold'>{}</span>",
                glib::markup_escape_text(text)
            ));
            header.append(&l);
        }
        frame.append(&header);

        let grid = gtk::Grid::new();
        grid.set_column_spacing(10);
        grid.set_row_spacing(2);
        let mut row = 0;
        for referee in &pool.referees {
            let icon = gtk::Image::from_file(bp_gtk::resources::image_path("referee.png"));
            icon.set_pixel_size(16);
            grid.attach(&icon, 0, row, 1, 1);
            for (c, cell) in referee.iter().enumerate() {
                grid.attach(
                    &Self::cell_widget(cell, false, true),
                    c as i32 + 1,
                    row,
                    1,
                    1,
                );
            }
            row += 1;
        }
        for fencer in &pool.fencers {
            let mark = match fencer.mark {
                Mark::Error => Some("dialog-warning-symbolic"),
                Mark::Moved => Some("go-jump-symbolic"),
                Mark::None => None,
            };
            let status = gtk::Image::new();
            if let Some(m) = mark {
                status.set_icon_name(Some(m));
            }
            status.set_pixel_size(16);
            grid.attach(&status, 0, row, 1, 1);
            let mut labels = Vec::new();
            for (c, cell) in fencer.cells.iter().enumerate() {
                let l = Self::cell_widget(cell, fencer.absent, false);
                grid.attach(&l, c as i32 + 1, row, 1, 1);
                labels.push(l);
            }
            if !locked {
                for l in &labels {
                    let source = gtk::DragSource::new();
                    source.set_actions(gdk::DragAction::MOVE);
                    source.set_content(Some(&gdk::ContentProvider::for_value(
                        &fencer.player.to_value(),
                    )));
                    l.add_controller(source);
                }
            }
            row += 1;
        }
        frame.append(&grid);

        if !locked {
            let target = gtk::DropTarget::new(PlayerRef::static_type(), gdk::DragAction::MOVE);
            let w = self.weak_self.clone();
            let name = pool.name.clone();
            target.connect_drop(move |_, value, _, _| {
                let Ok(player) = value.get::<PlayerRef>() else {
                    return false;
                };
                let Some(s) = w.upgrade() else { return false };
                let name = name.clone();
                // le widget source est détruit par le rafraîchissement : on diffère
                glib::idle_add_local_once(move || s.on_drop(player, &name));
                true
            });
            frame.add_controller(target);
        }
        frame.upcast()
    }

    fn cell_widget(cell: &ListCell, absent: bool, referee: bool) -> gtk::Widget {
        match cell {
            ListCell::Icon(pixbuf) => gtk::Image::from_pixbuf(pixbuf.as_ref()).upcast(),
            _ => {
                let text = match cell {
                    ListCell::Text(s) => s.clone(),
                    ListCell::Toggle(b) => {
                        if *b {
                            "✓".into()
                        } else {
                            String::new()
                        }
                    }
                    ListCell::Progress(n) => n.to_string(),
                    _ => unreachable!(),
                };
                let label = gtk::Label::new(None);
                let escaped = glib::markup_escape_text(&text);
                label.set_markup(&if absent {
                    format!("<s>{escaped}</s>")
                } else if referee {
                    format!("<b><i>{escaped}</i></b>")
                } else {
                    escaped.to_string()
                });
                label.set_xalign(0.0);
                label.upcast()
            }
        }
    }

    pub fn refresh_now(self: &Rc<Self>) {
        let Some(page) = self.page() else { return };
        let index = self.index;
        if let Some(Some(snapshot)) = page.with_contest(|c| Self::snapshot(c, index)) {
            self.apply(snapshot);
        }
    }

    // ----------------------------------------------------------------------
    // Actions

    fn edit<R>(
        &self,
        f: impl FnOnce(
            &mut bp_core::rounds::pool::round::PoolRound,
            &mut bp_core::player::PlayerStore,
        ) -> R,
    ) -> Option<R> {
        let page = self.page()?;
        let index = self.index;
        let result = page
            .with_contest(|c| {
                c.with_schedule(|s, ctx| {
                    let stage = s.stage_mut(index)?;
                    let allocator = stage.as_any_mut().downcast_mut::<PoolAllocatorStage>()?;
                    if allocator.base().locked {
                        return None;
                    }
                    let mut round = allocator.round();
                    Some(f(&mut round, ctx.players))
                })
            })
            .flatten();
        if result.is_some() {
            page.mark_dirty();
        }
        result
    }

    fn after_edit(self: &Rc<Self>) {
        self.refresh_now();
        if let Some(page) = self.page() {
            if let Some(sv) = page.schedule.borrow().as_ref() {
                sv.refresh_sensitivity();
            }
        }
    }

    fn on_nb_pools_changed(self: &Rc<Self>) {
        if self.updating.get() {
            return;
        }
        let combo: gtk::ComboBox = self.glade.get("nb_pools_combobox");
        self.select_config(combo.active());
    }

    fn on_pool_size_changed(self: &Rc<Self>) {
        if self.updating.get() {
            return;
        }
        let combo: gtk::ComboBox = self.glade.get("pool_size_combobox");
        self.select_config(combo.active());
    }

    fn select_config(self: &Rc<Self>, index: Option<u32>) {
        let Some(i) = index else { return };
        self.edit(|round, players| round.select_config(players, i as usize));
        self.after_edit();
    }

    fn on_swapping_changed(self: &Rc<Self>) {
        if self.updating.get() {
            return;
        }
        let regular: Vec<String> = self
            .regular
            .borrow()
            .iter()
            .filter(|(_, check)| check.is_active())
            .map(|(code, _)| code.clone())
            .collect();
        let extra_toggle: gtk::CheckButton = self.glade.get("extra_toggle");
        let extra_combo: gtk::ComboBox = self.glade.get("extra_combobox");
        let extra: Option<String> = if extra_toggle.is_active() {
            extra_combo
                .active()
                .and_then(|i| self.extra_codes.get(i as usize).cloned())
        } else {
            None
        };
        self.edit(|round, players| {
            let regular: Vec<&str> = regular.iter().map(|s| s.as_str()).collect();
            round.set_swapping_criteria(players, &regular, extra.as_deref());
        });
        self.after_edit();
    }

    fn on_drop(self: &Rc<Self>, player: PlayerRef, pool_name: &str) {
        let to = self
            .page()
            .and_then(|p| {
                let index = self.index;
                p.with_contest(|c| {
                    Self::stage(c, index, |a, _| {
                        a.round()
                            .pools()
                            .iter()
                            .position(|pool| pool.name() == pool_name)
                    })
                })
            })
            .flatten()
            .flatten();
        if let Some(to) = to {
            self.edit(|round, players| {
                let _ = round.move_fencer(players, player, to);
                round.check_distribution(players);
            });
        }
        self.after_edit();
    }

    fn on_filter(self: &Rc<Self>) {
        let Some(page) = self.page() else { return };
        let Some(app) = page.app.upgrade() else {
            return;
        };
        let window = page.window();
        let list_visible = self
            .glade
            .get::<gtk::ToggleButton>("fencer_list")
            .is_active();
        let kind = if list_visible {
            FilterKind::List
        } else {
            FilterKind::Allocator
        };
        let current = page
            .with_contest(|c| {
                Self::stage(c, self.index, |a, _| {
                    if list_visible {
                        a.round().list_filter.clone()
                    } else {
                        a.round().allocator_filter.clone()
                    }
                })
            })
            .flatten();
        let Some(current) = current else { return };
        let mut filter = Filter::including(
            if list_visible {
                "allocator_list"
            } else {
                "allocator"
            },
            &kind.available(),
        );
        filter.apply_list(&current);
        let changed = {
            let mut t = app.tournament.borrow_mut();
            let mut config = std::mem::take(&mut t.config);
            let changed =
                filter_dialog::select_attributes(window.as_ref(), &mut filter, Some(&mut config));
            t.config = config;
            changed
        };
        if changed {
            let codes = filter.selected_codes();
            page.with_contest(|c| {
                Self::stage(c, self.index, |a, _| {
                    let mut round = a.round();
                    if list_visible {
                        round.list_filter = codes;
                    } else {
                        round.allocator_filter = codes;
                    }
                })
            });
            page.mark_dirty();
        }
        self.after_edit();
    }

    fn on_print(self: &Rc<Self>) {
        let Some(page) = self.page() else { return };
        // Entraînement des jeunes : on imprime toujours le groupe complet.
        if super::group_print::is_youth(&page, self.index) {
            super::group_print::print(&page, self.index, None);
            return;
        }
        let data = page
            .with_contest(|c| {
                let title = format!("{} — {}", c.name, tr("Allocation"));
                Self::snapshot(c, self.index).map(|snapshot| (title, snapshot))
            })
            .flatten();
        let Some((title, snapshot)) = data else {
            return;
        };
        let list_visible = self
            .glade
            .get::<gtk::ToggleButton>("fencer_list")
            .is_active();
        if let Err(error) = print::run(page.window().as_ref(), &title, snapshot, list_visible) {
            dialogs::error(page.window().as_ref(), &error.to_string());
        }
    }

    fn on_latecomer(self: &Rc<Self>) {
        self.on_presence(PillowKind::Latecomer);
    }

    fn on_absent(self: &Rc<Self>) {
        self.on_presence(PillowKind::Absent);
    }

    fn on_presence(self: &Rc<Self>, kind: PillowKind) {
        let Some(page) = self.page() else { return };
        let data = page
            .with_contest(|c| {
                Self::stage(c, self.index, |a, c| {
                    let round = a.round();
                    if a.base().locked || !round.pillows_allowed() {
                        return None;
                    }
                    let checkin = c.schedule.checkin()?;
                    let candidates = checkin
                        .players()
                        .iter()
                        .copied()
                        .filter(|r| {
                            c.players
                                .get(*r)
                                .map(|p| {
                                    p.class == bp_core::player::PlayerClass::Fencer
                                        && p.attending() == (kind == PillowKind::Absent)
                                })
                                .unwrap_or(false)
                        })
                        .collect();
                    Some((
                        round.clone(),
                        c.players.clone(),
                        candidates,
                        checkin.filter.attr_list().to_vec(),
                    ))
                })
            })
            .flatten()
            .flatten();
        let Some((round, players, candidates, attrs)) = data else {
            return;
        };
        let Some(changes) = super::pillow::run(
            &self.glade,
            page.window().as_ref(),
            round,
            players,
            candidates,
            attrs,
            kind,
        ) else {
            return;
        };
        if changes.touched.is_empty() {
            return;
        }
        page.with_contest(|c| {
            c.with_schedule(|schedule, ctx| {
                let Some(allocator) = schedule
                    .stage(self.index)
                    .and_then(|s| s.as_any().downcast_ref::<PoolAllocatorStage>())
                else {
                    return;
                };
                *allocator.round() = changes.round;
                *ctx.players = changes.players;
                // Inscrire tous les nouveaux joueurs avant le reverrouillage : celui-ci
                // renumérote les références, ensuite propagées aux autres phases.
                if let Some(checkin) = schedule.checkin_mut() {
                    checkin.on_attendees_toggled(ctx, &changes.touched);
                }
                schedule.apply_renumbering(ctx);
                if let Some(allocator) = schedule
                    .stage(self.index)
                    .and_then(|s| s.as_any().downcast_ref::<PoolAllocatorStage>())
                {
                    allocator.round().check_distribution(ctx.players);
                }
                schedule.refresh_general_classification(ctx);
            })
        });
        page.mark_dirty();
        self.after_edit();
    }

    fn on_fencer_list_toggled(self: &Rc<Self>) {
        let toggle: gtk::ToggleButton = self.glade.get("fencer_list");
        let hook: gtk::Widget = self.glade.get("fencer_list_hook");
        hook.set_visible(toggle.is_active());
    }
}

impl StageScreen for AllocatorScreen {
    fn widget(&self) -> gtk::Widget {
        self.root.clone()
    }

    fn refresh(&self) {
        if let Some(page) = self.page() {
            let index = self.index;
            if let Some(Some(snapshot)) = page.with_contest(|c| Self::snapshot(c, index)) {
                self.apply(snapshot);
            }
        }
    }
}
