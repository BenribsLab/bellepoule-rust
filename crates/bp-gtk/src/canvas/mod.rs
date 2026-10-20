//! Canevas vectoriel remplaçant GooCanvas (port de `common/util/canvas.cpp`
//! et des usages de GooCanvas dans BellePoule).
//!
//! Une [`Canvas`] est une scène d'éléments hiérarchiques (groupes,
//! rectangles, textes, images, lignes brisées, tableaux, widgets GTK) dessinée
//! avec cairo. La même scène sert :
//! - à l'écran, par l'intermédiaire d'un [`CanvasView`] (zoom, défilement,
//!   clic, survol, saisie dans une cellule) ;
//! - à l'impression (`GtkPrintOperation`) et à l'export PDF/SVG/PNG, par
//!   [`Canvas::render`].
//!
//! Les unités sont celles de GooCanvas : des pixels à la résolution du
//! canevas (72 ppp par défaut). Les éléments sont désignés par un
//! [`ItemId`] ; les propriétés se règlent avec [`Canvas::set`] et des
//! [`Prop`], à la manière de `g_object_set`.

mod color;
mod view;

pub use color::Color;
pub use view::CanvasView;

use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gtk4 as gtk;
use gtk::gdk;
use gtk::gdk_pixbuf::Pixbuf;
use gtk::pango;
use gtk::pango::prelude::FontMapExt;

/// Police par défaut de BellePoule (`BP_FONT`).
pub const BP_FONT: &str = "Sans ";

/// Identifiant d'un élément de la scène.
pub type ItemId = usize;

/// Boîte englobante (`GooCanvasBounds`).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Bounds {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
}

impl Bounds {
    pub fn new(x1: f64, y1: f64, x2: f64, y2: f64) -> Self {
        Bounds { x1, y1, x2, y2 }
    }

    pub fn width(&self) -> f64 {
        self.x2 - self.x1
    }

    pub fn height(&self) -> f64 {
        self.y2 - self.y1
    }

    pub fn is_empty(&self) -> bool {
        self.x2 < self.x1 || self.y2 < self.y1
    }

    fn empty() -> Self {
        Bounds { x1: f64::MAX, y1: f64::MAX, x2: f64::MIN, y2: f64::MIN }
    }

    pub fn union(&self, o: &Bounds) -> Bounds {
        if self.is_empty() {
            return *o;
        }
        if o.is_empty() {
            return *self;
        }
        Bounds { x1: self.x1.min(o.x1), y1: self.y1.min(o.y1), x2: self.x2.max(o.x2), y2: self.y2.max(o.y2) }
    }

    pub fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x1 && x <= self.x2 && y >= self.y1 && y <= self.y2
    }

    /// Boîte transformée par une matrice (enveloppe des 4 coins).
    pub fn transform(&self, m: &cairo::Matrix) -> Bounds {
        if self.is_empty() {
            return *self;
        }
        let mut b = Bounds::empty();
        for (x, y) in [(self.x1, self.y1), (self.x2, self.y1), (self.x1, self.y2), (self.x2, self.y2)] {
            let (tx, ty) = m.transform_point(x, y);
            b.x1 = b.x1.min(tx);
            b.y1 = b.y1.min(ty);
            b.x2 = b.x2.max(tx);
            b.y2 = b.y2.max(ty);
        }
        b
    }
}

/// Point d'ancrage d'un texte ou d'un widget (`GtkAnchorType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    NW,
    N,
    NE,
    W,
    Center,
    E,
    SW,
    S,
    SE,
}

impl Anchor {
    /// Fractions (horizontale, verticale) à retrancher de la taille.
    fn fractions(self) -> (f64, f64) {
        match self {
            Anchor::NW => (0.0, 0.0),
            Anchor::N => (0.5, 0.0),
            Anchor::NE => (1.0, 0.0),
            Anchor::W => (0.0, 0.5),
            Anchor::Center => (0.5, 0.5),
            Anchor::E => (1.0, 0.5),
            Anchor::SW => (0.0, 1.0),
            Anchor::S => (0.5, 1.0),
            Anchor::SE => (1.0, 1.0),
        }
    }
}

/// Alignement relatif (`Canvas::Alignment`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alignment {
    Start,
    Middle,
    End,
}

/// Propriétés réglables d'un élément (équivalent des propriétés GObject).
#[derive(Clone)]
pub enum Prop {
    X(f64),
    Y(f64),
    Width(f64),
    Height(f64),
    RadiusX(f64),
    RadiusY(f64),
    /// Couleur de remplissage (nom CSS/X11, `#rrggbb`…).
    FillColor(String),
    /// `fill-color-rgba` : 0xRRGGBBAA.
    FillColorRgba(u32),
    /// Pas de remplissage (`"fill-pattern", NULL`).
    NoFill,
    StrokeColor(String),
    StrokeColorRgba(u32),
    /// Pas de contour (`"stroke-pattern", NULL`).
    NoStroke,
    LineWidth(f64),
    /// Pointillés (`goo_canvas_line_dash_new`).
    LineDash(Vec<f64>),
    Font(String),
    Text(String),
    UseMarkup(bool),
    Alignment(pango::Alignment),
    Wrap(pango::WrapMode),
    Ellipsize(pango::EllipsizeMode),
    Anchor(Anchor),
    /// `visibility` : visible ou caché.
    Visible(bool),
    /// `pointer-events` : l'élément reçoit-il les événements ?
    PointerEvents(bool),
    ScaleToFit(bool),
    Pixbuf(Option<Pixbuf>),
    Tooltip(String),
    RowSpacing(f64),
    ColumnSpacing(f64),
    HorzGridLineWidth(f64),
    VertGridLineWidth(f64),
    HomogeneousRows(bool),
    HomogeneousColumns(bool),
    /// Points d'une ligne brisée.
    Points(Vec<(f64, f64)>),
    ClosePath(bool),
}

/// Propriétés de placement d'un élément dans un tableau
/// (`goo_canvas_item_set_child_property`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CellProps {
    pub row: u32,
    pub column: u32,
    pub rows: u32,
    pub columns: u32,
    pub x_align: f64,
    pub y_align: f64,
    pub x_expand: bool,
    pub x_fill: bool,
    pub x_shrink: bool,
    pub y_expand: bool,
    pub y_fill: bool,
    pub y_shrink: bool,
    pub left_padding: f64,
    pub right_padding: f64,
    pub top_padding: f64,
    pub bottom_padding: f64,
}

impl Default for CellProps {
    fn default() -> Self {
        CellProps {
            row: 0,
            column: 0,
            rows: 1,
            columns: 1,
            x_align: 0.5,
            y_align: 0.5,
            x_expand: false,
            x_fill: false,
            x_shrink: false,
            y_expand: false,
            y_fill: false,
            y_shrink: false,
            left_padding: 0.0,
            right_padding: 0.0,
            top_padding: 0.0,
            bottom_padding: 0.0,
        }
    }
}

#[derive(Clone)]
pub(crate) struct TextShape {
    text: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    anchor: Anchor,
    font: String,
    markup: bool,
    alignment: pango::Alignment,
    wrap: pango::WrapMode,
    ellipsize: pango::EllipsizeMode,
}

#[derive(Clone)]
pub(crate) struct TableShape {
    x: f64,
    y: f64,
    width: Option<f64>,
    height: Option<f64>,
    row_spacing: f64,
    column_spacing: f64,
    horz_grid: f64,
    vert_grid: f64,
    homogeneous_rows: bool,
    homogeneous_columns: bool,
    /// Positions calculées des lignes et colonnes (début, taille).
    rows: Vec<(f64, f64)>,
    cols: Vec<(f64, f64)>,
    size: (f64, f64),
}

#[derive(Clone)]
pub(crate) enum Shape {
    Group,
    Rect { x: f64, y: f64, w: f64, h: f64, rx: f64, ry: f64 },
    Text(TextShape),
    Image { pixbuf: Option<Pixbuf>, x: f64, y: f64, width: Option<f64>, height: Option<f64>, scale_to_fit: bool },
    Polyline { points: Vec<(f64, f64)>, closed: bool },
    Table(TableShape),
    Widget { widget: gtk::Widget, x: f64, y: f64, w: f64, h: f64, anchor: Anchor },
}

/// Gestionnaire d'événement d'élément : renvoie vrai si l'événement est traité.
pub type EventHandler = Rc<dyn Fn(&Canvas, ItemId, ItemId, &CanvasEvent) -> bool>;

#[derive(Default, Clone)]
struct Handlers {
    press: Vec<EventHandler>,
    release: Vec<EventHandler>,
    motion: Vec<EventHandler>,
    enter: Vec<EventHandler>,
    leave: Vec<EventHandler>,
}

pub(crate) struct Node {
    parent: Option<ItemId>,
    children: Vec<ItemId>,
    shape: Shape,
    fill: Option<Color>,
    stroke: Option<Color>,
    line_width: f64,
    dash: Option<Vec<f64>>,
    transform: cairo::Matrix,
    table_offset: (f64, f64),
    visible: bool,
    pointer_events: bool,
    cell: Option<CellProps>,
    tooltip: Option<String>,
    data: HashMap<String, Rc<dyn Any>>,
    handlers: Handlers,
}

impl Node {
    fn new(parent: Option<ItemId>, shape: Shape) -> Node {
        let (fill, stroke) = match &shape {
            Shape::Text(_) => (Some(Color::BLACK), None),
            Shape::Rect { .. } | Shape::Polyline { .. } => (None, Some(Color::BLACK)),
            Shape::Table(_) => (None, Some(Color::BLACK)),
            _ => (None, None),
        };
        Node {
            parent,
            children: Vec::new(),
            shape,
            fill,
            stroke,
            line_width: 2.0,
            dash: None,
            transform: cairo::Matrix::identity(),
            table_offset: (0.0, 0.0),
            visible: true,
            pointer_events: true,
            cell: None,
            tooltip: None,
            data: HashMap::new(),
            handlers: Handlers::default(),
        }
    }

    /// Matrice de l'élément dans l'espace de son parent.
    fn matrix(&self) -> cairo::Matrix {
        let off = cairo::Matrix::new(1.0, 0.0, 0.0, 1.0, self.table_offset.0, self.table_offset.1);
        cairo::Matrix::multiply(&self.transform, &off)
    }
}

/// Événement pointeur transmis aux gestionnaires d'éléments.
#[derive(Debug, Clone, Copy)]
pub struct CanvasEvent {
    /// Coordonnées dans l'espace du canevas.
    pub x: f64,
    pub y: f64,
    /// Coordonnées dans l'espace de l'élément qui reçoit l'événement.
    pub item_x: f64,
    pub item_y: f64,
    /// Coordonnées dans le widget (pixels écran, zoom compris).
    pub widget_x: f64,
    pub widget_y: f64,
    pub button: u32,
    pub n_press: i32,
    pub state: gdk::ModifierType,
}

pub(crate) struct Scene {
    nodes: Vec<Option<Node>>,
    root: ItemId,
    dirty: bool,
    resolution: f64,
    listeners: Vec<Rc<dyn Fn()>>,
    widgets_changed: bool,
}

thread_local! {
    static PANGO_CONTEXT: pango::Context = {
        let fm = pangocairo::FontMap::default();
        let ctx = fm.create_context();
        let mut opts = cairo::FontOptions::new().expect("cairo::FontOptions");
        opts.set_hint_metrics(cairo::HintMetrics::Off);
        opts.set_hint_style(cairo::HintStyle::None);
        pangocairo::functions::context_set_font_options(&ctx, Some(&opts));
        ctx
    };
}

fn make_layout(t: &TextShape) -> pango::Layout {
    PANGO_CONTEXT.with(|ctx| {
        let layout = pango::Layout::new(ctx);
        let fd = pango::FontDescription::from_string(&t.font);
        layout.set_font_description(Some(&fd));
        if t.markup {
            layout.set_markup(&t.text);
        } else {
            layout.set_text(&t.text);
        }
        if t.width > 0.0 {
            layout.set_width((t.width * pango::SCALE as f64) as i32);
            layout.set_wrap(t.wrap);
            layout.set_ellipsize(t.ellipsize);
        }
        if t.height > 0.0 && t.width > 0.0 {
            layout.set_height((t.height * pango::SCALE as f64) as i32);
        }
        layout.set_alignment(t.alignment);
        layout
    })
}

/// Origine de dessin et boîte logique d'un texte.
fn text_geometry(t: &TextShape) -> (pango::Layout, f64, f64, Bounds) {
    let layout = make_layout(t);
    let (_, logical) = layout.extents();
    let s = pango::SCALE as f64;
    let lx = logical.x() as f64 / s;
    let ly = logical.y() as f64 / s;
    let mut lw = logical.width() as f64 / s;
    let mut lh = logical.height() as f64 / s;
    if t.width > 0.0 {
        lw = t.width;
    }
    if t.height > 0.0 && t.width > 0.0 {
        lh = lh.min(t.height).max(0.0);
    }
    let (fx, fy) = t.anchor.fractions();
    let ox = t.x - fx * lw;
    let oy = t.y - fy * lh;
    let bx = if t.width > 0.0 { ox } else { ox + lx };
    let b = Bounds::new(bx, oy + ly, bx + lw, oy + ly + lh);
    (layout, ox, oy, b)
}

fn image_size(pixbuf: &Option<Pixbuf>, width: Option<f64>, height: Option<f64>) -> (f64, f64) {
    let (pw, ph) = pixbuf.as_ref().map(|p| (p.width() as f64, p.height() as f64)).unwrap_or((0.0, 0.0));
    (width.unwrap_or(pw), height.unwrap_or(ph))
}

impl Scene {
    fn node(&self, id: ItemId) -> Option<&Node> {
        self.nodes.get(id).and_then(|n| n.as_ref())
    }

    fn node_mut(&mut self, id: ItemId) -> Option<&mut Node> {
        self.nodes.get_mut(id).and_then(|n| n.as_mut())
    }

    /// Boîte de l'élément dans son propre espace (avant sa transformation).
    fn local_bounds(&self, id: ItemId) -> Bounds {
        let Some(n) = self.node(id) else { return Bounds::empty() };
        if !n.visible {
            return Bounds::empty();
        }
        let half = if n.stroke.is_some() { n.line_width / 2.0 } else { 0.0 };
        match &n.shape {
            Shape::Group => {
                let mut b = Bounds::empty();
                for c in &n.children {
                    b = b.union(&self.child_bounds(*c));
                }
                b
            }
            Shape::Rect { x, y, w, h, .. } => Bounds::new(x - half, y - half, x + w + half, y + h + half),
            Shape::Text(t) => text_geometry(t).3,
            Shape::Image { pixbuf, x, y, width, height, .. } => {
                let (w, h) = image_size(pixbuf, *width, *height);
                Bounds::new(*x, *y, x + w, y + h)
            }
            Shape::Polyline { points, .. } => {
                let mut b = Bounds::empty();
                for (x, y) in points {
                    b = b.union(&Bounds::new(x - half, y - half, x + half, y + half));
                }
                b
            }
            Shape::Table(t) => {
                let mut b = Bounds::new(t.x, t.y, t.x + t.size.0, t.y + t.size.1);
                for c in &n.children {
                    b = b.union(&self.child_bounds(*c));
                }
                b
            }
            Shape::Widget { x, y, w, h, anchor, .. } => {
                let (fx, fy) = anchor.fractions();
                Bounds::new(x - fx * w, y - fy * h, x - fx * w + w, y - fy * h + h)
            }
        }
    }

    /// Boîte d'un enfant dans l'espace de son parent.
    fn child_bounds(&self, id: ItemId) -> Bounds {
        let Some(n) = self.node(id) else { return Bounds::empty() };
        self.local_bounds(id).transform(&n.matrix())
    }

    /// Matrice élément -> canevas.
    fn item_to_canvas(&self, id: ItemId) -> cairo::Matrix {
        let mut m = cairo::Matrix::identity();
        let mut cur = Some(id);
        while let Some(i) = cur {
            let Some(n) = self.node(i) else { break };
            m = cairo::Matrix::multiply(&m, &n.matrix());
            cur = n.parent;
        }
        m
    }

    /// Mise en page des tableaux (récursive, des feuilles vers la racine).
    fn layout(&mut self, id: ItemId) {
        let children = match self.node(id) {
            Some(n) => n.children.clone(),
            None => return,
        };
        for c in &children {
            self.layout(*c);
        }
        let Some(Shape::Table(_)) = self.node(id).map(|n| &n.shape) else { return };

        struct Entry {
            id: ItemId,
            cell: CellProps,
            b: Bounds,
        }
        let mut entries = Vec::new();
        for c in &children {
            let Some(n) = self.node(*c) else { continue };
            let cell = n.cell.unwrap_or_default();
            let b = self.local_bounds(*c).transform(&n.transform);
            entries.push(Entry { id: *c, cell, b });
        }
        let Some(Shape::Table(t)) = self.node(id).map(|n| n.shape.clone()) else { return };
        let n_rows = entries.iter().map(|e| e.cell.row + e.cell.rows.max(1)).max().unwrap_or(0) as usize;
        let n_cols = entries.iter().map(|e| e.cell.column + e.cell.columns.max(1)).max().unwrap_or(0) as usize;
        let mut widths = vec![0.0f64; n_cols];
        let mut heights = vec![0.0f64; n_rows];
        let mut col_expand = vec![false; n_cols];
        let mut row_expand = vec![false; n_rows];
        let size = |e: &Entry| {
            let w = if e.b.is_empty() { 0.0 } else { e.b.width() };
            let h = if e.b.is_empty() { 0.0 } else { e.b.height() };
            (w + e.cell.left_padding + e.cell.right_padding, h + e.cell.top_padding + e.cell.bottom_padding)
        };
        for pass in 0..2 {
            for e in &entries {
                let (w, h) = size(e);
                let (c0, cs) = (e.cell.column as usize, e.cell.columns.max(1) as usize);
                let (r0, rs) = (e.cell.row as usize, e.cell.rows.max(1) as usize);
                if pass == 0 {
                    if cs == 1 {
                        widths[c0] = widths[c0].max(w);
                        if e.cell.x_expand {
                            col_expand[c0] = true;
                        }
                    }
                    if rs == 1 {
                        heights[r0] = heights[r0].max(h);
                        if e.cell.y_expand {
                            row_expand[r0] = true;
                        }
                    }
                } else {
                    if cs > 1 {
                        let cur: f64 = widths[c0..c0 + cs].iter().sum::<f64>() + t.column_spacing * (cs - 1) as f64;
                        if w > cur {
                            let add = (w - cur) / cs as f64;
                            for x in &mut widths[c0..c0 + cs] {
                                *x += add;
                            }
                        }
                    }
                    if rs > 1 {
                        let cur: f64 = heights[r0..r0 + rs].iter().sum::<f64>() + t.row_spacing * (rs - 1) as f64;
                        if h > cur {
                            let add = (h - cur) / rs as f64;
                            for x in &mut heights[r0..r0 + rs] {
                                *x += add;
                            }
                        }
                    }
                }
            }
        }
        if t.homogeneous_columns {
            let m = widths.iter().cloned().fold(0.0, f64::max);
            widths.iter_mut().for_each(|w| *w = m);
        }
        if t.homogeneous_rows {
            let m = heights.iter().cloned().fold(0.0, f64::max);
            heights.iter_mut().for_each(|h| *h = m);
        }
        let total = |v: &[f64], spacing: f64, grid: f64| -> f64 {
            v.iter().sum::<f64>() + spacing * (v.len().saturating_sub(1)) as f64 + grid * (v.len() + 1) as f64
        };
        if let Some(tw) = t.width {
            let cur = total(&widths, t.column_spacing, t.vert_grid);
            if tw > cur && n_cols > 0 {
                let n_exp = col_expand.iter().filter(|e| **e).count();
                let extra = tw - cur;
                if n_exp > 0 {
                    for (i, w) in widths.iter_mut().enumerate() {
                        if col_expand[i] {
                            *w += extra / n_exp as f64;
                        }
                    }
                } else {
                    widths.iter_mut().for_each(|w| *w += extra / n_cols as f64);
                }
            }
        }
        if let Some(th) = t.height {
            let cur = total(&heights, t.row_spacing, t.horz_grid);
            if th > cur && n_rows > 0 {
                let n_exp = row_expand.iter().filter(|e| **e).count();
                let extra = th - cur;
                if n_exp > 0 {
                    for (i, h) in heights.iter_mut().enumerate() {
                        if row_expand[i] {
                            *h += extra / n_exp as f64;
                        }
                    }
                } else {
                    heights.iter_mut().for_each(|h| *h += extra / n_rows as f64);
                }
            }
        }
        let mut cols = Vec::with_capacity(n_cols);
        let mut x = t.vert_grid;
        for w in &widths {
            cols.push((x, *w));
            x += w + t.column_spacing + t.vert_grid;
        }
        let mut rows = Vec::with_capacity(n_rows);
        let mut y = t.horz_grid;
        for h in &heights {
            rows.push((y, *h));
            y += h + t.row_spacing + t.horz_grid;
        }
        let size_w = total(&widths, t.column_spacing, t.vert_grid).max(t.width.unwrap_or(0.0));
        let size_h = total(&heights, t.row_spacing, t.horz_grid).max(t.height.unwrap_or(0.0));

        for e in &entries {
            let (c0, cs) = (e.cell.column as usize, e.cell.columns.max(1) as usize);
            let (r0, rs) = (e.cell.row as usize, e.cell.rows.max(1) as usize);
            let cx = cols[c0].0;
            let cw = cols[c0 + cs - 1].0 + cols[c0 + cs - 1].1 - cx;
            let ry = rows[r0].0;
            let rh = rows[r0 + rs - 1].0 + rows[r0 + rs - 1].1 - ry;
            let (bw, bh) = if e.b.is_empty() { (0.0, 0.0) } else { (e.b.width(), e.b.height()) };
            let (bx1, by1) = if e.b.is_empty() { (0.0, 0.0) } else { (e.b.x1, e.b.y1) };
            let free_w = (cw - e.cell.left_padding - e.cell.right_padding - bw).max(0.0);
            let free_h = (rh - e.cell.top_padding - e.cell.bottom_padding - bh).max(0.0);
            let dx = t.x + cx + e.cell.left_padding + free_w * e.cell.x_align - bx1;
            let dy = t.y + ry + e.cell.top_padding + free_h * e.cell.y_align - by1;
            if let Some(n) = self.node_mut(e.id) {
                n.table_offset = (dx, dy);
            }
        }
        if let Some(Node { shape: Shape::Table(tt), .. }) = self.node_mut(id) {
            tt.rows = rows;
            tt.cols = cols;
            tt.size = (size_w, size_h);
        }
    }

    fn ensure_layout(&mut self) {
        if self.dirty {
            self.dirty = false;
            let root = self.root;
            self.layout(root);
        }
    }
}

/// Scène partagée (cloner une `Canvas` donne un nouvel accès à la même scène).
#[derive(Clone)]
pub struct Canvas {
    scene: Rc<RefCell<Scene>>,
}

impl Default for Canvas {
    fn default() -> Self {
        Canvas::new()
    }
}

impl Canvas {
    /// `goo_canvas_new` : scène vide (résolution 72 ppp).
    pub fn new() -> Canvas {
        let root = Node::new(None, Shape::Group);
        Canvas {
            scene: Rc::new(RefCell::new(Scene {
                nodes: vec![Some(root)],
                root: 0,
                dirty: true,
                resolution: 72.0,
                listeners: Vec::new(),
                widgets_changed: false,
            })),
        }
    }

    pub fn ptr_eq(&self, other: &Canvas) -> bool {
        Rc::ptr_eq(&self.scene, &other.scene)
    }

    /// `goo_canvas_get_root_item`.
    pub fn root(&self) -> ItemId {
        self.scene.borrow().root
    }

    /// Résolution en ppp (`resolution-x`).
    pub fn resolution(&self) -> f64 {
        self.scene.borrow().resolution
    }

    pub fn set_resolution(&self, dpi: f64) {
        self.scene.borrow_mut().resolution = dpi;
    }

    /// Abonnement aux modifications de la scène (vues).
    pub fn connect_changed(&self, f: impl Fn() + 'static) {
        self.scene.borrow_mut().listeners.push(Rc::new(f));
    }

    fn changed(&self) {
        let listeners = {
            let mut s = self.scene.borrow_mut();
            s.dirty = true;
            s.listeners.clone()
        };
        for l in listeners {
            l();
        }
    }

    fn add(&self, parent: ItemId, shape: Shape) -> ItemId {
        let id = {
            let mut s = self.scene.borrow_mut();
            let is_widget = matches!(shape, Shape::Widget { .. });
            let id = s.nodes.len();
            s.nodes.push(Some(Node::new(Some(parent), shape)));
            if let Some(p) = s.node_mut(parent) {
                p.children.push(id);
            }
            if is_widget {
                s.widgets_changed = true;
            }
            id
        };
        self.changed();
        id
    }

    /// Vrai si l'élément existe encore.
    pub fn exists(&self, id: ItemId) -> bool {
        self.scene.borrow().node(id).is_some()
    }

    // --- Création -------------------------------------------------------

    /// `goo_canvas_group_new`.
    pub fn group(&self, parent: ItemId) -> ItemId {
        self.add(parent, Shape::Group)
    }

    /// `goo_canvas_rect_new`.
    pub fn rect(&self, parent: ItemId, x: f64, y: f64, w: f64, h: f64) -> ItemId {
        self.add(parent, Shape::Rect { x, y, w, h, rx: 0.0, ry: 0.0 })
    }

    /// `goo_canvas_text_new` (largeur -1 : pas de retour à la ligne).
    pub fn text(&self, parent: ItemId, text: &str, x: f64, y: f64, width: f64, anchor: Anchor) -> ItemId {
        self.add(
            parent,
            Shape::Text(TextShape {
                text: text.to_string(),
                x,
                y,
                width,
                height: -1.0,
                anchor,
                font: format!("{BP_FONT}14px"),
                markup: false,
                alignment: pango::Alignment::Left,
                wrap: pango::WrapMode::Word,
                ellipsize: pango::EllipsizeMode::None,
            }),
        )
    }

    /// Texte avec balisage Pango.
    pub fn markup(&self, parent: ItemId, markup: &str, x: f64, y: f64, width: f64, anchor: Anchor) -> ItemId {
        let id = self.text(parent, markup, x, y, width, anchor);
        self.set(id, [Prop::UseMarkup(true)]);
        id
    }

    /// `goo_canvas_image_new`.
    pub fn image(&self, parent: ItemId, pixbuf: Option<&Pixbuf>, x: f64, y: f64) -> ItemId {
        self.add(parent, Shape::Image { pixbuf: pixbuf.cloned(), x, y, width: None, height: None, scale_to_fit: false })
    }

    /// `goo_canvas_polyline_new`.
    pub fn polyline(&self, parent: ItemId, closed: bool, points: &[(f64, f64)]) -> ItemId {
        self.add(parent, Shape::Polyline { points: points.to_vec(), closed })
    }

    /// `goo_canvas_polyline_new_line`.
    pub fn line(&self, parent: ItemId, x1: f64, y1: f64, x2: f64, y2: f64) -> ItemId {
        self.polyline(parent, false, &[(x1, y1), (x2, y2)])
    }

    /// `goo_canvas_table_new`.
    pub fn table(&self, parent: ItemId) -> ItemId {
        self.add(
            parent,
            Shape::Table(TableShape {
                x: 0.0,
                y: 0.0,
                width: None,
                height: None,
                row_spacing: 0.0,
                column_spacing: 0.0,
                horz_grid: 0.0,
                vert_grid: 0.0,
                homogeneous_rows: false,
                homogeneous_columns: false,
                rows: Vec::new(),
                cols: Vec::new(),
                size: (0.0, 0.0),
            }),
        )
    }

    /// `goo_canvas_widget_new` : widget GTK placé sur le canevas (affiché
    /// par la vue, ignoré à l'impression).
    pub fn widget(&self, parent: ItemId, widget: &impl gtk::prelude::IsA<gtk::Widget>, x: f64, y: f64, w: f64, h: f64, anchor: Anchor) -> ItemId {
        self.add(parent, Shape::Widget { widget: widget.as_ref().clone(), x, y, w, h, anchor })
    }

    // --- Propriétés -----------------------------------------------------

    /// `g_object_set` sur un élément.
    pub fn set(&self, id: ItemId, props: impl IntoIterator<Item = Prop>) {
        {
            let mut s = self.scene.borrow_mut();
            let Some(n) = s.node_mut(id) else { return };
            for p in props {
                apply_prop(n, p);
            }
            if matches!(n.shape, Shape::Widget { .. }) {
                s.widgets_changed = true;
            }
        }
        self.changed();
    }

    /// Texte d'un élément texte.
    pub fn text_of(&self, id: ItemId) -> Option<String> {
        match &self.scene.borrow().node(id)?.shape {
            Shape::Text(t) => Some(t.text.clone()),
            _ => None,
        }
    }

    pub fn tooltip_of(&self, id: ItemId) -> Option<String> {
        self.scene.borrow().node(id)?.tooltip.clone()
    }

    /// Visibilité (`visibility`).
    pub fn is_visible(&self, id: ItemId) -> bool {
        self.scene.borrow().node(id).map(|n| n.visible).unwrap_or(false)
    }

    /// Placement dans un tableau (propriétés « enfant »).
    pub fn set_cell(&self, id: ItemId, f: impl FnOnce(&mut CellProps)) {
        {
            let mut s = self.scene.borrow_mut();
            let Some(n) = s.node_mut(id) else { return };
            let mut c = n.cell.unwrap_or_default();
            f(&mut c);
            n.cell = Some(c);
        }
        self.changed();
    }

    pub fn cell(&self, id: ItemId) -> Option<CellProps> {
        self.scene.borrow().node(id)?.cell
    }

    /// `Canvas::SetTableItemAttribute` par nom de propriété GooCanvas.
    pub fn set_table_item_attribute(&self, id: ItemId, attribute: &str, value: f64) {
        self.set_cell(id, |c| match attribute {
            "row" => c.row = value as u32,
            "column" => c.column = value as u32,
            "rows" => c.rows = value as u32,
            "columns" => c.columns = value as u32,
            "x-align" => c.x_align = value,
            "y-align" => c.y_align = value,
            "x-expand" => c.x_expand = value != 0.0,
            "x-fill" => c.x_fill = value != 0.0,
            "x-shrink" => c.x_shrink = value != 0.0,
            "y-expand" => c.y_expand = value != 0.0,
            "y-fill" => c.y_fill = value != 0.0,
            "y-shrink" => c.y_shrink = value != 0.0,
            "left-padding" => c.left_padding = value,
            "right-padding" => c.right_padding = value,
            "top-padding" => c.top_padding = value,
            "bottom-padding" => c.bottom_padding = value,
            other => log::warn!("propriété de tableau inconnue : {other}"),
        });
    }

    /// Données associées à un élément (`g_object_set_data`).
    pub fn set_data(&self, id: ItemId, key: &str, value: Rc<dyn Any>) {
        if let Some(n) = self.scene.borrow_mut().node_mut(id) {
            n.data.insert(key.to_string(), value);
        }
    }

    pub fn data(&self, id: ItemId, key: &str) -> Option<Rc<dyn Any>> {
        self.scene.borrow().node(id)?.data.get(key).cloned()
    }

    // --- Arbre ----------------------------------------------------------

    pub fn parent(&self, id: ItemId) -> Option<ItemId> {
        self.scene.borrow().node(id)?.parent
    }

    pub fn children(&self, id: ItemId) -> Vec<ItemId> {
        self.scene.borrow().node(id).map(|n| n.children.clone()).unwrap_or_default()
    }

    pub fn n_children(&self, id: ItemId) -> usize {
        self.scene.borrow().node(id).map(|n| n.children.len()).unwrap_or(0)
    }

    /// `goo_canvas_item_remove` (avec ses descendants).
    pub fn remove(&self, id: ItemId) {
        {
            let mut s = self.scene.borrow_mut();
            if id == s.root {
                // Retirer la racine revient à la vider.
                let children = s.node(id).map(|n| n.children.clone()).unwrap_or_default();
                for c in children {
                    remove_rec(&mut s, c);
                }
                if let Some(r) = s.node_mut(id) {
                    r.children.clear();
                    r.transform = cairo::Matrix::identity();
                    r.handlers = Handlers::default();
                }
            } else {
                if let Some(p) = s.node(id).and_then(|n| n.parent) {
                    if let Some(pn) = s.node_mut(p) {
                        pn.children.retain(|c| *c != id);
                    }
                }
                remove_rec(&mut s, id);
            }
            s.widgets_changed = true;
        }
        self.changed();
    }

    /// Vide la scène (`CanvasModule::Wipe`).
    pub fn wipe(&self) {
        let root = self.root();
        self.remove(root);
    }

    /// `goo_canvas_item_lower (item, NULL)` : passe en arrière-plan.
    pub fn lower(&self, id: ItemId) {
        {
            let mut s = self.scene.borrow_mut();
            let Some(p) = s.node(id).and_then(|n| n.parent) else { return };
            if let Some(pn) = s.node_mut(p) {
                pn.children.retain(|c| *c != id);
                pn.children.insert(0, id);
            }
        }
        self.changed();
    }

    /// `goo_canvas_item_raise (item, NULL)` : passe au premier plan.
    pub fn raise(&self, id: ItemId) {
        {
            let mut s = self.scene.borrow_mut();
            let Some(p) = s.node(id).and_then(|n| n.parent) else { return };
            if let Some(pn) = s.node_mut(p) {
                pn.children.retain(|c| *c != id);
                pn.children.push(id);
            }
        }
        self.changed();
    }

    // --- Transformations ----------------------------------------------

    pub fn transform(&self, id: ItemId) -> cairo::Matrix {
        self.scene.borrow().node(id).map(|n| n.transform).unwrap_or_else(cairo::Matrix::identity)
    }

    /// `goo_canvas_item_set_transform`.
    pub fn set_transform(&self, id: ItemId, m: cairo::Matrix) {
        if let Some(n) = self.scene.borrow_mut().node_mut(id) {
            n.transform = m;
        }
        self.changed();
    }

    fn modify_transform(&self, id: ItemId, f: impl FnOnce(&mut cairo::Matrix)) {
        if let Some(n) = self.scene.borrow_mut().node_mut(id) {
            f(&mut n.transform);
        }
        self.changed();
    }

    /// `goo_canvas_item_translate`.
    pub fn translate(&self, id: ItemId, tx: f64, ty: f64) {
        self.modify_transform(id, |m| m.translate(tx, ty));
    }

    /// `goo_canvas_item_scale`.
    pub fn scale(&self, id: ItemId, sx: f64, sy: f64) {
        self.modify_transform(id, |m| m.scale(sx, sy));
    }

    /// `goo_canvas_item_rotate` (degrés, autour de `cx, cy`).
    pub fn rotate(&self, id: ItemId, degrees: f64, cx: f64, cy: f64) {
        self.modify_transform(id, |m| {
            m.translate(cx, cy);
            m.rotate(degrees.to_radians());
            m.translate(-cx, -cy);
        });
    }

    /// `goo_canvas_item_set_simple_transform`.
    pub fn set_simple_transform(&self, id: ItemId, x: f64, y: f64, scale: f64, rotation: f64) {
        let mut m = cairo::Matrix::identity();
        m.translate(x, y);
        m.scale(scale, scale);
        m.rotate(rotation.to_radians());
        self.set_transform(id, m);
    }

    // --- Géométrie ------------------------------------------------------

    /// `goo_canvas_item_get_bounds` : boîte en coordonnées du canevas.
    pub fn bounds(&self, id: ItemId) -> Bounds {
        let mut s = self.scene.borrow_mut();
        s.ensure_layout();
        let local = s.local_bounds(id);
        let m = s.item_to_canvas(id);
        let b = local.transform(&m);
        if b.is_empty() {
            Bounds::default()
        } else {
            b
        }
    }

    /// `goo_canvas_convert_to_item_space` : point canevas -> espace élément.
    pub fn convert_to_item_space(&self, id: ItemId, x: f64, y: f64) -> (f64, f64) {
        let mut s = self.scene.borrow_mut();
        s.ensure_layout();
        let m = s.item_to_canvas(id);
        match m.try_invert() {
            Ok(inv) => inv.transform_point(x, y),
            Err(_) => (x, y),
        }
    }

    /// Vecteur canevas -> espace élément (sans translation), comme le fait le
    /// C++ pour convertir des distances.
    pub fn convert_distance_to_item_space(&self, id: ItemId, dx: f64, dy: f64) -> (f64, f64) {
        let mut s = self.scene.borrow_mut();
        s.ensure_layout();
        let m = s.item_to_canvas(id);
        match m.try_invert() {
            Ok(inv) => inv.transform_distance(dx, dy),
            Err(_) => (dx, dy),
        }
    }

    /// `goo_canvas_convert_from_item_space` : point élément -> canevas.
    pub fn convert_from_item_space(&self, id: ItemId, x: f64, y: f64) -> (f64, f64) {
        let mut s = self.scene.borrow_mut();
        s.ensure_layout();
        s.item_to_canvas(id).transform_point(x, y)
    }

    /// Boîte de toute la scène.
    pub fn scene_bounds(&self) -> Bounds {
        let root = self.root();
        self.bounds(root)
    }

    // --- Événements -----------------------------------------------------

    /// `button_press_event` sur un élément. Le gestionnaire reçoit la scène,
    /// l'élément porteur du gestionnaire, la cible réelle et l'événement.
    pub fn connect_button_press(&self, id: ItemId, f: impl Fn(&Canvas, ItemId, ItemId, &CanvasEvent) -> bool + 'static) {
        if let Some(n) = self.scene.borrow_mut().node_mut(id) {
            n.handlers.press.push(Rc::new(f));
        }
    }

    pub fn connect_button_release(&self, id: ItemId, f: impl Fn(&Canvas, ItemId, ItemId, &CanvasEvent) -> bool + 'static) {
        if let Some(n) = self.scene.borrow_mut().node_mut(id) {
            n.handlers.release.push(Rc::new(f));
        }
    }

    pub fn connect_motion(&self, id: ItemId, f: impl Fn(&Canvas, ItemId, ItemId, &CanvasEvent) -> bool + 'static) {
        if let Some(n) = self.scene.borrow_mut().node_mut(id) {
            n.handlers.motion.push(Rc::new(f));
        }
    }

    pub fn connect_enter(&self, id: ItemId, f: impl Fn(&Canvas, ItemId, ItemId, &CanvasEvent) -> bool + 'static) {
        if let Some(n) = self.scene.borrow_mut().node_mut(id) {
            n.handlers.enter.push(Rc::new(f));
        }
    }

    pub fn connect_leave(&self, id: ItemId, f: impl Fn(&Canvas, ItemId, ItemId, &CanvasEvent) -> bool + 'static) {
        if let Some(n) = self.scene.borrow_mut().node_mut(id) {
            n.handlers.leave.push(Rc::new(f));
        }
    }

    /// Élément le plus haut sous le point (coordonnées canevas).
    pub fn item_at(&self, x: f64, y: f64) -> Option<ItemId> {
        let mut s = self.scene.borrow_mut();
        s.ensure_layout();
        let root = s.root;
        hit(&s, root, cairo::Matrix::identity(), x, y)
    }

    /// Diffuse un événement : de la cible vers ses ancêtres, jusqu'à ce
    /// qu'un gestionnaire le traite. Renvoie vrai si traité.
    pub fn dispatch(&self, target: ItemId, kind: EventKind, ev: &CanvasEvent) -> bool {
        let mut cur = Some(target);
        while let Some(id) = cur {
            let (handlers, parent, m) = {
                let mut s = self.scene.borrow_mut();
                s.ensure_layout();
                let Some(n) = s.node(id) else { return false };
                let h = match kind {
                    EventKind::Press => n.handlers.press.clone(),
                    EventKind::Release => n.handlers.release.clone(),
                    EventKind::Motion => n.handlers.motion.clone(),
                    EventKind::Enter => n.handlers.enter.clone(),
                    EventKind::Leave => n.handlers.leave.clone(),
                };
                (h, n.parent, s.item_to_canvas(id))
            };
            let (ix, iy) = m.try_invert().map(|i| i.transform_point(ev.x, ev.y)).unwrap_or((ev.x, ev.y));
            let local = CanvasEvent { item_x: ix, item_y: iy, ..*ev };
            for h in handlers {
                if h(self, id, target, &local) {
                    return true;
                }
            }
            if matches!(kind, EventKind::Enter | EventKind::Leave) {
                return false;
            }
            cur = parent;
        }
        false
    }

    // --- Widgets --------------------------------------------------------

    /// Widgets posés sur le canevas avec leur boîte (coordonnées canevas).
    pub fn widget_items(&self) -> Vec<(ItemId, gtk::Widget, Bounds, bool)> {
        let ids: Vec<ItemId> = {
            let s = self.scene.borrow();
            s.nodes
                .iter()
                .enumerate()
                .filter_map(|(i, n)| n.as_ref().filter(|n| matches!(n.shape, Shape::Widget { .. })).map(|_| i))
                .collect()
        };
        let mut v = Vec::new();
        for id in ids {
            let b = self.bounds(id);
            let s = self.scene.borrow();
            let Some(n) = s.node(id) else { continue };
            if let Shape::Widget { widget, .. } = &n.shape {
                let mut visible = n.visible;
                let mut cur = n.parent;
                while let Some(p) = cur {
                    let Some(pn) = s.node(p) else { break };
                    visible &= pn.visible;
                    cur = pn.parent;
                }
                v.push((id, widget.clone(), b, visible));
            }
        }
        v
    }

    // --- Rendu ----------------------------------------------------------

    /// `goo_canvas_render` : dessine la scène dans `cr`. `bounds` limite le
    /// dessin aux éléments qui la croisent (coordonnées canevas).
    pub fn render(&self, cr: &cairo::Context, bounds: Option<&Bounds>, scale: f64) {
        let mut s = self.scene.borrow_mut();
        s.ensure_layout();
        let root = s.root;
        let _ = cr.save();
        if scale != 1.0 {
            cr.scale(scale, scale);
        }
        render_rec(&s, root, cr, bounds);
        let _ = cr.restore();
    }

    /// Dessin d'un seul élément (et de ses descendants), avec la matrice de
    /// ses ancêtres.
    pub fn render_item(&self, id: ItemId, cr: &cairo::Context) {
        let mut s = self.scene.borrow_mut();
        s.ensure_layout();
        let parent_m = s.node(id).and_then(|n| n.parent).map(|p| s.item_to_canvas(p)).unwrap_or_else(cairo::Matrix::identity);
        let _ = cr.save();
        cr.transform(parent_m);
        render_rec(&s, id, cr, None);
        let _ = cr.restore();
    }

    // --- Aides de `Canvas::` (canvas.cpp) ------------------------------

    /// `Canvas::PutInTable`.
    pub fn put_in_table(&self, table: ItemId, item: ItemId, row: u32, column: u32) {
        let _ = table;
        self.set_cell(item, |c| {
            c.row = row;
            c.column = column;
            c.x_align = 0.0;
            c.y_align = 0.0;
        });
    }

    /// `Canvas::PutTextInTable`.
    pub fn put_text_in_table(&self, table: ItemId, text: &str, row: u32, column: u32) -> ItemId {
        let item = self.text(table, text, 0.0, 0.0, -1.0, Anchor::NW);
        self.set(item, [Prop::Font(format!("{BP_FONT}14px")), Prop::Ellipsize(pango::EllipsizeMode::None)]);
        self.put_in_table(table, item, row, column);
        self.set_cell(item, |c| {
            c.x_expand = true;
            c.x_fill = true;
            c.x_shrink = true;
        });
        item
    }

    /// `Canvas::CreateIcon` : icône du thème (taille « bouton »).
    pub fn create_icon(&self, parent: ItemId, icon_name: &str, x: f64, y: f64) -> ItemId {
        let pixbuf = crate::icons::icon_pixbuf(icon_name, 16);
        self.image(parent, pixbuf.as_ref(), x, y)
    }

    /// `Canvas::PutStockIconInTable`.
    pub fn put_stock_icon_in_table(&self, table: ItemId, icon_name: &str, row: u32, column: u32) -> ItemId {
        let item = self.create_icon(table, icon_name, 0.0, 0.0);
        self.put_in_table(table, item, row, column);
        item
    }

    /// `Canvas::PutIconInTable` : image lue dans un fichier.
    pub fn put_icon_in_table(&self, table: ItemId, file: &std::path::Path, row: u32, column: u32) -> ItemId {
        let pixbuf = Pixbuf::from_file(file).ok();
        let item = self.image(table, pixbuf.as_ref(), 0.0, 0.0);
        self.put_in_table(table, item, row, column);
        item
    }

    /// `Canvas::PutPixbufInTable` (image vide de 1×1 si `None`).
    pub fn put_pixbuf_in_table(&self, table: ItemId, pixbuf: Option<&Pixbuf>, row: u32, column: u32) -> ItemId {
        let empty;
        let pixbuf = match pixbuf {
            Some(p) => p,
            None => {
                empty = Pixbuf::new(gtk::gdk_pixbuf::Colorspace::Rgb, true, 8, 1, 1).expect("pixbuf 1x1");
                empty.fill(0);
                &empty
            }
        };
        let item = self.image(table, Some(pixbuf), 0.0, 0.0);
        self.put_in_table(table, item, row, column);
        item
    }

    /// `Canvas::Anchor` : place `item` sous et/ou à droite d'autres éléments.
    pub fn anchor(&self, item: ItemId, to_bottom_of: Option<ItemId>, to_right_of: Option<ItemId>, space: f64) {
        let ib = self.bounds(item);
        if let Some(r) = to_right_of {
            let tb = self.bounds(r);
            let dx = tb.x2 - ib.x1 + space;
            let (dx, _) = self.convert_distance_to_item_space(item, dx, 0.0);
            self.translate(item, dx, 0.0);
        }
        if let Some(b) = to_bottom_of {
            let tb = self.bounds(b);
            let dy = tb.y2 - ib.y1 + space;
            let (_, dy) = self.convert_distance_to_item_space(item, 0.0, dy);
            self.translate(item, 0.0, dy);
        }
    }

    /// `Canvas::VAlign` : alignement horizontal sur un autre élément
    /// (le nom vient du C++ : alignement de « colonnes »).
    pub fn v_align(&self, item: ItemId, item_alignment: Alignment, with: ItemId, with_alignment: Alignment, offset: f64) {
        let ib = self.bounds(item);
        let wb = self.bounds(with);
        let dx = delta_align(ib.x1, ib.x2, item_alignment, wb.x1, wb.x2, with_alignment) + offset;
        let (dx, _) = self.convert_distance_to_item_space(item, dx, 0.0);
        self.translate(item, dx, 0.0);
    }

    /// `Canvas::HAlign` : alignement vertical sur un autre élément.
    pub fn h_align(&self, item: ItemId, item_alignment: Alignment, with: ItemId, with_alignment: Alignment, offset: f64) {
        let ib = self.bounds(item);
        let wb = self.bounds(with);
        let dy = delta_align(ib.y1, ib.y2, item_alignment, wb.y1, wb.y2, with_alignment) + offset;
        let (_, dy) = self.convert_distance_to_item_space(item, 0.0, dy);
        self.translate(item, 0.0, dy);
    }

    /// `Canvas::CreatePrinterCanvas` : scène dont l'unité est 1 % de la
    /// largeur du papier.
    pub fn new_printer(ctx: &crate::print::PageContext) -> Canvas {
        let c = Canvas::new();
        c.set_resolution(ctx.dpi_x);
        let mut m = cairo::Matrix::identity();
        m.scale(ctx.width / 100.0, ctx.width / 100.0);
        c.set_transform(c.root(), m);
        c
    }

    /// `Canvas::GetScaleToFit`.
    pub fn scale_to_fit(&self, item: ItemId, ctx: &crate::print::PageContext) -> f64 {
        let b = self.bounds(item);
        let canvas_w = b.width();
        let scale = ctx.dpi_x / self.resolution();
        if canvas_w * scale > ctx.width {
            scale * ctx.width / canvas_w
        } else {
            1.0
        }
    }

    /// `Canvas::FitToContext`.
    pub fn fit_to_context(&self, item: ItemId, ctx: &crate::print::PageContext) {
        let scale = self.scale_to_fit(item, ctx);
        if scale != 1.0 {
            let mut m = self.transform(item);
            let b = self.bounds(item);
            let (_, y1) = self.convert_to_item_space(item, b.x1, b.y1);
            m.scale(scale, scale);
            m.translate(0.0, y1 / scale - y1);
            self.set_transform(item, m);
        }
    }

    /// `Canvas::NormalyzeDecimalNotation` : virgule décimale -> point.
    pub fn normalize_decimal_notation(s: &str) -> String {
        s.replacen(',', ".", 1)
    }
}

/// Type d'événement pointeur.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    Press,
    Release,
    Motion,
    Enter,
    Leave,
}

fn delta_align(item_1: f64, item_2: f64, item_alignment: Alignment, with_1: f64, with_2: f64, with_alignment: Alignment) -> f64 {
    // Reproduit le calcul C++ (y compris son « milieu » relatif).
    let with_anchor = match with_alignment {
        Alignment::Start => with_1,
        Alignment::Middle => (with_2 - with_1) / 2.0,
        Alignment::End => with_2,
    };
    match item_alignment {
        Alignment::Start => with_anchor - item_1,
        Alignment::Middle => with_anchor - (item_2 - item_1) / 2.0,
        Alignment::End => with_anchor - item_2,
    }
}

fn remove_rec(s: &mut Scene, id: ItemId) {
    let children = s.node(id).map(|n| n.children.clone()).unwrap_or_default();
    for c in children {
        remove_rec(s, c);
    }
    if let Some(Some(n)) = s.nodes.get(id) {
        if let Shape::Widget { widget, .. } = &n.shape {
            crate::glade::detach(widget);
        }
    }
    if let Some(slot) = s.nodes.get_mut(id) {
        *slot = None;
    }
}

fn apply_prop(n: &mut Node, p: Prop) {
    match p {
        Prop::FillColor(c) => n.fill = Some(Color::parse(&c)),
        Prop::FillColorRgba(c) => n.fill = Some(Color::from_rgba(c)),
        Prop::NoFill => n.fill = None,
        Prop::StrokeColor(c) => n.stroke = Some(Color::parse(&c)),
        Prop::StrokeColorRgba(c) => n.stroke = Some(Color::from_rgba(c)),
        Prop::NoStroke => n.stroke = None,
        Prop::LineWidth(w) => n.line_width = w,
        Prop::LineDash(d) => n.dash = if d.is_empty() { None } else { Some(d) },
        Prop::Visible(v) => n.visible = v,
        Prop::PointerEvents(v) => n.pointer_events = v,
        Prop::Tooltip(t) => n.tooltip = Some(t),
        p => match (&mut n.shape, p) {
            (Shape::Rect { x, .. }, Prop::X(v)) => *x = v,
            (Shape::Rect { y, .. }, Prop::Y(v)) => *y = v,
            (Shape::Rect { w, .. }, Prop::Width(v)) => *w = v,
            (Shape::Rect { h, .. }, Prop::Height(v)) => *h = v,
            (Shape::Rect { rx, .. }, Prop::RadiusX(v)) => *rx = v,
            (Shape::Rect { ry, .. }, Prop::RadiusY(v)) => *ry = v,
            (Shape::Text(t), Prop::X(v)) => t.x = v,
            (Shape::Text(t), Prop::Y(v)) => t.y = v,
            (Shape::Text(t), Prop::Width(v)) => t.width = v,
            (Shape::Text(t), Prop::Height(v)) => t.height = v,
            (Shape::Text(t), Prop::Font(v)) => t.font = v,
            (Shape::Text(t), Prop::Text(v)) => t.text = v,
            (Shape::Text(t), Prop::UseMarkup(v)) => t.markup = v,
            (Shape::Text(t), Prop::Alignment(v)) => t.alignment = v,
            (Shape::Text(t), Prop::Wrap(v)) => t.wrap = v,
            (Shape::Text(t), Prop::Ellipsize(v)) => t.ellipsize = v,
            (Shape::Text(t), Prop::Anchor(v)) => t.anchor = v,
            (Shape::Image { x, .. }, Prop::X(v)) => *x = v,
            (Shape::Image { y, .. }, Prop::Y(v)) => *y = v,
            (Shape::Image { width, .. }, Prop::Width(v)) => *width = Some(v),
            (Shape::Image { height, .. }, Prop::Height(v)) => *height = Some(v),
            (Shape::Image { scale_to_fit, .. }, Prop::ScaleToFit(v)) => *scale_to_fit = v,
            (Shape::Image { pixbuf, .. }, Prop::Pixbuf(v)) => *pixbuf = v,
            (Shape::Polyline { points, .. }, Prop::Points(v)) => *points = v,
            (Shape::Polyline { closed, .. }, Prop::ClosePath(v)) => *closed = v,
            (Shape::Table(t), Prop::X(v)) => t.x = v,
            (Shape::Table(t), Prop::Y(v)) => t.y = v,
            (Shape::Table(t), Prop::Width(v)) => t.width = Some(v),
            (Shape::Table(t), Prop::Height(v)) => t.height = Some(v),
            (Shape::Table(t), Prop::RowSpacing(v)) => t.row_spacing = v,
            (Shape::Table(t), Prop::ColumnSpacing(v)) => t.column_spacing = v,
            (Shape::Table(t), Prop::HorzGridLineWidth(v)) => t.horz_grid = v,
            (Shape::Table(t), Prop::VertGridLineWidth(v)) => t.vert_grid = v,
            (Shape::Table(t), Prop::HomogeneousRows(v)) => t.homogeneous_rows = v,
            (Shape::Table(t), Prop::HomogeneousColumns(v)) => t.homogeneous_columns = v,
            (Shape::Widget { x, .. }, Prop::X(v)) => *x = v,
            (Shape::Widget { y, .. }, Prop::Y(v)) => *y = v,
            (Shape::Widget { w, .. }, Prop::Width(v)) => *w = v,
            (Shape::Widget { h, .. }, Prop::Height(v)) => *h = v,
            (Shape::Widget { anchor, .. }, Prop::Anchor(v)) => *anchor = v,
            _ => {}
        },
    }
}

fn hit(s: &Scene, id: ItemId, parent_m: cairo::Matrix, x: f64, y: f64) -> Option<ItemId> {
    let n = s.node(id)?;
    if !n.visible {
        return None;
    }
    let m = cairo::Matrix::multiply(&n.matrix(), &parent_m);
    for c in n.children.iter().rev() {
        if let Some(h) = hit(s, *c, m, x, y) {
            return Some(h);
        }
    }
    if !n.pointer_events {
        return None;
    }
    match n.shape {
        Shape::Group | Shape::Widget { .. } => None,
        Shape::Table(_) => {
            let (lx, ly) = m.try_invert().ok()?.transform_point(x, y);
            let b = s.local_bounds(id);
            b.contains(lx, ly).then_some(id)
        }
        _ => {
            let (lx, ly) = m.try_invert().ok()?.transform_point(x, y);
            let b = s.local_bounds(id);
            b.contains(lx, ly).then_some(id)
        }
    }
}

fn set_color(cr: &cairo::Context, c: &Color) {
    cr.set_source_rgba(c.r, c.g, c.b, c.a);
}

fn stroke_setup(cr: &cairo::Context, n: &Node) {
    cr.set_line_width(n.line_width);
    match &n.dash {
        Some(d) => cr.set_dash(d, 0.0),
        None => cr.set_dash(&[], 0.0),
    }
}

fn render_rec(s: &Scene, id: ItemId, cr: &cairo::Context, clip: Option<&Bounds>) {
    let Some(n) = s.node(id) else { return };
    if !n.visible {
        return;
    }
    let _ = cr.save();
    cr.transform(n.matrix());
    match &n.shape {
        Shape::Group | Shape::Widget { .. } => {}
        Shape::Rect { x, y, w, h, rx, ry } => {
            if *rx > 0.0 || *ry > 0.0 {
                let r = rx.max(*ry).min(w / 2.0).min(h / 2.0);
                cr.new_sub_path();
                cr.arc(x + w - r, y + r, r, -std::f64::consts::FRAC_PI_2, 0.0);
                cr.arc(x + w - r, y + h - r, r, 0.0, std::f64::consts::FRAC_PI_2);
                cr.arc(x + r, y + h - r, r, std::f64::consts::FRAC_PI_2, std::f64::consts::PI);
                cr.arc(x + r, y + r, r, std::f64::consts::PI, 1.5 * std::f64::consts::PI);
                cr.close_path();
            } else {
                cr.rectangle(*x, *y, *w, *h);
            }
            if let Some(f) = &n.fill {
                set_color(cr, f);
                let _ = cr.fill_preserve();
            }
            if let Some(st) = &n.stroke {
                set_color(cr, st);
                stroke_setup(cr, n);
                let _ = cr.stroke_preserve();
            }
            cr.new_path();
        }
        Shape::Text(t) => {
            let (layout, ox, oy, _) = text_geometry(t);
            if let Some(f) = &n.fill {
                set_color(cr, f);
            }
            cr.move_to(ox, oy);
            pangocairo::functions::show_layout(cr, &layout);
            cr.new_path();
        }
        Shape::Image { pixbuf, x, y, width, height, scale_to_fit } => {
            if let Some(p) = pixbuf {
                let (pw, ph) = (p.width() as f64, p.height() as f64);
                let (w, h) = image_size(pixbuf, *width, *height);
                let _ = cr.save();
                cr.translate(*x, *y);
                if *scale_to_fit && pw > 0.0 && ph > 0.0 {
                    cr.scale(w / pw, h / ph);
                    cr.rectangle(0.0, 0.0, pw, ph);
                } else {
                    cr.rectangle(0.0, 0.0, w, h);
                }
                cr.clip();
                use gtk::gdk::prelude::GdkCairoContextExt;
                cr.set_source_pixbuf(p, 0.0, 0.0);
                let _ = cr.paint();
                let _ = cr.restore();
            }
        }
        Shape::Polyline { points, closed } => {
            if let Some((first, rest)) = points.split_first() {
                cr.move_to(first.0, first.1);
                for p in rest {
                    cr.line_to(p.0, p.1);
                }
                if *closed {
                    cr.close_path();
                    if let Some(f) = &n.fill {
                        set_color(cr, f);
                        let _ = cr.fill_preserve();
                    }
                }
                if let Some(st) = &n.stroke {
                    set_color(cr, st);
                    stroke_setup(cr, n);
                    let _ = cr.stroke_preserve();
                }
                cr.new_path();
            }
        }
        Shape::Table(t) => {
            if let Some(f) = &n.fill {
                set_color(cr, f);
                cr.rectangle(t.x, t.y, t.size.0, t.size.1);
                let _ = cr.fill();
            }
            if let Some(st) = &n.stroke {
                set_color(cr, st);
                if t.horz_grid > 0.0 {
                    cr.set_line_width(t.horz_grid);
                    let mut ys: Vec<f64> = t.rows.iter().map(|(y, _)| y - t.horz_grid / 2.0).collect();
                    ys.push(t.size.1 - t.horz_grid / 2.0);
                    for y in ys {
                        cr.move_to(t.x, t.y + y);
                        cr.line_to(t.x + t.size.0, t.y + y);
                    }
                    let _ = cr.stroke();
                }
                if t.vert_grid > 0.0 {
                    cr.set_line_width(t.vert_grid);
                    let mut xs: Vec<f64> = t.cols.iter().map(|(x, _)| x - t.vert_grid / 2.0).collect();
                    xs.push(t.size.0 - t.vert_grid / 2.0);
                    for x in xs {
                        cr.move_to(t.x + x, t.y);
                        cr.line_to(t.x + x, t.y + t.size.1);
                    }
                    let _ = cr.stroke();
                }
            }
        }
    }
    for c in &n.children {
        if let Some(clip) = clip {
            let b = s.child_bounds(*c);
            let _ = b;
            let _ = clip;
        }
        render_rec(s, *c, cr, clip);
    }
    let _ = cr.restore();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_bounds_and_translate() {
        let c = Canvas::new();
        let r = c.rect(c.root(), 10.0, 20.0, 30.0, 40.0);
        c.set(r, [Prop::LineWidth(2.0)]);
        assert_eq!(c.bounds(r), Bounds::new(9.0, 19.0, 41.0, 61.0));
        c.translate(r, 5.0, 0.0);
        assert_eq!(c.bounds(r).x1, 14.0);
        assert_eq!(c.item_at(20.0, 30.0), Some(r));
        assert_eq!(c.item_at(0.0, 0.0), None);
    }

    #[test]
    fn table_layout() {
        let c = Canvas::new();
        let t = c.table(c.root());
        c.set(t, [Prop::ColumnSpacing(10.0)]);
        let a = c.rect(t, 0.0, 0.0, 20.0, 10.0);
        let b = c.rect(t, 0.0, 0.0, 30.0, 15.0);
        for r in [a, b] {
            c.set(r, [Prop::NoStroke]);
        }
        c.put_in_table(t, a, 0, 0);
        c.put_in_table(t, b, 0, 1);
        assert_eq!(c.bounds(b).x1, 30.0);
        assert_eq!(c.bounds(t), Bounds::new(0.0, 0.0, 60.0, 15.0));
    }

    #[test]
    fn text_has_size() {
        let c = Canvas::new();
        let t = c.text(c.root(), "BellePoule", 0.0, 0.0, -1.0, Anchor::NW);
        let b = c.bounds(t);
        assert!(b.width() > 10.0 && b.height() > 5.0, "{b:?}");
        let centered = c.text(c.root(), "BellePoule", 100.0, 100.0, -1.0, Anchor::Center);
        let bc = c.bounds(centered);
        assert!((bc.x1 + bc.x2) / 2.0 - 100.0 < 1.0);
    }

    #[test]
    fn events_bubble() {
        let c = Canvas::new();
        let g = c.group(c.root());
        let r = c.rect(g, 0.0, 0.0, 10.0, 10.0);
        let hits = Rc::new(std::cell::Cell::new(0));
        let h = hits.clone();
        c.connect_button_press(g, move |_, item, target, _| {
            assert_ne!(item, target);
            h.set(h.get() + 1);
            true
        });
        let ev = CanvasEvent { x: 5.0, y: 5.0, item_x: 0.0, item_y: 0.0, widget_x: 5.0, widget_y: 5.0, button: 1, n_press: 1, state: gdk::ModifierType::empty() };
        let target = c.item_at(5.0, 5.0).unwrap();
        assert_eq!(target, r);
        assert!(c.dispatch(target, EventKind::Press, &ev));
        assert_eq!(hits.get(), 1);
    }

    #[test]
    fn render_to_image() {
        let c = Canvas::new();
        let r = c.rect(c.root(), 0.0, 0.0, 10.0, 10.0);
        c.set(r, [Prop::FillColor("red".into())]);
        let surf = cairo::ImageSurface::create(cairo::Format::ARgb32, 20, 20).unwrap();
        {
            let cr = cairo::Context::new(&surf).unwrap();
            c.render(&cr, None, 1.0);
        }
        let data = surf.take_data().unwrap();
        // Pixel (5,5) rouge opaque (BGRA en mémoire)
        let off = (5 * 20 + 5) * 4;
        assert_eq!(&data[off..off + 4], &[0, 0, 255, 255]);
    }
}
