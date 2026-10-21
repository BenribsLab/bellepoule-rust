//! Vue de la salle (port de la partie graphique de `hall.cpp`, `piste.cpp`
//! et `timeline.cpp`) : pistes dessinées, sélection au lasso, déplacement,
//! dépôt des matchs et des arbitres, frise horaire et barre d'outils.

use std::cell::Cell;
use std::rc::{Rc, Weak};

use bp_core::i18n::tr;
use bp_gtk::{dialogs, glade::Glade};
use gtk::prelude::*;
use gtk4 as gtk;
use gtk::{gdk, glib};

use super::{draw, Ui};
use crate::assign::{AssignOptions, RefereeChoice};
use crate::hall::CursorKind;
use crate::piste::{H, W};
use crate::slot::{JobKey, PisteKey};

/// Glisser du curseur de la frise : sur le curseur lui-même, ou sur le fond
/// (défilement de la frise).
#[derive(Clone, Copy, PartialEq)]
enum TimelineDrag {
    None,
    Cursor { last: f64 },
    Slide { origin: f64, moved: bool },
}

pub struct HallView {
    ui: Weak<Ui>,
    glade: Glade,
    root: gtk::Widget,
    area: gtk::DrawingArea,
    timeline: gtk::DrawingArea,
    zoom: gtk::Adjustment,
    timeline_drag: Cell<TimelineDrag>,
    /// Bouton 1 enfoncé dans la salle.
    button1: Cell<bool>,
    updating: Cell<bool>,
}

impl HallView {
    pub fn new(ui: &Rc<Ui>) -> Rc<HallView> {
        let glade = Glade::new("hall.ui").expect("hall.ui");
        if ui.local_output.is_some() { super::embedded_layout::hall(&glade); }
        let root: gtk::Widget = glade.get("root");
        Glade::detach_from_parent(&root);
        glade.get::<gtk::Widget>("vbox1").set_vexpand(false);

        let area = gtk::DrawingArea::new();
        area.set_focusable(true);
        glade.get::<gtk::ScrolledWindow>("canvas_scrolled_window").set_child(Some(&area));
        glade.get::<gtk::Widget>("canvas_scrolled_window").set_vexpand(true);
        glade.get::<gtk::Widget>("canvas_scrolled_window").set_hexpand(true);

        let timeline = gtk::DrawingArea::new();
        timeline.set_content_height(50);
        timeline.set_hexpand(true);
        let tl_viewport: gtk::Viewport = glade.get("timeline_viewport");
        tl_viewport.set_vexpand(false);
        tl_viewport.set_child(Some(&timeline));

        let zoom: gtk::Adjustment = glade.get("zoom_adjustment");

        let view = Rc::new(HallView {
            ui: Rc::downgrade(ui),
            glade,
            root,
            area,
            timeline,
            zoom,
            timeline_drag: Cell::new(TimelineDrag::None),
            button1: Cell::new(false),
            updating: Cell::new(false),
        });
        view.connect();
        view
    }

    pub fn root(&self) -> gtk::Widget {
        self.root.clone()
    }

    fn ui(&self) -> Option<Rc<Ui>> {
        self.ui.upgrade()
    }

    /// Applique une action au modèle puis met l'interface à jour.
    fn act(&self, f: impl FnOnce(&mut crate::hall::Hall)) {
        if let Some(ui) = self.ui() {
            f(&mut ui.hall.borrow_mut());
            ui.after_change();
        }
    }

    fn scale(&self) -> f64 {
        self.zoom.value().max(0.1)
    }

    fn connect(self: &Rc<Self>) {
        // Dessin de la salle
        {
            let w = Rc::downgrade(self);
            self.area.set_draw_func(move |_, cr, _, _| {
                let Some(v) = w.upgrade() else { return };
                let Some(ui) = v.ui() else { return };
                let hall = ui.hall.borrow();
                cr.set_source_rgb(1.0, 1.0, 1.0);
                let _ = cr.paint();
                cr.scale(v.scale(), v.scale());
                for p in &hall.pistes {
                    draw::piste(cr, &hall, p);
                }
                if hall.lasso.is_active() {
                    if let Some(b) = hall.lasso.bounds {
                        cr.set_source_rgba(0.0, 0.0, 0.0, 0.6);
                        cr.set_line_width(1.0);
                        cr.set_dash(&[4.0, 4.0], 0.0);
                        cr.rectangle(b.x1, b.y1, b.x2 - b.x1, b.y2 - b.y1);
                        let _ = cr.stroke();
                        cr.set_dash(&[], 0.0);
                    }
                }
            });
        }
        {
            let w = Rc::downgrade(self);
            self.zoom.connect_value_changed(move |_| {
                if let Some(v) = w.upgrade() {
                    v.resize_area();
                    v.area.queue_draw();
                }
            });
        }

        // Souris : clics
        {
            let click = gtk::GestureClick::new();
            click.set_button(gdk::BUTTON_PRIMARY);
            let w = Rc::downgrade(self);
            click.connect_pressed(move |g, n, x, y| {
                let Some(v) = w.upgrade() else { return };
                v.area.grab_focus();
                v.button1.set(true);
                let ctrl = g.current_event_state().contains(gdk::ModifierType::CONTROL_MASK);
                let (hx, hy) = (x / v.scale(), y / v.scale());
                let piste = v.ui().and_then(|ui| ui.hall.borrow().piste_at(hx, hy));
                match piste {
                    Some(k) if n == 2 => v.show_piste_jobs(k),
                    Some(k) => v.act(|h| h.piste_press(k, ctrl, hx, hy)),
                    None => v.act(|h| h.background_press(hx, hy)),
                }
            });
            let w = Rc::downgrade(self);
            click.connect_released(move |_, _, x, y| {
                let Some(v) = w.upgrade() else { return };
                v.button1.set(false);
                let (hx, hy) = (x / v.scale(), y / v.scale());
                let on_piste = v.ui().and_then(|ui| ui.hall.borrow().piste_at(hx, hy)).is_some();
                v.act(|h| {
                    if on_piste || h.dragging.is_some() {
                        h.piste_release();
                    } else {
                        h.background_release();
                    }
                });
                v.resize_area();
            });
            self.area.add_controller(click);
        }

        // Numéro de piste : menu contextuel, indépendant du déplacement.
        {
            let click = gtk::GestureClick::new();
            click.set_button(gdk::BUTTON_SECONDARY);
            let weak = Rc::downgrade(self);
            click.connect_pressed(move |_, _, x, y| {
                if let Some(view) = weak.upgrade() {
                    view.piste_menu(x, y);
                }
            });
            self.area.add_controller(click);
        }

        // Souris : mouvements
        {
            let motion = gtk::EventControllerMotion::new();
            let w = Rc::downgrade(self);
            motion.connect_motion(move |m, x, y| {
                let Some(v) = w.upgrade() else { return };
                let Some(ui) = v.ui() else { return };
                let state = m.current_event_state();
                let ctrl = state.contains(gdk::ModifierType::CONTROL_MASK);
                let button1 = v.button1.get() && state.contains(gdk::ModifierType::BUTTON1_MASK);
                if !button1 {
                    v.button1.set(false);
                }
                let (hx, hy) = (x / v.scale(), y / v.scale());
                let needs_redraw = {
                    let mut hall = ui.hall.borrow_mut();
                    let before = (hall.lasso.bounds, hall.selected.clone(), hall.dragging);
                    hall.motion(hx, hy, button1, ctrl);
                    let after = (hall.lasso.bounds, hall.selected.clone(), hall.dragging);
                    before != after || button1
                };
                v.set_cursor(ui.hall.borrow().cursor);
                if needs_redraw {
                    v.area.queue_draw();
                    v.refresh_toolbar();
                }
            });
            self.area.add_controller(motion);
        }

        // Dépôt d'un match ou d'un arbitre
        {
            let drop = gtk::DropTarget::new(glib::Type::STRING, gdk::DragAction::COPY);
            let w = Rc::downgrade(self);
            drop.connect_enter(move |_, x, y| {
                let Some(v) = w.upgrade() else { return gdk::DragAction::empty() };
                v.drop_motion(x, y)
            });
            let w = Rc::downgrade(self);
            drop.connect_motion(move |_, x, y| {
                let Some(v) = w.upgrade() else { return gdk::DragAction::empty() };
                v.drop_motion(x, y)
            });
            let w = Rc::downgrade(self);
            drop.connect_leave(move |_| {
                if let Some(v) = w.upgrade() {
                    v.drop_leave();
                }
            });
            let w = Rc::downgrade(self);
            drop.connect_drop(move |_, value, x, y| {
                let Some(v) = w.upgrade() else { return false };
                let Ok(payload) = value.get::<String>() else { return false };
                v.drop_payload(&payload, x, y)
            });
            self.area.add_controller(drop);
        }

        // Frise
        {
            let w = Rc::downgrade(self);
            self.timeline.set_draw_func(move |_, cr, width, height| {
                let Some(v) = w.upgrade() else { return };
                let Some(ui) = v.ui() else { return };
                {
                    let mut hall = ui.hall.borrow_mut();
                    hall.timeline.width = width as f64;
                    hall.timeline.height = height as f64;
                }
                let hall = ui.hall.borrow();
                draw::timeline(cr, &hall, width as f64, height as f64);
            });
            let drag = gtk::GestureDrag::new();
            let w = Rc::downgrade(self);
            drag.connect_drag_begin(move |_, x, _| {
                let Some(v) = w.upgrade() else { return };
                let Some(ui) = v.ui() else { return };
                let cursor_x = ui.hall.borrow().timeline.cursor_x();
                if (x - cursor_x).abs() <= 6.0 || (x >= cursor_x && x <= cursor_x + 45.0) {
                    v.timeline_drag.set(TimelineDrag::Cursor { last: x });
                } else {
                    ui.hall.borrow_mut().timeline.slideout_origin = x;
                    v.timeline_drag.set(TimelineDrag::Slide { origin: x, moved: false });
                }
            });
            let w = Rc::downgrade(self);
            drag.connect_drag_update(move |g, dx, _| {
                let Some(v) = w.upgrade() else { return };
                let Some(ui) = v.ui() else { return };
                let Some((sx, _)) = g.start_point() else { return };
                let x = sx + dx;
                match v.timeline_drag.get() {
                    TimelineDrag::Cursor { last } => {
                        let width = ui.hall.borrow().timeline.width;
                        let x = x.clamp(2.0, width - 2.0);
                        let moved = ui.hall.borrow_mut().timeline.translate_cursor(x - last);
                        v.timeline_drag.set(TimelineDrag::Cursor { last: x });
                        if moved {
                            ui.hall.borrow_mut().on_timeline_cursor_moved();
                            v.redraw_all();
                        }
                    }
                    TimelineDrag::Slide { origin, .. } => {
                        {
                            let mut hall = ui.hall.borrow_mut();
                            hall.timeline.slide_out(x, false);
                            hall.on_timeline_cursor_moved();
                        }
                        v.timeline_drag.set(TimelineDrag::Slide { origin, moved: true });
                        v.redraw_all();
                    }
                    TimelineDrag::None => {}
                }
            });
            let w = Rc::downgrade(self);
            drag.connect_drag_end(move |g, dx, _| {
                let Some(v) = w.upgrade() else { return };
                let Some(ui) = v.ui() else { return };
                let x = g.start_point().map(|(sx, _)| sx + dx).unwrap_or(0.0);
                {
                    let mut hall = ui.hall.borrow_mut();
                    match v.timeline_drag.get() {
                        TimelineDrag::Cursor { .. } => {
                            let scale = hall.timeline.time_scale();
                            let round = crate::model::Timeline::rectification(hall.timeline.cursor);
                            hall.timeline.translate_cursor(round as f64 * scale);
                        }
                        TimelineDrag::Slide { moved, .. } => {
                            if !moved || dx.abs() < 1.0 {
                                let scale = hall.timeline.time_scale();
                                let cx = hall.timeline.cursor_x();
                                hall.timeline.translate_cursor(x - cx);
                                let round = crate::model::Timeline::rectification(hall.timeline.cursor);
                                hall.timeline.translate_cursor(round as f64 * scale);
                            } else {
                                hall.timeline.round_origin();
                                let rounded = hall.timeline.cursor_time(true);
                                hall.timeline.set_cursor_time(rounded);
                            }
                            hall.timeline.slideout_origin = 0.0;
                        }
                        TimelineDrag::None => {}
                    }
                    hall.on_timeline_cursor_moved();
                    hall.on_timeline_cursor_changed();
                }
                v.timeline_drag.set(TimelineDrag::None);
                ui.refresh();
            });
            self.timeline.add_controller(drag);
        }

        // Barre d'outils
        macro_rules! on {
            ($name:expr, $body:expr) => {{
                let w = Rc::downgrade(self);
                let f = $body;
                self.glade.connect_simple($name, move |_| {
                    if let Some(v) = w.upgrade() {
                        f(&v);
                    }
                });
            }};
        }
        on!("on_add_piste_button_clicked", |v: &Rc<HallView>| {
            v.act(|h| {
                h.add_piste(0.0, 0.0);
            });
            v.resize_area();
        });
        on!("on_remove_piste_button_clicked", |v: &Rc<HallView>| v.act(|h| h.remove_selected()));
        on!("on_rotate_piste_button_clicked", |v: &Rc<HallView>| v.act(|h| h.rotate_selected()));
        on!("on_clock_button_clicked", |v: &Rc<HallView>| v.act(|h| h.alter_clock()));
        on!("on_warning_checkbutton_toggled", |v: &Rc<HallView>| {
            if v.updating.get() {
                return;
            }
            let get = |id: &str| v.glade.get::<gtk::CheckButton>(id).is_active();
            let (club, league, country) = (get("club_checkbutton"), get("league_checkbutton"), get("country_checkbutton"));
            v.act(|h| {
                h.affinities.set_validity("club", club);
                h.affinities.set_validity("league", league);
                h.affinities.set_validity("country", country);
                h.on_new_warning_policy();
            });
        });
        on!("on_overlap_treeview_row_activated", |v: &Rc<HallView>| v.on_overlap_activated());
        {
            let w = Rc::downgrade(self);
            self.glade.get::<gtk::ToggleButton>("cone_togglebutton").connect_toggled(move |b| {
                let Some(v) = w.upgrade() else { return };
                if !v.updating.get() {
                    let active = b.is_active();
                    v.act(|h| h.block_selected(active));
                }
            });
        }
        {
            let w = Rc::downgrade(self);
            self.glade.get::<gtk::Button>("print_toolbutton").connect_clicked(move |_| {
                if let Some(v) = w.upgrade() {
                    v.print();
                }
            });
        }
        // Couleurs des cases d'avertissement (`SetWarningColors`).
        let css = gtk::CssProvider::new();
        let mut rules = String::new();
        if let Some(ui) = self.ui() {
            for title in crate::affinities::TITLES {
                if let Some(c) = ui.hall.borrow().affinities.color_of(title) {
                    rules.push_str(&format!(".bp-warning-{title} {{ background-color: {}; }}\n", c.to_css()));
                    self.glade.get::<gtk::Widget>(&format!("{title}_box")).add_css_class(&format!("bp-warning-{title}"));
                }
            }
        }
        css.load_from_data(&rules);
        if let Some(display) = gdk::Display::default() {
            gtk::style_context_add_provider_for_display(&display, &css, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
        }
        self.resize_area();
    }

    // --------------------------------------------------------------------------------
    // Glisser-déposer

    /// Survol pendant un glisser : la piste visée prend le focus, celles qui
    /// ne peuvent pas recevoir l'objet sont voilées.
    fn drop_motion(&self, x: f64, y: f64) -> gdk::DragAction {
        let Some(ui) = self.ui() else { return gdk::DragAction::empty() };
        let payload = self.current_payload();
        let (hx, hy) = (x / self.scale(), y / self.scale());
        let allowed = {
            let mut hall = ui.hall.borrow_mut();
            let target = hall.piste_at(hx, hy);
            let keys: Vec<PisteKey> = hall.pistes.iter().map(|p| p.key).collect();
            let mut allowed = false;
            for k in keys {
                let ok = match &payload {
                    Some(Payload::Job(j)) => hall.job_drop_allowed(*j, k),
                    Some(Payload::Referee(r)) => hall.referee_drop_allowed(*r, k),
                    None => true,
                };
                if Some(k) == target {
                    allowed = ok;
                }
                if let Some(p) = hall.piste_mut(k) {
                    p.blurred = payload.is_some() && !ok;
                    p.focused = Some(k) == target && ok;
                }
            }
            allowed || payload.is_none()
        };
        self.area.queue_draw();
        if allowed {
            gdk::DragAction::COPY
        } else {
            gdk::DragAction::empty()
        }
    }

    fn drop_leave(&self) {
        if let Some(ui) = self.ui() {
            let mut hall = ui.hall.borrow_mut();
            for p in hall.pistes.iter_mut() {
                p.blurred = false;
                p.focused = false;
            }
        }
        self.area.queue_draw();
    }

    fn drop_payload(&self, payload: &str, x: f64, y: f64) -> bool {
        let Some(ui) = self.ui() else { return false };
        let (hx, hy) = (x / self.scale(), y / self.scale());
        let Some(piste) = ui.hall.borrow().piste_at(hx, hy) else {
            self.drop_leave();
            return false;
        };
        let done = match Payload::parse(payload) {
            Some(Payload::Job(j)) => {
                let (allowed, assigned) = {
                    let hall = ui.hall.borrow();
                    (hall.job_drop_allowed(j, piste), hall.jobs.get(&j).is_some_and(|j| j.slot.is_some()))
                };
                if allowed {
                    if assigned {
                        // Terminer le glisser-déposer avant d'ouvrir une boucle
                        // modale GTK. Annuler ne touche pas la réservation.
                        let weak = Rc::downgrade(&ui);
                        glib::idle_add_local_once(move || {
                            if let Some(ui) = weak.upgrade() {
                                super::pool_resources::edit_with_piste(&ui, j, Some(piste));
                            }
                        });
                    } else {
                        ui.hall.borrow_mut().drop_job(j, piste);
                    }
                }
                allowed
            }
            Some(Payload::Referee(r)) if ui.hall.borrow().referee_drop_allowed(r, piste) => {
                ui.hall.borrow_mut().drop_referee(r, piste);
                true
            }
            _ => false,
        };
        self.drop_leave();
        ui.hall.borrow_mut().on_timeline_cursor_moved();
        ui.after_change();
        done
    }

    /// Objet en cours de glisser dans l'application (mémorisé par la source).
    fn current_payload(&self) -> Option<Payload> {
        DRAGGED.with(|d| d.borrow().as_deref().and_then(Payload::parse))
    }

    // --------------------------------------------------------------------------------
    // Divers

    fn set_cursor(&self, kind: CursorKind) {
        let name = match kind {
            CursorKind::Default => None,
            CursorKind::Fleur => Some("move"),
            CursorKind::Plus => Some("crosshair"),
            CursorKind::Cross => Some("not-allowed"),
        };
        self.area.set_cursor_from_name(name);
    }

    /// Taille du dessin : englobe toutes les pistes (plus une marge).
    fn resize_area(&self) {
        let Some(ui) = self.ui() else { return };
        let (mut w, mut h) = (W * 2.0, H * 10.0);
        for p in &ui.hall.borrow().pistes {
            let b = p.bounds();
            w = w.max(b.x2 + W / 2.0);
            h = h.max(b.y2 + H * 3.0);
        }
        self.area.set_content_width((w * self.scale()) as i32);
        self.area.set_content_height((h * self.scale()) as i32);
    }

    fn redraw_all(&self) {
        self.area.queue_draw();
        self.timeline.queue_draw();
    }

    fn refresh_toolbar(&self) {
        let Some(ui) = self.ui() else { return };
        let state = ui.hall.borrow().toolbar_state();
        self.updating.set(true);
        self.glade.get::<gtk::Widget>("toolbar").set_sensitive(state.enabled);
        self.glade.get::<gtk::Widget>("remove_piste_button").set_sensitive(state.remove);
        self.glade.get::<gtk::Widget>("rotate_piste_button").set_sensitive(state.rotate);
        let cone = self.glade.get::<gtk::ToggleButton>("cone_togglebutton");
        cone.set_sensitive(state.cone_sensitive);
        if state.cone_sensitive {
            cone.set_active(state.cone_active);
        }
        self.updating.set(false);
    }

    /// Réaffiche la salle, la frise, l'horloge et les avertissements.
    pub fn refresh(&self) {
        let Some(ui) = self.ui() else { return };
        self.refresh_toolbar();
        let clock = ui.hall.borrow_mut().clock.tick();
        self.glade.get::<gtk::Label>("clock_label").set_text(&clock);
        self.updating.set(true);
        for title in crate::affinities::TITLES {
            if let Some(i) = crate::affinities::AffinityRules::index_of(title) {
                let valid = ui.hall.borrow().affinities.validity[i];
                self.glade.get::<gtk::CheckButton>(&format!("{title}_checkbutton")).set_active(valid);
            }
        }
        self.updating.set(false);

        // Avertissements de chevauchement
        let store: gtk::ListStore = self.glade.get("overlap_liststore");
        store.clear();
        let overlaps = ui.hall.borrow().overlaps.clone();
        for o in &overlaps {
            let iter = store.append();
            store.set(&iter, &[(0, &"dialog-warning"), (1, &o.message), (3, &o.color.to_css())]);
        }
        self.glade.get::<gtk::Widget>("scrolledwindow1").set_visible(!overlaps.is_empty());
        self.resize_area();
        self.redraw_all();
    }

    /// `OnOverlapWarningActivated`.
    fn on_overlap_activated(&self) {
        let Some(ui) = self.ui() else { return };
        let tree: gtk::TreeView = self.glade.get("overlap_treeview");
        let Some((paths, _)) = Some(tree.selection().selected_rows()) else { return };
        let Some(index) = paths.first().and_then(|p| p.indices().first().copied()) else { return };
        let message = ui.hall.borrow().overlaps.get(index as usize).map(|o| o.message.clone()).unwrap_or_default();
        let dialog: gtk::MessageDialog = self.glade.get("slot_overlap_dialog");
        dialog.set_transient_for(Some(ui.window()));
        dialog.set_secondary_use_markup(true);
        dialog.set_secondary_text(Some(
            &tr("<b>%s</b> is delayed.\nDo you want to postpone the next bouts?\n")
                .replace("%s", &gtk::glib::markup_escape_text(&message)),
        ));
        let yes = dialogs::run_dialog(&dialog) == gtk::ResponseType::Yes;
        dialog.set_visible(false);
        if yes {
            self.act(|h| h.fix_overlap_warning(index as usize));
        }
    }

    fn piste_menu(self: &Rc<Self>, x: f64, y: f64) {
        let Some(ui) = self.ui() else { return };
        let piste = ui.hall.borrow().piste_at(x / self.scale(), y / self.scale());
        let Some(piste) = piste else { return };
        let popover = gtk::Popover::new();
        let menu = gtk::Box::new(gtk::Orientation::Vertical, 4);
        let button = gtk::Button::with_label(&tr("Change piste number…"));
        button.set_has_frame(false);
        let weak = Rc::downgrade(self);
        let weak_popover = popover.downgrade();
        button.connect_clicked(move |_| {
            if let Some(popover) = weak_popover.upgrade() { popover.popdown(); }
            if let Some(view) = weak.upgrade() { view.change_piste_number(piste); }
        });
        menu.append(&button);
        let pool = {
            let hall = ui.hall.borrow();
            hall.piste_displayed_slot(piste).and_then(|sk| hall.slots.get(&sk))
                .and_then(|s| s.jobs.iter().copied().find(|k| hall.jobs.get(k).is_some_and(|j| j.is_pool)))
        };
        if let Some(job) = pool {
            let resources = gtk::Button::with_label(&tr("Pool resources…"));
            resources.set_has_frame(false);
            let weak = Rc::downgrade(&ui);
            let weak_popover = popover.downgrade();
            resources.connect_clicked(move |_| {
                if let Some(p) = weak_popover.upgrade() { p.popdown(); }
                if let Some(ui) = weak.upgrade() { super::pool_resources::edit(&ui, job); }
            });
            menu.append(&resources);
        }
        popover.set_child(Some(&menu));
        popover.set_parent(&self.area);
        popover.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
        popover.connect_closed(|p| p.unparent());
        popover.popup();
    }

    fn change_piste_number(&self, piste: PisteKey) {
        let Some(ui) = self.ui() else { return };
        let number = ui.hall.borrow().piste(piste).map(|p| p.id);
        let Some(number) = number else { return };
        let dialog = gtk::Dialog::with_buttons(
            Some(&tr("Change piste number…")), Some(ui.window()), gtk::DialogFlags::MODAL,
            &[(&tr("_Cancel"), gtk::ResponseType::Cancel), (&tr("_OK"), gtk::ResponseType::Ok)],
        );
        dialog.set_default_response(gtk::ResponseType::Ok);
        let content = dialog.content_area();
        content.set_spacing(8);
        content.set_margin_top(12);
        content.set_margin_bottom(12);
        content.set_margin_start(12);
        content.set_margin_end(12);
        let label = gtk::Label::new(Some(&tr("Piste number")));
        let input = gtk::SpinButton::with_range(1.0, u32::MAX as f64, 1.0);
        input.set_numeric(true);
        input.set_value(number as f64);
        let error = gtk::Label::new(None);
        error.set_wrap(true);
        error.add_css_class("error");
        content.append(&label);
        content.append(&input);
        content.append(&error);
        input.grab_focus();
        while dialogs::run_dialog(&dialog) == gtk::ResponseType::Ok {
            input.update();
            let result = ui.hall.borrow_mut().renumber_piste(piste, input.value() as u32);
            match result {
                Ok(()) => { ui.after_change(); break; }
                Err(message) => error.set_text(&message),
            }
        }
        dialog.destroy();
    }

    /// Double-clic sur une piste : matchs de la piste (`Piste::OnDoubleClick`).
    fn show_piste_jobs(&self, piste: PisteKey) {
        let Some(ui) = self.ui() else { return };
        let jobs = ui.hall.borrow().piste_jobs(piste);
        if !jobs.is_empty() {
            super::batches::show_job_board(&ui, &jobs, jobs.first().copied());
        }
    }

    /// Dialogue « Placer les matchs » d'un lot (`OnBatchAssignmentRequest`).
    pub fn assign_batch(&self, competition: u32, batch: u32, selected_jobs: Vec<JobKey>) {
        let Some(ui) = self.ui() else { return };
        let weapon = ui.hall.borrow().competition(competition).map(|c| c.weapon.clone()).unwrap_or_default();
        ui.expose_weapon(&weapon);
        if ui.hall.borrow().pistes.is_empty() {
            return;
        }
        for id in ["all_jobs", "all_referees", "all_pistes"] {
            self.glade.get::<gtk::CheckButton>(id).set_active(true);
        }
        {
            let mut hall = ui.hall.borrow_mut();
            let cursor = hall.timeline.cursor_time(true);
            hall.timeline.set_cursor_time(cursor);
            hall.on_timeline_cursor_moved();
        }
        let cursor = ui.hall.borrow().timeline.cursor_time(true);
        {
            use chrono::Timelike;
            self.glade.get::<gtk::Label>("cursor_date").set_markup(&format!(
                "<span weight=\"bold\" background=\"black\" foreground=\"white\"> {:02}:{:02} </span>",
                cursor.hour(),
                cursor.minute()
            ));
        }
        let dialog: gtk::Dialog = self.glade.get("job_dialog");
        dialog.set_transient_for(Some(ui.window()));
        let response = dialogs::run_dialog(&dialog);
        dialog.set_visible(false);
        if response != gtk::ResponseType::Other(0) {
            return;
        }
        let checked = |id: &str| self.glade.get::<gtk::CheckButton>(id).is_active();
        let referees = if checked("all_referees") {
            RefereeChoice::All
        } else if checked("selected_referees") {
            RefereeChoice::Selected(ui.selected_referees(&weapon))
        } else {
            RefereeChoice::None
        };
        let referees = if ui.management_features().referees { referees } else { RefereeChoice::None };
        let options = AssignOptions { all_jobs: checked("all_jobs"), selected_jobs, referees, all_pistes: checked("all_pistes") };
        let no_referee_available = options.referees != RefereeChoice::None
            && !ui.hall.borrow().referees.weapon_has_referees(&weapon);
        if no_referee_available {
            let d: gtk::MessageDialog = self.glade.get("referee_error_dialog");
            d.set_transient_for(Some(ui.window()));
            dialogs::run_dialog(&d);
            d.set_visible(false);
        }
        self.act(|h| h.batch_assignment(competition, batch, &options));
    }

    /// Impression de la salle (`CanvasModule::Print`).
    fn print(&self) {
        let Some(ui) = self.ui() else { return };
        let weak = self.ui.clone();
        let result = bp_gtk::print::run_document(Some(ui.window()), &tr("Hall"), move || {
            Box::new(HallPrint { ui: weak.clone() })
        });
        if let Err(e) = result {
            dialogs::error(Some(ui.window()), &e.to_string());
        }
    }
}

struct HallPrint {
    ui: Weak<Ui>,
}

impl bp_gtk::print::PageSource for HallPrint {
    fn begin(&mut self, _width: f64, _height: f64) -> usize {
        1
    }

    fn draw(&mut self, _page: usize, cr: &cairo::Context, width: f64, height: f64) {
        let Some(ui) = self.ui.upgrade() else { return };
        let hall = ui.hall.borrow();
        let (mut w, mut h) = (1.0f64, 1.0f64);
        for p in &hall.pistes {
            let b = p.bounds();
            w = w.max(b.x2 + 15.0);
            h = h.max(b.y2 + 15.0);
        }
        let factor = (width / w).min(height / h);
        cr.scale(factor, factor);
        for p in &hall.pistes {
            draw::piste(cr, &hall, p);
        }
    }
}

/// Objet glissé : un match (`bellepoule/job`) ou un arbitre
/// (`bellepoule/referee`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Payload {
    Job(JobKey),
    Referee(u32),
}

impl Payload {
    pub fn parse(s: &str) -> Option<Payload> {
        let (kind, value) = s.split_once(':')?;
        let n = value.parse().ok()?;
        match kind {
            "job" => Some(Payload::Job(n)),
            "referee" => Some(Payload::Referee(n)),
            _ => None,
        }
    }

    pub fn text(&self) -> String {
        match self {
            Payload::Job(j) => format!("job:{j}"),
            Payload::Referee(r) => format!("referee:{r}"),
        }
    }
}

thread_local! {
    /// Objet en cours de glisser (GTK ne lit le contenu qu'au dépôt).
    pub static DRAGGED: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

/// Source de glisser pour un match ou un arbitre.
/// `payload` reçoit l'indice de la ligne sous la souris (liste).
pub fn drag_source(tree: &gtk::TreeView, payload: impl Fn(Option<usize>) -> Option<Payload> + 'static) {
    let source = gtk::DragSource::new();
    source.set_actions(gdk::DragAction::COPY);
    let t = tree.clone();
    source.connect_prepare(move |_, x, y| {
        let (bx, by) = t.convert_widget_to_bin_window_coords(x as i32, y as i32);
        let row = t.path_at_pos(bx, by).and_then(|(p, ..)| p).and_then(|p| p.indices().first().map(|i| *i as usize));
        if let Some(r) = row {
            let path = gtk::TreePath::from_indices(&[r as i32]);
            if !t.selection().path_is_selected(&path) {
                t.selection().unselect_all();
                t.selection().select_path(&path);
            }
        }
        let p = payload(row)?;
        let text = p.text();
        DRAGGED.with(|d| *d.borrow_mut() = Some(text.clone()));
        Some(gdk::ContentProvider::for_value(&text.to_value()))
    });
    source.connect_drag_end(|_, _, _| {
        DRAGGED.with(|d| *d.borrow_mut() = None);
    });
    tree.add_controller(source);
}
