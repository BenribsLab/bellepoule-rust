//! Affichage d'un canevas à l'écran (équivalent de `GooCanvas` comme widget) :
//! défilement, zoom, clics, survol, et widgets GTK posés sur la scène.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk::prelude::*;
use gtk4 as gtk;

use super::{Bounds, Canvas, CanvasEvent, Color, EventKind, ItemId};

struct State {
    canvas: Canvas,
    zoom: Cell<f64>,
    background: Cell<Color>,
    hovered: RefCell<Option<ItemId>>,
    area: gtk::DrawingArea,
    fixed: gtk::Fixed,
}

/// Vue défilante et zoomable d'un [`Canvas`].
#[derive(Clone)]
pub struct CanvasView {
    scrolled: gtk::ScrolledWindow,
    state: Rc<State>,
}

impl CanvasView {
    pub fn new(canvas: &Canvas) -> CanvasView {
        let area = gtk::DrawingArea::new();
        let fixed = gtk::Fixed::new();
        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(&area));
        overlay.add_overlay(&fixed);
        fixed.set_can_target(false);

        let scrolled = gtk::ScrolledWindow::new();
        scrolled.set_child(Some(&overlay));
        scrolled.set_hexpand(true);
        scrolled.set_vexpand(true);

        let state = Rc::new(State {
            canvas: canvas.clone(),
            zoom: Cell::new(1.0),
            background: Cell::new(Color::WHITE),
            hovered: RefCell::new(None),
            area: area.clone(),
            fixed,
        });
        let view = CanvasView { scrolled, state };
        view.install_drawing();
        view.install_events();
        view.install_updates();
        view.refresh();
        view
    }

    pub fn widget(&self) -> &gtk::ScrolledWindow {
        &self.scrolled
    }

    pub fn canvas(&self) -> &Canvas {
        &self.state.canvas
    }

    pub fn zoom(&self) -> f64 {
        self.state.zoom.get()
    }

    /// Facteur d'agrandissement (1,0 = taille réelle).
    pub fn set_zoom(&self, zoom: f64) {
        self.state.zoom.set(zoom.clamp(0.1, 10.0));
        self.refresh();
    }

    pub fn set_background(&self, c: Color) {
        self.state.background.set(c);
        self.state.area.queue_draw();
    }

    /// Taille de la scène à l'écran, mise à jour du défilement et du dessin.
    pub fn refresh(&self) {
        let s = &self.state;
        let b = s.canvas.scene_bounds();
        let zoom = s.zoom.get();
        let w = ((b.x2.max(0.0)) * zoom).ceil() as i32 + 1;
        let h = ((b.y2.max(0.0)) * zoom).ceil() as i32 + 1;
        s.area.set_content_width(w);
        s.area.set_content_height(h);
        self.place_widgets();
        s.area.queue_draw();
    }

    fn install_drawing(&self) {
        let st = self.state.clone();
        self.state.area.set_draw_func(move |_, cr, _, _| {
            let bg = st.background.get();
            cr.set_source_rgba(bg.r, bg.g, bg.b, bg.a);
            let _ = cr.paint();
            st.canvas.render(cr, None, st.zoom.get());
        });
    }

    fn install_updates(&self) {
        let weak = Rc::downgrade(&self.state);
        let scrolled = self.scrolled.clone();
        self.state.canvas.connect_changed(move || {
            // Le dessin est reporté : plusieurs modifications de suite ne
            // provoquent qu'un seul rafraîchissement.
            if let Some(st) = weak.upgrade() {
                let view = CanvasView { scrolled: scrolled.clone(), state: st };
                let v2 = view.clone();
                glib_idle(move || v2.refresh());
            }
        });
    }

    /// Place les widgets GTK posés sur la scène.
    fn place_widgets(&self) {
        let s = &self.state;
        let zoom = s.zoom.get();
        let items = s.canvas.widget_items();
        for (_, w, b, visible) in items {
            if w.parent().map(|p| p != *s.fixed.upcast_ref::<gtk::Widget>()).unwrap_or(true) {
                if let Some(p) = w.parent() {
                    let _ = p;
                    w.unparent();
                }
                s.fixed.put(&w, b.x1 * zoom, b.y1 * zoom);
            }
            s.fixed.move_(&w, b.x1 * zoom, b.y1 * zoom);
            w.set_size_request(((b.width()) * zoom).max(1.0) as i32, ((b.height()) * zoom).max(1.0) as i32);
            w.set_visible(visible);
        }
        s.fixed.set_can_target(true);
    }

    fn install_events(&self) {
        let area = &self.state.area;

        let click = gtk::GestureClick::new();
        click.set_button(0);
        {
            let st = self.state.clone();
            click.connect_pressed(move |g, n, x, y| {
                Self::dispatch_at(&st, EventKind::Press, g.current_button(), n, x, y, g.current_event_state());
            });
        }
        {
            let st = self.state.clone();
            click.connect_released(move |g, n, x, y| {
                Self::dispatch_at(&st, EventKind::Release, g.current_button(), n, x, y, g.current_event_state());
            });
        }
        area.add_controller(click);

        let motion = gtk::EventControllerMotion::new();
        {
            let st = self.state.clone();
            motion.connect_motion(move |m, x, y| {
                Self::on_motion(&st, x, y, m.current_event_state());
            });
        }
        {
            let st = self.state.clone();
            motion.connect_leave(move |_| {
                if let Some(old) = st.hovered.borrow_mut().take() {
                    let ev = Self::event(&st, 0.0, 0.0, 0, 0, gtk::gdk::ModifierType::empty());
                    st.canvas.dispatch(old, EventKind::Leave, &ev);
                }
            });
        }
        area.add_controller(motion);
    }

    fn event(st: &State, wx: f64, wy: f64, button: u32, n_press: i32, state: gtk::gdk::ModifierType) -> CanvasEvent {
        let z = st.zoom.get();
        let (x, y) = (wx / z, wy / z);
        CanvasEvent { x, y, item_x: x, item_y: y, widget_x: wx, widget_y: wy, button, n_press, state }
    }

    fn dispatch_at(st: &Rc<State>, kind: EventKind, button: u32, n: i32, wx: f64, wy: f64, state: gtk::gdk::ModifierType) {
        let ev = Self::event(st, wx, wy, button, n, state);
        if let Some(target) = st.canvas.item_at(ev.x, ev.y) {
            st.canvas.dispatch(target, kind, &ev);
        }
    }

    fn on_motion(st: &Rc<State>, wx: f64, wy: f64, state: gtk::gdk::ModifierType) {
        let ev = Self::event(st, wx, wy, 0, 0, state);
        let target = st.canvas.item_at(ev.x, ev.y);
        let previous = *st.hovered.borrow();
        if previous != target {
            if let Some(old) = previous {
                if st.canvas.exists(old) {
                    st.canvas.dispatch(old, EventKind::Leave, &ev);
                }
            }
            if let Some(new) = target {
                st.canvas.dispatch(new, EventKind::Enter, &ev);
            }
            *st.hovered.borrow_mut() = target;
        }
        if let Some(t) = target {
            st.canvas.dispatch(t, EventKind::Motion, &ev);
        }
    }

    /// Fait défiler la vue pour montrer `bounds` (coordonnées canevas).
    pub fn scroll_to(&self, bounds: &Bounds) {
        let z = self.zoom();
        let h = self.scrolled.hadjustment();
        let v = self.scrolled.vadjustment();
        h.set_value((bounds.x1 * z - 10.0).max(0.0));
        v.set_value((bounds.y1 * z - 10.0).max(0.0));
    }
}

fn glib_idle(f: impl FnOnce() + 'static) {
    let f = RefCell::new(Some(f));
    gtk::glib::idle_add_local_once(move || {
        if let Some(f) = f.borrow_mut().take() {
            f();
        }
    });
}
