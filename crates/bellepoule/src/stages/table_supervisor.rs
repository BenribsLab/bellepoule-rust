//! Superviseur de tableaux : arbre, saisie, suites de classement et impressions.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use bp_core::filter::Filter;
use bp_core::i18n::tr;
use bp_core::player::{PlayerRef, PlayerStore};
use bp_core::rounds::table::{
    FencedPlaces, LayoutMetrics, NodeId, TableSet, TableSetStatus, TableSupervisor,
};
use bp_core::stage::{Stage, StageBase, StageContext};
use bp_gtk::{
    canvas::{Anchor, Bounds, Canvas, CanvasView, Prop},
    dialogs,
    glade::Glade,
};
use gtk::{glib, prelude::*};
use gtk4 as gtk;

use super::StageScreen;
use crate::contest_page::ContestPage;
use crate::widgets::{
    filter_dialog,
    players_list::{ColumnSpec, PlayersListView, RowData},
    render,
};

mod print;
mod scene;

const NO_NODE: u32 = u32::MAX;
const METRICS: LayoutMetrics = LayoutMetrics {
    cell_width: 320.0,
    row_height: 36.0,
    header_height: 42.0,
    column_spacing: 12.0,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct ScoreKey {
    set: usize,
    node: NodeId,
    player: PlayerRef,
    quick: bool,
}
struct Focus {
    key: ScoreKey,
    text: String,
    cursor: i32,
    advance: bool,
}

#[derive(Clone)]
struct SetRow {
    index: usize,
    parent: Option<usize>,
    name: String,
    visible: bool,
    status: TableSetStatus,
}
#[derive(Clone)]
struct Snapshot {
    base: StageBase,
    mode: FencedPlaces,
    displayed: Option<usize>,
    set: Option<TableSet>,
    sets: Vec<SetRow>,
    players: PlayerStore,
    referees: PlayerStore,
    specs: Vec<ColumnSpec>,
    rows: Vec<RowData>,
    stuffing: bool,
    title: String,
    contest_net_id: u32,
}

pub struct TableScreen {
    page: Weak<ContestPage>,
    weak_self: Weak<Self>,
    index: usize,
    glade: Glade,
    table_glade: Glade,
    root: gtk::Widget,
    configuration: gtk::Widget,
    canvas: Canvas,
    view: CanvasView,
    score_css: gtk::CssProvider,
    classification: PlayersListView,
    updating: Cell<bool>,
    refresh_pending: Cell<bool>,
    entries: RefCell<HashMap<ScoreKey, gtk::Entry>>,
    order: RefCell<Vec<ScoreKey>>,
    focus: RefCell<Option<Focus>>,
    quick_node: Cell<Option<NodeId>>,
}

impl TableScreen {
    pub fn new(page: &Rc<ContestPage>, index: usize) -> Rc<Self> {
        let glade = Glade::new("table_supervisor.ui").expect("table_supervisor.ui");
        let table_glade = Glade::new("table.ui").expect("table.ui");
        let root: gtk::Widget = glade.get("root");
        let configuration: gtk::Widget = glade.get("stage_configuration");
        Glade::detach_from_parent(&root);
        Glade::detach_from_parent(&configuration);
        let table_root: gtk::Widget = table_glade.get("root");
        Glade::detach_from_parent(&table_root);
        glade
            .get::<gtk::Viewport>("table_set_hook")
            .set_child(Some(&table_root));
        let canvas = Canvas::new();
        let view = CanvasView::new(&canvas);
        let scroll: gtk::ScrolledWindow = table_glade.get("canvas_scrolled_window");
        let parent = scroll
            .parent()
            .and_then(|w| w.downcast::<gtk::Box>().ok())
            .expect("table root");
        parent.remove(&scroll);
        parent.append(view.widget());
        let classification = PlayersListView::new();
        glade
            .get::<gtk::Viewport>("classification_hook")
            .set_child(Some(classification.widget()));
        glade
            .get::<gtk::TreeModelFilter>("table_set_treemodelfilter")
            .set_visible_column(3);
        // GTK4 conserve la saisie au zoom maximal, sans réduire la police des widgets.
        glade
            .get::<gtk::Adjustment>("zoom_adjustment")
            .set_page_size(0.0);
        let zoom = glade.get::<gtk::Scale>("zoom_scale").value();
        view.set_zoom(zoom);
        let screen = Rc::new_cyclic(|weak_self| Self {
            page: Rc::downgrade(page),
            weak_self: weak_self.clone(),
            index,
            glade,
            table_glade,
            root,
            configuration,
            canvas,
            view,
            classification,
            score_css: gtk::CssProvider::new(),
            updating: Cell::new(false),
            refresh_pending: Cell::new(false),
            entries: RefCell::new(HashMap::new()),
            order: RefCell::new(Vec::new()),
            focus: RefCell::new(None),
            quick_node: Cell::new(None),
        });
        screen.connect();
        screen.update_score_css(zoom);
        screen.refresh();
        screen
    }

    fn connect(self: &Rc<Self>) {
        macro_rules! on {
            ($name:expr, $method:ident) => {{
                let weak = self.weak_self.clone();
                self.glade.connect_simple($name, move |_| {
                    if let Some(s) = weak.upgrade() {
                        s.$method();
                    }
                });
            }};
        }
        on!("on_table_set_treeview_cursor_changed", on_set_selected);
        on!("on_previous_button_clicked", on_previous);
        on!("on_next_button_clicked", on_next);
        on!(
            "on_table_classification_toggletoolbutton_toggled",
            toggle_views
        );
        on!("on_input_toolbutton_toggled", toggle_views);
        on!("on_search_entry_changed", on_search);
        on!("on_table_filter_toolbutton_clicked", on_filter);
        on!("on_table_print_toolbutton_clicked", on_print);
        on!("on_print_score_sheets_clicked", on_print_sheets);
        on!("on_table_stuff_toolbutton_clicked", on_stuff);
        on!("on_all_radiobutton_toggled", on_config);
        for id in ["name_entry", "max_score_entry"] {
            let weak = self.weak_self.clone();
            self.glade.get::<gtk::Entry>(id).connect_changed(move |_| {
                if let Some(s) = weak.upgrade() {
                    s.on_config();
                }
            });
        }
        for id in [
            "no_radiobutton",
            "third_place_radiobutton",
            "all_places_radiobutton",
        ] {
            let weak = self.weak_self.clone();
            self.glade
                .get::<gtk::CheckButton>(id)
                .connect_toggled(move |b| {
                    if b.is_active() {
                        if let Some(s) = weak.upgrade() {
                            s.on_config();
                        }
                    }
                });
        }
        let weak = self.weak_self.clone();
        self.glade
            .get::<gtk::SpinButton>("nb_qualified_spinbutton")
            .connect_value_changed(move |_| {
                if let Some(s) = weak.upgrade() {
                    s.on_config();
                }
            });
        let weak = self.weak_self.clone();
        self.glade
            .get::<gtk::Scale>("zoom_scale")
            .connect_value_changed(move |scale| {
                if let Some(s) = weak.upgrade() {
                    s.update_score_css(scale.value());
                    s.view.set_zoom(scale.value());
                }
            });
        let weak = self.weak_self.clone();
        self.table_glade
            .connect_simple("on_quick_search_combobox_changed", move |_| {
                if let Some(s) = weak.upgrade() {
                    s.on_quick_selected();
                }
            });
    }

    fn access<R>(
        &self,
        write: bool,
        f: impl FnOnce(&mut TableSupervisor, &mut StageContext) -> R,
    ) -> Option<R> {
        let page = self.page.upgrade()?;
        let result = page
            .with_contest(|c| {
                c.with_schedule(|schedule, ctx| {
                    let stage = schedule
                        .stage_mut(self.index)?
                        .as_any_mut()
                        .downcast_mut::<TableSupervisor>()?;
                    if write && stage.base().locked {
                        return None;
                    }
                    Some(f(stage, ctx))
                })
            })
            .flatten();
        if write && result.is_some() {
            page.mark_dirty();
        }
        result
    }

    fn snapshot(&self) -> Option<Snapshot> {
        self.page.upgrade()?.with_contest(|c| Self::build_snapshot(c, self.index)).flatten()
    }

    fn build_snapshot(c: &mut bp_core::contest::Contest, index: usize) -> Option<Snapshot> {
        let title = c.name.clone();
        let referees = c.referees.store().clone();
        let contest_net_id = c.net_id;
        c.with_schedule(|schedule, ctx| {
            let stage = schedule.stage_mut(index)?.as_any_mut().downcast_mut::<TableSupervisor>()?;
            if !stage.base().locked {
                stage.update_classification(ctx);
            }
            let base = stage.base().clone();
            let mut filter = Self::filter(true);
            filter.apply_list(&base.classification_filter);
            let specs = render::column_specs(filter.layout(), false);
            let rows = stage
                .classification(ctx)
                .iter()
                .filter_map(|entry| {
                    let p = ctx.players.get(entry.player)?;
                    Some(RowData {
                        player: entry.player,
                        cells: specs.iter().map(|s| render::cell_for(p, s, base.owner())).collect(),
                        sort: render::sort_values(p, &specs, base.owner()),
                        children: Vec::new(),
                        dimmed: entry.status != 'Q',
                    })
                })
                .collect();
            let displayed = stage.displayed_table_set();
            let set = displayed.and_then(|i| stage.table_sets().get(i)).cloned();
            let sets = stage
                .table_sets()
                .iter()
                .enumerate()
                .map(|(i, s)| SetRow {
                    index: i,
                    parent: s.parent(),
                    name: s.name(),
                    visible: s.is_visible(),
                    status: stage.table_set_status(i),
                })
                .collect();
            Some(Snapshot {
                base,
                mode: stage.fenced_places_mode(),
                displayed,
                set,
                sets,
                players: ctx.players.clone(),
                referees,
                specs,
                rows,
                stuffing: ctx.settings.score_stuffing_allowed,
                title,
                contest_net_id,
            })
        })
    }

    fn filter(classification: bool) -> Filter {
        let mut excluded = vec![
            "ref",
            "plugin_ID",
            "incident",
            "IP",
            "password",
            "cyphered_password",
            "HS",
            "attending",
            "exported",
            "final_rank",
            "global_status",
            "indice",
            "level",
            "workload_rate",
            "pool_nr",
            "promoted",
            "team",
            "victories_count",
            "bouts_count",
            "victories_ratio",
            "strip",
            "time",
        ];
        if !classification {
            excluded.extend(["rank", "status"]);
        }
        Filter::excluding(
            if classification {
                "table_classification"
            } else {
                "table"
            },
            &excluded,
        )
    }

    fn queue_refresh(&self) {
        if self.refresh_pending.replace(true) {
            return;
        }
        let weak = self.weak_self.clone();
        glib::idle_add_local_once(move || {
            let Some(s) = weak.upgrade() else { return };
            s.refresh_pending.set(false);
            let mut focus = s.focus.borrow_mut().take();
            if let Some(f) = focus.as_mut() {
                if let Some(entry) = s.entries.borrow().get(&f.key) {
                    f.cursor = entry.position();
                }
            }
            let target = focus.as_ref().map(|f| {
                if f.advance {
                    let order = s.order.borrow();
                    order
                        .iter()
                        .position(|k| *k == f.key)
                        .and_then(|i| order.get(i + 1))
                        .copied()
                        .unwrap_or(f.key)
                } else {
                    f.key
                }
            });
            if let Some(key) = target.filter(|k| k.quick) {
                s.quick_node.set(Some(key.node));
            }
            s.refresh();
            if let (Some(f), Some(key)) = (focus, target) {
                if let Some(entry) = s.entries.borrow().get(&key) {
                    s.updating.set(true);
                    if !f.advance {
                        entry.set_text(&f.text);
                    }
                    crate::widgets::focus_entry_when_ready(entry, f.advance, f.cursor);
                    s.updating.set(false);
                }
            }
            if let Some(page) = s.page.upgrade() {
                if let Some(schedule) = page.schedule.borrow().as_ref() {
                    schedule.refresh_sensitivity();
                }
            }
        });
    }

    fn valid_key(stage: &TableSupervisor, key: ScoreKey) -> bool {
        stage
            .table_sets()
            .get(key.set)
            .and_then(|set| set.nodes().get(key.node))
            .and_then(|n| n.bout.as_ref())
            .is_some_and(|m| {
                m.opponent(0).is_some() && m.opponent(1).is_some() && m.score(key.player).is_some()
            })
    }

    fn score_entry(&self, key: ScoreKey, text: &str, locked: bool, error: bool) -> gtk::Entry {
        let entry = gtk::Entry::new();
        entry.set_width_chars(3);
        entry.set_max_width_chars(4);
        entry.set_max_length(6);
        gtk::prelude::EntryExt::set_alignment(&entry, 0.5);
        entry.set_text(text);
        if !key.quick {
            entry.add_css_class("bp-table-score");
            entry
                .style_context()
                .add_provider(&self.score_css, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
        }
        entry.set_editable(!locked);
        entry.set_can_focus(!locked);
        entry.set_tooltip_text(Some("V · W3 / V3 · A / E / F"));
        if error {
            entry.add_css_class("error");
        }
        self.entries.borrow_mut().insert(key, entry.clone());
        let weak = self.weak_self.clone();
        entry.connect_changed(move |entry| {
            let Some(s) = weak.upgrade() else { return };
            if s.updating.get() {
                return;
            }
            let text = entry.text().to_string();
            let upper = text.to_ascii_uppercase();
            let status = match upper.as_str() {
                "A" => Some('A'),
                "E" => Some('E'),
                "F" => Some('F'),
                _ => None,
            };
            let digits = upper.strip_prefix(['V', 'W']).unwrap_or(&upper);
            if status.is_none() && (!text.is_ascii() || !digits.bytes().all(|b| b.is_ascii_digit()))
            {
                entry.add_css_class("error");
                return;
            }
            let input = if upper.starts_with('V') && upper.len() > 1 {
                format!("W{}", &upper[1..])
            } else {
                upper
            };
            let result = s.access(true, |stage, ctx| {
                if !Self::valid_key(stage, key) {
                    return false;
                }
                if let Some(code) = status {
                    stage.set_fencer_status(ctx, key.set, key.node, key.player, code);
                    true
                } else {
                    if stage.current_status(key.set, key.node, key.player) != 'Q' {
                        stage.set_fencer_status(ctx, key.set, key.node, key.player, 'Q');
                    }
                    stage.set_score(ctx, key.set, key.node, key.player, &input)
                }
            });
            if result.is_none() {
                return;
            }
            if crate::widgets::entry_has_focus(entry) {
                *s.focus.borrow_mut() = Some(Focus {
                    key,
                    text,
                    cursor: entry.position(),
                    advance: result == Some(true),
                });
            }
            s.queue_refresh();
        });
        let weak = self.weak_self.clone();
        entry.connect_activate(move |entry| {
            if let Some(s) = weak.upgrade() {
                *s.focus.borrow_mut() = Some(Focus {
                    key,
                    text: entry.text().to_string(),
                    cursor: entry.position(),
                    advance: true,
                });
                s.queue_refresh();
            }
        });
        if !locked {
            let click = gtk::GestureClick::new();
            click.set_button(3);
            click.set_propagation_phase(gtk::PropagationPhase::Capture);
            let weak = self.weak_self.clone();
            let target = entry.downgrade();
            click.connect_pressed(move |gesture, _, _, _| {
                if let (Some(s), Some(entry)) = (weak.upgrade(), target.upgrade()) {
                    gesture.set_state(gtk::EventSequenceState::Claimed);
                    s.status_menu(&entry, key);
                }
            });
            entry.add_controller(click);
        }
        entry
    }

    fn update_score_css(&self, zoom: f64) {
        self.score_css.load_from_data(&format!(
            "entry.bp-table-score {{ min-width: 0; min-height: 0; padding: 0; border-radius: 0; font-size: {}px; }}",
            14.0 * zoom
        ));
    }

    fn status_menu(&self, entry: &gtk::Entry, key: ScoreKey) {
        let popover = gtk::Popover::new();
        popover.set_parent(entry);
        let buttons = gtk::Box::new(gtk::Orientation::Vertical, 0);
        for (code, title) in [
            ('Q', "Restore"),
            ('A', "Withdrawal"),
            ('E', "Excluded"),
            ('F', "Forfeit"),
        ] {
            let button = gtk::Button::with_label(&tr(title));
            let weak = self.weak_self.clone();
            let popup = popover.downgrade();
            button.connect_clicked(move |_| {
                if let Some(p) = popup.upgrade() {
                    p.popdown();
                }
                if let Some(s) = weak.upgrade() {
                    s.access(true, |stage, ctx| {
                        if Self::valid_key(stage, key) {
                            stage.set_fencer_status(ctx, key.set, key.node, key.player, code);
                        }
                    });
                    s.queue_refresh();
                }
            });
            buttons.append(&button);
        }
        popover.set_child(Some(&buttons));
        popover.connect_closed(|p| p.unparent());
        popover.popup();
    }

    fn fill_sets(&self, snapshot: &Snapshot) {
        let store: gtk::TreeStore = self.glade.get("table_set_treestore");
        store.clear();
        let mut iters = HashMap::new();
        let mut selected = None;
        for row in &snapshot.sets {
            let parent = row.parent.and_then(|p| iters.get(&p));
            let iter = store.append(parent);
            let icon = match row.status {
                TableSetStatus::Over => "object-select-symbolic",
                TableSetStatus::Error => "dialog-warning-symbolic",
                TableSetStatus::Running => "media-playback-start-symbolic",
                TableSetStatus::Empty => "",
            };
            store.set(
                &iter,
                &[
                    (0, &row.name),
                    (1, &(row.index as u32)),
                    (2, &icon),
                    (3, &row.visible),
                ],
            );
            iters.insert(row.index, iter);
            if snapshot.displayed == Some(row.index) {
                selected = Some(iter);
            }
        }
        let tree: gtk::TreeView = self.glade.get("table_set_treeview");
        tree.set_visible(matches!(
            snapshot.mode,
            FencedPlaces::AllPlaces | FencedPlaces::ThirdPlaces
        ));
        tree.expand_all();
        if let Some(iter) = selected {
            let filter: gtk::TreeModelFilter = self.glade.get("table_set_treemodelfilter");
            if let Some(path) = filter.convert_child_path_to_path(&store.path(&iter)) {
                tree.selection().select_path(&path);
            }
        }
    }

    fn fill_quick(&self, set: &TableSet, set_index: usize, snapshot: &Snapshot) {
        let store: gtk::TreeStore = self.table_glade.get("match_treestore");
        store.clear();
        let mut selected = None;
        let quick = set.quick_search(&snapshot.players);
        let valid = self.quick_node.get().filter(|node| {
            quick
                .iter()
                .any(|t| t.matches.iter().any(|m| m.node == *node))
        });
        let node = valid
            .or_else(|| {
                quick
                    .iter()
                    .flat_map(|t| &t.matches)
                    .find(|m| set.bout(m.node).is_some_and(|b| !b.is_over()))
                    .map(|m| m.node)
            })
            .or_else(|| quick.iter().flat_map(|t| &t.matches).next().map(|m| m.node));
        self.quick_node.set(node);
        for table in quick {
            if table.matches.is_empty() {
                continue;
            }
            let parent = store.append(None);
            store.set(&parent, &[(0, &table.image), (1, &NO_NODE), (3, &true)]);
            for m in table.matches {
                let iter = store.append(Some(&parent));
                store.set(
                    &iter,
                    &[
                        (0, &m.label),
                        (1, &(m.node as u32)),
                        (2, &m.name),
                        (3, &true),
                    ],
                );
                if node == Some(m.node) {
                    selected = Some(iter);
                }
                if let Some(bout) = set.bout(m.node) {
                    for p in [bout.opponent(0), bout.opponent(1)].into_iter().flatten() {
                        self.order.borrow_mut().push(ScoreKey {
                            set: set_index,
                            node: m.node,
                            player: p,
                            quick: true,
                        });
                    }
                }
            }
        }
        let combo: gtk::ComboBox = self.table_glade.get("quick_search_combobox");
        if let Some(iter) = selected {
            let filter: gtk::TreeModelFilter = self.table_glade.get("match_treemodelfilter");
            if let Some(iter) = filter.convert_child_iter_to_iter(&iter) {
                combo.set_active_iter(Some(&iter));
            }
        } else {
            combo.set_active(None);
        }
        for (i, (label, hook)) in [
            ("fencerA_label", "fencerA_hook"),
            ("fencerB_label", "fencerB_hook"),
        ]
        .into_iter()
        .enumerate()
        {
            let bout = node.and_then(|n| set.bout(n));
            let player = bout.and_then(|m| m.opponent(i));
            self.table_glade.get::<gtk::Label>(label).set_text(
                &player
                    .and_then(|p| snapshot.players.get(p))
                    .map(|p| format!("{} {}", p.name(), p.first_name()))
                    .unwrap_or_default(),
            );
            let hook: gtk::ScrolledWindow = self.table_glade.get(hook);
            if let (Some(node), Some(player), Some(bout)) = (node, player, bout) {
                let text = bout
                    .score(player)
                    .map(|score| {
                        score
                            .drop_reason()
                            .map(|c| c.to_string())
                            .unwrap_or_else(|| score.image(bout.max_score))
                    })
                    .unwrap_or_default();
                hook.set_child(Some(&self.score_entry(
                    ScoreKey {
                        set: set_index,
                        node,
                        player,
                        quick: true,
                    },
                    &text,
                    snapshot.base.locked,
                    bout.has_error(),
                )));
            } else {
                hook.set_child(gtk::Widget::NONE);
            }
        }
    }

    fn display(&self, snapshot: &Snapshot) {
        self.canvas.reset();
        self.entries.borrow_mut().clear();
        self.order.borrow_mut().clear();
        let Some((set_index, set)) = snapshot.displayed.zip(snapshot.set.as_ref()) else {
            self.quick_node.set(None);
            self.table_glade
                .get::<gtk::TreeStore>("match_treestore")
                .clear();
            for id in ["fencerA_hook", "fencerB_hook"] {
                self.table_glade
                    .get::<gtk::ScrolledWindow>(id)
                    .set_child(gtk::Widget::NONE);
            }
            for id in ["fencerA_label", "fencerB_label"] {
                self.table_glade.get::<gtk::Label>(id).set_text("");
            }
            for id in ["previous_button", "next_button", "print_score_sheets"] {
                self.glade.get::<gtk::Widget>(id).set_sensitive(false);
            }
            self.glade.get::<gtk::Label>("from_label").set_text("");
            self.glade
                .get::<gtk::Label>("search_namespace")
                .set_text("");
            self.view.refresh();
            return;
        };
        let drawing = scene::draw(&self.canvas, set, snapshot, &METRICS, true);
        let mut slots: Vec<_> = drawing
            .cells
            .iter()
            .filter_map(|c| c.score.as_ref())
            .filter(|s| s.visible && s.fencer.is_some())
            .collect();
        slots.sort_by_key(|slot| {
            let node = set.node(slot.bout_node);
            (
                std::cmp::Reverse(node.table),
                node.bout.as_ref().map(|m| m.number).unwrap_or(0),
                node.bout
                    .as_ref()
                    .and_then(|m| m.position_of(slot.fencer.unwrap()))
                    .unwrap_or(0),
            )
        });
        for slot in slots {
            let key = ScoreKey {
                set: set_index,
                node: slot.bout_node,
                player: slot.fencer.unwrap(),
                quick: false,
            };
            let text = slot
                .drop_reason
                .map(|c| c.to_string())
                .unwrap_or_else(|| slot.text.clone());
            let error = set.bout(slot.bout_node).is_some_and(|m| m.has_error());
            let entry = self.score_entry(key, &text, snapshot.base.locked, error);
            self.canvas.widget(
                self.canvas.root(),
                &entry,
                slot.bounds.x1,
                slot.bounds.y1,
                slot.bounds.x2 - slot.bounds.x1,
                slot.bounds.y2 - slot.bounds.y1,
                Anchor::NW,
            );
            self.order.borrow_mut().push(key);
        }
        for header in &drawing.headers {
            if !header.print_icon {
                continue;
            }
            let hit =
                self.canvas
                    .rect(self.canvas.root(), header.bounds.x2 - 24.0, 0.0, 24.0, 24.0);
            self.canvas
                .set(hit, [Prop::FillColorRgba(0xffffff00), Prop::NoStroke]);
            let weak = self.weak_self.clone();
            let table = header.table;
            self.canvas
                .connect_button_press(hit, move |_, _, _, event| {
                    if event.button != 1 {
                        return false;
                    }
                    if let Some(s) = weak.upgrade() {
                        glib::idle_add_local_once(move || s.print_sheets(Some(table), None));
                    }
                    true
                });
        }
        for cell in drawing.cells.iter().filter(|cell| cell.print_icon) {
            let (x, y) = scene::print_origin(cell);
            let hit = self.canvas.rect(self.canvas.root(), x, y, 22.0, 20.0);
            self.canvas
                .set(hit, [Prop::FillColorRgba(0xffffff00), Prop::NoStroke]);
            let weak = self.weak_self.clone();
            let node = cell.node;
            self.canvas
                .connect_button_press(hit, move |_, _, _, event| {
                    if event.button != 1 {
                        return false;
                    }
                    if let Some(s) = weak.upgrade() {
                        glib::idle_add_local_once(move || s.print_sheets(None, Some(node)));
                    }
                    true
                });
        }
        self.fill_quick(set, set_index, snapshot);
        self.glade
            .get::<gtk::Widget>("previous_button")
            .set_sensitive(set.can_display_previous());
        self.glade
            .get::<gtk::Widget>("next_button")
            .set_sensitive(set.can_display_next());
        self.glade.get::<gtk::Label>("from_label").set_text(
            &set.tables()
                .get(set.from_table())
                .map(|t| t.image())
                .unwrap_or_default(),
        );
        self.glade
            .get::<gtk::Label>("search_namespace")
            .set_text(&set.search_namespace());
        self.glade
            .get::<gtk::Widget>("print_score_sheets")
            .set_sensitive(true);
        self.view.refresh();
    }

    fn on_set_selected(&self) {
        if self.updating.get() {
            return;
        }
        let tree: gtk::TreeView = self.glade.get("table_set_treeview");
        if let Some((model, iter)) = tree.selection().selected() {
            let index = model.get::<u32>(&iter, 1) as usize;
            self.access(false, |stage, _| stage.select_table_set(Some(index)));
            self.quick_node.set(None);
            self.focus.borrow_mut().take();
            self.refresh();
        }
    }
    fn on_previous(&self) {
        self.navigate(false);
    }
    fn on_next(&self) {
        self.navigate(true);
    }
    fn navigate(&self, next: bool) {
        self.access(false, |stage, _| {
            if let Some(i) = stage.displayed_table_set() {
                if next {
                    stage.display_next(i);
                } else {
                    stage.display_previous(i);
                }
            }
        });
        self.focus.borrow_mut().take();
        self.refresh();
    }
    fn toggle_views(&self) {
        let classification = self
            .glade
            .get::<gtk::ToggleButton>("table_classification_toggletoolbutton")
            .is_active();
        self.glade
            .get::<gtk::Widget>("main_hook")
            .set_visible(!classification);
        self.glade
            .get::<gtk::Widget>("classification_hook")
            .set_visible(classification);
        self.glade
            .get::<gtk::Widget>("input_toolbutton")
            .set_sensitive(!classification);
        let input = self
            .glade
            .get::<gtk::ToggleButton>("input_toolbutton")
            .is_active();
        self.table_glade
            .get::<gtk::Widget>("input_hbox")
            .set_visible(input && !classification);
    }
    fn on_quick_selected(&self) {
        if self.updating.get() {
            return;
        }
        let combo: gtk::ComboBox = self.table_glade.get("quick_search_combobox");
        if let (Some(model), Some(iter)) = (combo.model(), combo.active_iter()) {
            let node = model.get::<u32>(&iter, 1);
            if node == NO_NODE {
                self.queue_refresh();
                return;
            }
            self.quick_node.set(Some(node as usize));
            self.refresh();
        }
    }
    fn on_search(&self) {
        if self.updating.get() {
            return;
        }
        let entry: gtk::Entry = self.glade.get("search_entry");
        entry.remove_css_class("error");
        if entry.text().is_empty() {
            return;
        }
        let node = entry
            .text()
            .parse::<u32>()
            .ok()
            .and_then(|number| {
                self.access(false, |stage, _| {
                    let index = stage.displayed_table_set()?;
                    let set = stage.table_set(index);
                    let node = set.goto_match(number)?;
                    let child = set.node(node).children.first().copied().unwrap_or(node);
                    let bounds = set
                        .node_bounds(child, &METRICS)
                        .or_else(|| set.node_bounds(node, &METRICS))?;
                    Some((node, bounds))
                })
            })
            .flatten();
        if let Some((node, b)) = node {
            self.quick_node.set(Some(node));
            self.view.scroll_to(&Bounds::new(b.x1, b.y1, b.x2, b.y2));
            self.queue_refresh();
        } else {
            entry.add_css_class("error");
        }
    }
    fn on_config(&self) {
        if self.updating.get() {
            return;
        }
        let name = self
            .glade
            .get::<gtk::Entry>("name_entry")
            .text()
            .to_string();
        let Ok(max) = self
            .glade
            .get::<gtk::Entry>("max_score_entry")
            .text()
            .parse::<u32>()
        else {
            return;
        };
        if max == 0 || max > 999 {
            return;
        }
        let all = self
            .glade
            .get::<gtk::CheckButton>("all_radiobutton")
            .is_active();
        let quota = self
            .glade
            .get::<gtk::SpinButton>("nb_qualified_spinbutton")
            .value_as_int()
            .max(0) as u32;
        let mode = if self
            .glade
            .get::<gtk::CheckButton>("all_places_radiobutton")
            .is_active()
        {
            FencedPlaces::AllPlaces
        } else if self
            .glade
            .get::<gtk::CheckButton>("third_place_radiobutton")
            .is_active()
        {
            FencedPlaces::ThirdPlaces
        } else {
            FencedPlaces::None
        };
        self.access(true, |stage, _| {
            stage.base_mut().name = name;
            stage.base_mut().nb_qualified = if all { None } else { Some(quota) };
            stage.set_max_score(max);
            stage.set_fenced_places(mode);
        });
        self.queue_refresh();
    }
    fn on_filter(&self) {
        let Some(page) = self.page.upgrade() else {
            return;
        };
        let classification = self
            .glade
            .get::<gtk::ToggleButton>("table_classification_toggletoolbutton")
            .is_active();
        let current = self.access(false, |s, _| {
            if classification {
                s.base().classification_filter.clone()
            } else {
                s.base().filter.clone()
            }
        });
        let Some(current) = current else { return };
        let mut filter = Self::filter(classification);
        filter.apply_list(&current);
        if filter_dialog::select_attributes(page.window().as_ref(), &mut filter, None) {
            self.access(false, |s, _| {
                if classification {
                    s.base_mut().classification_filter = filter.selected_codes();
                } else {
                    s.base_mut().filter = filter.selected_codes();
                }
            });
            page.mark_dirty();
            self.refresh();
        }
    }
    fn on_stuff(&self) {
        self.access(true, |s, ctx| {
            if ctx.settings.score_stuffing_allowed {
                s.stuff(ctx, &mut rand::thread_rng());
            }
        });
        self.queue_refresh();
    }
    fn on_print(&self) {
        let Some(snapshot) = self.snapshot() else {
            return;
        };
        let parent = self.page.upgrade().and_then(|p| p.window());
        let classification = self
            .glade
            .get::<gtk::ToggleButton>("table_classification_toggletoolbutton")
            .is_active();
        if classification {
            if let Err(e) = print::classification(parent.as_ref(), &snapshot) {
                dialogs::error(parent.as_ref(), &e.to_string());
            }
            return;
        }
        self.finish_printing(&snapshot, parent.as_ref(), print::choose_and_print(parent.as_ref(), &snapshot));
    }
    /// Marque les feuilles imprimées (seulement après une impression confirmée).
    fn finish_printing(
        &self,
        snapshot: &Snapshot,
        parent: Option<&gtk::Window>,
        result: Result<Vec<NodeId>, glib::Error>,
    ) {
        match result {
            Ok(printed) if !printed.is_empty() => {
                self.access(false, |s, _| {
                    if let Some(i) = snapshot.displayed {
                        if let Some(set) = s.table_sets().get(i).filter(|set| {
                            snapshot.set.as_ref().is_some_and(|old| old.id() == set.id())
                        }) {
                            let _ = set;
                            for node in printed {
                                s.table_set_mut(i).mark_printed(node);
                            }
                        }
                    }
                });
            }
            Err(e) => dialogs::error(parent, &e.to_string()),
            _ => {}
        }
    }
    fn on_print_sheets(&self) {
        self.print_sheets(None, None);
    }
    fn print_sheets(&self, table: Option<usize>, node: Option<NodeId>) {
        let Some(snapshot) = self.snapshot() else {
            return;
        };
        let parent = self.page.upgrade().and_then(|p| p.window());
        match print::score_sheets(parent.as_ref(), &snapshot, table, node) {
            Ok(printed) if !printed.is_empty() => {
                self.access(false, |s, _| {
                    if let Some(i) = snapshot.displayed {
                        if let Some(set) = s.table_sets().get(i).filter(|set| {
                            snapshot
                                .set
                                .as_ref()
                                .is_some_and(|old| old.id() == set.id())
                        }) {
                            let _ = set;
                            for node in printed {
                                s.table_set_mut(i).mark_printed(node);
                            }
                        }
                    }
                });
            }
            Err(e) => dialogs::error(parent.as_ref(), &e.to_string()),
            _ => {}
        }
    }
}

impl StageScreen for TableScreen {
    fn widget(&self) -> gtk::Widget {
        self.root.clone()
    }
    fn configuration(&self) -> Option<gtk::Widget> {
        Some(self.configuration.clone())
    }
    fn refresh(&self) {
        let Some(snapshot) = self.snapshot() else {
            return;
        };
        self.updating.set(true);
        self.fill_sets(&snapshot);
        self.classification.set_columns(snapshot.specs.clone());
        self.classification.set_rows(snapshot.rows.clone());
        self.glade
            .get::<gtk::Entry>("name_entry")
            .set_text(&snapshot.base.name);
        self.glade
            .get::<gtk::Entry>("max_score_entry")
            .set_text(&snapshot.base.max_score.unwrap_or(10).to_string());
        self.glade
            .get::<gtk::CheckButton>("all_radiobutton")
            .set_active(snapshot.base.nb_qualified.is_none());
        self.glade
            .get::<gtk::CheckButton>("number_radiobutton")
            .set_active(snapshot.base.nb_qualified.is_some());
        self.glade
            .get::<gtk::SpinButton>("nb_qualified_spinbutton")
            .set_value(snapshot.base.nb_qualified.unwrap_or(0) as f64);
        self.glade
            .get::<gtk::Widget>("nb_qualified_spinbutton")
            .set_visible(snapshot.base.nb_qualified.is_some());
        let mode = match snapshot.mode {
            FencedPlaces::ThirdPlaces => "third_place_radiobutton",
            FencedPlaces::AllPlaces => "all_places_radiobutton",
            _ => "no_radiobutton",
        };
        self.glade.get::<gtk::CheckButton>(mode).set_active(true);
        self.configuration.set_sensitive(!snapshot.base.locked);
        self.glade
            .get::<gtk::Widget>("stuff_toolbutton")
            .set_visible(snapshot.stuffing);
        self.glade
            .get::<gtk::Widget>("stuff_toolbutton")
            .set_sensitive(!snapshot.base.locked);
        self.display(&snapshot);
        self.toggle_views();
        self.updating.set(false);
    }
}

/// Résultats complets des tableaux : le tableau principal puis le classement
/// (impression de l'événement).
pub fn results_sources(c: &mut bp_core::contest::Contest, index: usize) -> Vec<Box<dyn bp_gtk::print::PageSource>> {
    let mut parts: Vec<Box<dyn bp_gtk::print::PageSource>> = Vec::new();
    let Some(mut snapshot) = TableScreen::build_snapshot(c, index) else { return parts };
    let root = c.schedule.stage(index).and_then(|s| s.as_any().downcast_ref::<TableSupervisor>()).and_then(|t| t.table_sets().first().cloned());
    if let Some(root) = root {
        snapshot.set = Some(root);
        snapshot.displayed = Some(0);
        parts.push(Box::new(print::TablesDoc::new(snapshot.clone(), 32)));
    }
    parts.push(print::classification_source(&snapshot));
    parts
}
