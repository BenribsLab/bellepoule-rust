//! Écran de la composition des poules (port de `pool_allocator.cpp`, parties
//! interface) : formules, poules, glisser-déposer des tireurs, critères de
//! séparation, liste des tireurs.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use bp_core::attribute;
use bp_core::filter::Filter;
use bp_core::i18n::tr;
use bp_core::player::PlayerRef;
use bp_core::rounds::pool::round::{extra_swapping_criteria, regular_swapping_criteria, FilterKind};
use bp_core::rounds::pool::PoolAllocatorStage;
use bp_core::stage::Stage;
use bp_gtk::dialogs;
use bp_gtk::glade::Glade;
use gtk4 as gtk;
use gtk::gdk;
use gtk::glib;
use gtk::prelude::*;

use super::StageScreen;
use crate::contest_page::ContestPage;
use crate::widgets::filter_dialog;
use crate::widgets::players_list::{Cell as ListCell, PlayersListView, RowData};
use crate::widgets::render;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mark {
    None,
    Moved,
    Error,
}

struct FencerLine {
    player: PlayerRef,
    cells: Vec<String>,
    mark: Mark,
    absent: bool,
}

struct PoolView {
    name: String,
    balanced: bool,
    piste: Option<String>,
    time: Option<String>,
    referees: Vec<Vec<String>>,
    fencers: Vec<FencerLine>,
}

struct Snapshot {
    /// (taille, nombre de poules) de chaque formule.
    configs: Vec<(String, String)>,
    selected: Option<usize>,
    best: Option<usize>,
    pools: Vec<PoolView>,
    locked: bool,
    swap_errors: usize,
    criteria: Vec<String>,
    adjustable: bool,
    list_rows: Vec<RowData>,
    list_specs: Vec<crate::widgets::players_list::ColumnSpec>,
}

pub struct AllocatorScreen {
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

        let screen = Rc::new(AllocatorScreen {
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
        on!("on_latecomer_toolbutton_clicked", on_not_available);
        on!("on_absent_toolbutton_clicked", on_not_available);

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

    fn stage<R>(c: &mut bp_core::contest::Contest, index: usize, f: impl FnOnce(&PoolAllocatorStage, &bp_core::contest::Contest) -> R) -> Option<R> {
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

            let layout: Vec<_> = round
                .allocator_filter
                .iter()
                .filter_map(|code| attribute::registry().by_code(code).map(|d| (d.code_name, d)))
                .collect();
            let image = |r: PlayerRef| -> Vec<String> {
                let Some(p) = store.get(r) else { return Vec::new() };
                layout
                    .iter()
                    .map(|(code, desc)| {
                        render::value_of(p, code, owner).map(|v| render::user_image(desc, v, desc.favorite_look)).unwrap_or_default()
                    })
                    .collect()
            };

            let configs = round.configs().iter().map(|c| (c.size_label(), c.nb_pool.to_string())).collect();
            let pools = round
                .pools()
                .iter()
                .enumerate()
                .map(|(i, pool)| PoolView {
                    name: pool.name().to_string(),
                    balanced: round.pool_is_balanced(i),
                    piste: pool.piste_label(),
                    time: pool.start_time_image(),
                    referees: pool.referees().iter().map(|r| image(*r)).collect(),
                    fencers: pool
                        .fencers()
                        .iter()
                        .map(|f| {
                            let absent = store.get(*f).and_then(|p| p.str_attr("incident")).map(|s| s.starts_with('A')).unwrap_or(false);
                            let mark = if round.swapper().is_an_error(*f) {
                                Mark::Error
                            } else if round.is_moved(store, *f) {
                                Mark::Moved
                            } else {
                                Mark::None
                            };
                            FencerLine { player: *f, cells: image(*f), mark, absent }
                        })
                        .collect(),
                })
                .collect();

            // Liste des tireurs
            let mut list_filter = Filter::including("allocator_list", &FilterKind::List.available());
            list_filter.apply_list(&round.list_filter);
            let list_specs = render::column_specs(list_filter.layout(), false);
            let list_rows = round
                .attendees()
                .iter()
                .filter_map(|r| {
                    let p = store.get(*r)?;
                    Some(RowData {
                        player: *r,
                        cells: list_specs.iter().map(|s| render::cell_for(p, s, owner)).collect(),
                        sort: render::sort_values(p, &list_specs, owner),
                        children: Vec::new(),
                        dimmed: false,
                    })
                })
                .collect();

            Snapshot {
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

    fn apply(self: &Rc<Self>, s: Snapshot) {
        self.updating.set(true);

        // Formules
        let store: gtk::ListStore = self.glade.get("combo_liststore");
        store.clear();
        for (i, (size, nb)) in s.configs.iter().enumerate() {
            let iter = store.append();
            let icon = if Some(i) == s.best { "help-about" } else { "" };
            store.set(&iter, &[(0, size), (1, nb), (2, &icon)]);
        }
        let nb_combo: gtk::ComboBox = self.glade.get("nb_pools_combobox");
        let size_combo: gtk::ComboBox = self.glade.get("pool_size_combobox");
        nb_combo.set_active(s.selected.map(|i| i as u32));
        size_combo.set_active(s.selected.map(|i| i as u32));
        nb_combo.set_sensitive(!s.locked);
        size_combo.set_sensitive(!s.locked);
        for id in ["extra_toggle", "extra_combobox", "regular_swapping_hbox"] {
            self.glade.get::<gtk::Widget>(id).set_sensitive(!s.locked);
        }
        for id in ["latecomer_toolbutton", "absent_toolbutton"] {
            self.glade.get::<gtk::Widget>(id).set_visible(s.adjustable);
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

    fn pool_widget(self: &Rc<Self>, pool: &PoolView, locked: bool) -> gtk::Widget {
        let frame = gtk::Box::new(gtk::Orientation::Vertical, 4);
        frame.add_css_class("bp-pool");

        let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let icon = gtk::Image::from_icon_name(if pool.balanced { "object-select-symbolic" } else { "dialog-error-symbolic" });
        header.append(&icon);
        let name = gtk::Label::new(None);
        name.set_markup(&format!("<span size='large' weight='bold'>{}</span>", glib::markup_escape_text(&pool.name)));
        header.append(&name);
        for text in [pool.piste.as_ref(), pool.time.as_ref()].into_iter().flatten() {
            let l = gtk::Label::new(None);
            l.set_markup(&format!("<span foreground='darkgreen' style='italic' weight='bold'>{}</span>", glib::markup_escape_text(text)));
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
            for (c, text) in referee.iter().enumerate() {
                let l = gtk::Label::new(None);
                l.set_markup(&format!("<i><b>{}</b></i>", glib::markup_escape_text(text)));
                l.set_xalign(0.0);
                grid.attach(&l, c as i32 + 1, row, 1, 1);
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
            for (c, text) in fencer.cells.iter().enumerate() {
                let l = gtk::Label::new(None);
                let esc = glib::markup_escape_text(text);
                if fencer.absent {
                    l.set_markup(&format!("<s>{esc}</s>"));
                } else {
                    l.set_text(text);
                }
                l.set_xalign(0.0);
                grid.attach(&l, c as i32 + 1, row, 1, 1);
                labels.push(l);
            }
            if !locked {
                for l in &labels {
                    let source = gtk::DragSource::new();
                    source.set_actions(gdk::DragAction::MOVE);
                    source.set_content(Some(&gdk::ContentProvider::for_value(&fencer.player.to_value())));
                    l.add_controller(source);
                }
            }
            row += 1;
        }
        frame.append(&grid);

        if !locked {
            let target = gtk::DropTarget::new(PlayerRef::static_type(), gdk::DragAction::MOVE);
            let w = Rc::downgrade(self);
            let name = pool.name.clone();
            target.connect_drop(move |_, value, _, _| {
                let Ok(player) = value.get::<PlayerRef>() else { return false };
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
        f: impl FnOnce(&mut bp_core::rounds::pool::round::PoolRound, &mut bp_core::player::PlayerStore) -> R,
    ) -> Option<R> {
        let page = self.page()?;
        let index = self.index;
        let result = page
            .with_contest(|c| {
                c.with_schedule(|s, ctx| {
                    let stage = s.stage_mut(index)?;
                    let allocator = stage.as_any_mut().downcast_mut::<PoolAllocatorStage>()?;
                    let mut round = allocator.round();
                    Some(f(&mut round, ctx.players))
                })
            })
            .flatten();
        page.mark_dirty();
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
        let regular: Vec<String> =
            self.regular.borrow().iter().filter(|(_, check)| check.is_active()).map(|(code, _)| code.clone()).collect();
        let extra_toggle: gtk::CheckButton = self.glade.get("extra_toggle");
        let extra_combo: gtk::ComboBox = self.glade.get("extra_combobox");
        let extra: Option<String> = if extra_toggle.is_active() {
            extra_combo.active().and_then(|i| self.extra_codes.get(i as usize).cloned())
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
                p.with_contest(|c| Self::stage(c, index, |a, _| a.round().pools().iter().position(|pool| pool.name() == pool_name)))
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
        let Some(app) = page.app.upgrade() else { return };
        let window = page.window();
        let current = page.with_contest(|c| Self::stage(c, self.index, |a, _| a.round().allocator_filter.clone())).flatten().flatten();
        let Some(current) = current else { return };
        let mut filter = Filter::including("allocator", &FilterKind::Allocator.available());
        filter.apply_list(&current);
        let changed = {
            let mut t = app.tournament.borrow_mut();
            let mut config = std::mem::take(&mut t.config);
            let changed = filter_dialog::select_attributes(window.as_ref(), &mut filter, Some(&mut config));
            t.config = config;
            changed
        };
        if changed {
            let codes = filter.selected_codes();
            self.edit(|round, _| round.allocator_filter = codes);
        }
        self.after_edit();
    }

    fn on_print(self: &Rc<Self>) {
        dialogs::info(
            self.page().and_then(|p| p.window()).as_ref(),
            &tr("Print"),
            Some(&tr("Printing is not available yet in this version.")),
        );
    }

    fn on_not_available(self: &Rc<Self>) {
        dialogs::info(
            self.page().and_then(|p| p.window()).as_ref(),
            &tr("Not available yet in this version."),
            None,
        );
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
        // `refresh` n'a pas accès à `Rc<Self>` : on reconstruit un Rc faible.
        // Le rafraîchissement réel passe par `refresh_now`, appelé ci-dessous.
        if let Some(page) = self.page() {
            let index = self.index;
            if let Some(Some(snapshot)) = page.with_contest(|c| Self::snapshot(c, index)) {
                self.apply_ref(snapshot);
            }
        }
    }
}
