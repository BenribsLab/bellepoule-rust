//! Liste de joueurs (port de `actors/players_list.cpp`, partie affichage).
//!
//! La liste ne connaît pas le modèle : l'écran lui fournit les colonnes à
//! afficher et, pour chaque joueur, le contenu de ses cellules (texte,
//! icône, case à cocher ou jauge) ainsi que les valeurs qui servent au tri.
//! Elle renvoie les actions de l'utilisateur (case cochée, double-clic,
//! sélection) par des fonctions de rappel.

use std::cell::RefCell;
use std::rc::Rc;

use bp_core::attribute::{Look, Value};
use bp_core::player::PlayerRef;
use gtk4 as gtk;
use gtk::gdk_pixbuf::Pixbuf;
use gtk::glib;
use gtk::prelude::*;

/// Contenu d'une cellule.
#[derive(Clone)]
pub enum Cell {
    Text(String),
    Icon(Option<Pixbuf>),
    Toggle(bool),
    Progress(i32),
}

/// Colonne affichée.
#[derive(Debug, Clone)]
pub struct ColumnSpec {
    /// Nom de code de l'attribut.
    pub code: String,
    /// En-tête (déjà traduit).
    pub title: String,
    pub look: Look,
    pub kind: CellKind,
    /// Une case à cocher peut-elle être modifiée par l'utilisateur ?
    pub editable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellKind {
    Text,
    Icon,
    Toggle,
    Progress,
}

/// Ligne : un joueur, ses cellules, ses valeurs de tri et ses sous-lignes
/// (les membres d'une équipe).
#[derive(Clone)]
pub struct RowData {
    pub player: PlayerRef,
    pub cells: Vec<Cell>,
    pub sort: Vec<Option<Value>>,
    pub children: Vec<RowData>,
    /// Ligne grisée (joueur absent).
    pub dimmed: bool,
}

type ToggleFn = dyn Fn(&[PlayerRef], &str, bool);
type ActivateFn = dyn Fn(PlayerRef);
type SelectionFn = dyn Fn();

struct Inner {
    tree: gtk::TreeView,
    scrolled: gtk::ScrolledWindow,
    store: RefCell<Option<gtk::TreeStore>>,
    columns: RefCell<Vec<ColumnSpec>>,
    rows: RefCell<Vec<RowData>>,
    /// Colonne de tri et sens (croissant si vrai).
    sort: RefCell<Option<(usize, bool)>>,
    on_toggle: RefCell<Option<Rc<ToggleFn>>>,
    on_activate: RefCell<Option<Rc<ActivateFn>>>,
    on_selection: RefCell<Option<Rc<SelectionFn>>>,
    /// Édition suspendue (phase verrouillée).
    locked: std::cell::Cell<bool>,
    toggles: RefCell<Vec<gtk::CellRendererToggle>>,
}

#[derive(Clone)]
pub struct PlayersListView {
    inner: Rc<Inner>,
}

impl PlayersListView {
    pub fn new() -> PlayersListView {
        let scrolled = gtk::ScrolledWindow::new();
        scrolled.set_hexpand(true);
        scrolled.set_vexpand(true);
        Self::in_scrolled(scrolled)
    }

    /// Liste placée dans une fenêtre défilante existante (celle d'une interface `.ui`).
    pub fn in_scrolled(scrolled: gtk::ScrolledWindow) -> PlayersListView {
        let tree = gtk::TreeView::new();
        tree.set_headers_visible(true);
        tree.set_enable_search(false);
        tree.selection().set_mode(gtk::SelectionMode::Multiple);
        tree.set_rubber_banding(true);
        scrolled.set_child(Some(&tree));
        let view = PlayersListView {
            inner: Rc::new(Inner {
                tree: tree.clone(),
                scrolled,
                store: RefCell::new(None),
                columns: RefCell::new(Vec::new()),
                rows: RefCell::new(Vec::new()),
                sort: RefCell::new(None),
                on_toggle: RefCell::new(None),
                on_activate: RefCell::new(None),
                on_selection: RefCell::new(None),
                locked: std::cell::Cell::new(false),
                toggles: RefCell::new(Vec::new()),
            }),
        };
        {
            let v = view.clone();
            tree.connect_row_activated(move |_, path, _| {
                if let Some(r) = v.player_at(path) {
                    let cb = v.inner.on_activate.borrow().clone();
                    if let Some(cb) = cb {
                        cb(r);
                    }
                }
            });
        }
        {
            let v = view.clone();
            tree.selection().connect_changed(move |_| {
                let cb = v.inner.on_selection.borrow().clone();
                if let Some(cb) = cb {
                    cb();
                }
            });
        }
        view
    }

    pub fn widget(&self) -> &gtk::ScrolledWindow {
        &self.inner.scrolled
    }

    pub fn connect_toggled(&self, f: impl Fn(&[PlayerRef], &str, bool) + 'static) {
        *self.inner.on_toggle.borrow_mut() = Some(Rc::new(f));
    }

    pub fn connect_activated(&self, f: impl Fn(PlayerRef) + 'static) {
        *self.inner.on_activate.borrow_mut() = Some(Rc::new(f));
    }

    pub fn connect_selection_changed(&self, f: impl Fn() + 'static) {
        *self.inner.on_selection.borrow_mut() = Some(Rc::new(f));
    }

    /// Gèle ou dégèle les cases à cocher.
    pub fn set_locked(&self, locked: bool) {
        self.inner.locked.set(locked);
    }

    fn player_at(&self, path: &gtk::TreePath) -> Option<PlayerRef> {
        let store = self.inner.store.borrow().clone()?;
        let iter = store.iter(path)?;
        Some(store.get::<u32>(&iter, 0))
    }

    /// Joueurs sélectionnés.
    pub fn selected(&self) -> Vec<PlayerRef> {
        let Some(store) = self.inner.store.borrow().clone() else { return Vec::new() };
        let (paths, _) = self.inner.tree.selection().selected_rows();
        paths.iter().filter_map(|p| store.iter(p)).map(|i| store.get::<u32>(&i, 0)).collect()
    }

    /// Sélectionne les lignes des joueurs donnés.
    pub fn select(&self, players: &[PlayerRef]) {
        let Some(store) = self.inner.store.borrow().clone() else { return };
        let sel = self.inner.tree.selection();
        sel.unselect_all();
        store.foreach(|_, path, iter| {
            if players.contains(&store.get::<u32>(iter, 0)) {
                sel.select_path(path);
            }
            false
        });
    }

    pub fn expand_all(&self) {
        self.inner.tree.expand_all();
    }

    pub fn collapse_all(&self) {
        self.inner.tree.collapse_all();
    }

    // ----------------------------------------------------------------------
    // Colonnes

    /// Remplace les colonnes affichées (`SetFilter`).
    pub fn set_columns(&self, specs: Vec<ColumnSpec>) {
        if self.inner.columns.borrow().iter().map(key).eq(specs.iter().map(key)) {
            *self.inner.columns.borrow_mut() = specs;
            return;
        }
        let tree = &self.inner.tree;
        self.inner.toggles.borrow_mut().clear();
        while let Some(c) = tree.column(0) {
            tree.remove_column(&c);
        }
        let mut types: Vec<glib::Type> = vec![glib::Type::U32];
        for s in &specs {
            types.push(match s.kind {
                CellKind::Text => glib::Type::STRING,
                CellKind::Icon => Pixbuf::static_type(),
                CellKind::Toggle => glib::Type::BOOL,
                CellKind::Progress => glib::Type::I32,
            });
        }
        types.push(glib::Type::BOOL); // ligne grisée
        let store = gtk::TreeStore::new(&types);
        let dim_col = (types.len() - 1) as u32;

        for (i, spec) in specs.iter().enumerate() {
            let col = (i + 1) as u32;
            let column = gtk::TreeViewColumn::new();
            column.set_title(&spec.title);
            column.set_resizable(true);
            column.set_clickable(true);
            match spec.kind {
                CellKind::Text => {
                    let r = gtk::CellRendererText::new();
                    column.pack_start(&r, true);
                    column.add_attribute(&r, "text", col as i32);
                    column.add_attribute(&r, "sensitive", dim_col as i32);
                    // `sensitive` = vrai quand la ligne n'est pas grisée
                }
                CellKind::Icon => {
                    let r = gtk::CellRendererPixbuf::new();
                    column.pack_start(&r, false);
                    column.add_attribute(&r, "pixbuf", col as i32);
                }
                CellKind::Toggle => {
                    let r = gtk::CellRendererToggle::new();
                    r.set_activatable(spec.editable);
                    self.inner.toggles.borrow_mut().push(r.clone());
                    column.pack_start(&r, false);
                    column.add_attribute(&r, "active", col as i32);
                    if spec.editable {
                        let v = self.clone();
                        let code = spec.code.clone();
                        r.connect_toggled(move |cell, path| {
                            v.on_cell_toggled(&path, &code, cell.is_active());
                        });
                    }
                }
                CellKind::Progress => {
                    let r = gtk::CellRendererProgress::new();
                    column.pack_start(&r, true);
                    column.add_attribute(&r, "value", col as i32);
                }
            }
            {
                let v = self.clone();
                column.connect_clicked(move |_| v.on_header_clicked(i));
            }
            tree.append_column(&column);
        }
        tree.set_model(Some(&store));
        *self.inner.store.borrow_mut() = Some(store);
        *self.inner.columns.borrow_mut() = specs;
        let sort = *self.inner.sort.borrow();
        if let Some((c, _)) = sort {
            if c >= self.inner.columns.borrow().len() {
                *self.inner.sort.borrow_mut() = None;
            }
        }
        self.fill();
    }

    fn on_header_clicked(&self, index: usize) {
        {
            let mut sort = self.inner.sort.borrow_mut();
            *sort = match *sort {
                Some((c, asc)) if c == index => Some((c, !asc)),
                _ => Some((index, true)),
            };
        }
        self.fill();
    }

    /// Une case est cochée : la sélection entière est modifiée si la ligne
    /// cliquée en fait partie (`OnCellToggled`).
    fn on_cell_toggled(&self, path: &gtk::TreePath, code: &str, was_active: bool) {
        if self.inner.locked.get() {
            return;
        }
        let Some(clicked) = self.player_at(path) else { return };
        let selected = self.selected();
        let targets = if selected.contains(&clicked) { selected } else { vec![clicked] };
        let cb = self.inner.on_toggle.borrow().clone();
        if let Some(cb) = cb {
            cb(&targets, code, !was_active);
        }
    }

    // ----------------------------------------------------------------------
    // Contenu

    /// Remplace les lignes (le tri choisi par l'utilisateur est conservé).
    pub fn set_rows(&self, rows: Vec<RowData>) {
        *self.inner.rows.borrow_mut() = rows;
        self.fill();
    }

    fn sorted(&self, rows: &[RowData]) -> Vec<RowData> {
        let mut out: Vec<RowData> = rows.to_vec();
        if let Some((col, asc)) = *self.inner.sort.borrow() {
            let code = self.inner.columns.borrow().get(col).map(|c| c.code.clone());
            if let Some(code) = code {
                bp_core::sort::glib_sort_by(&mut out, |a, b| {
                    let o = bp_core::contest::players::compare_values(
                        &code,
                        a.sort.get(col).and_then(|v| v.as_ref()),
                        b.sort.get(col).and_then(|v| v.as_ref()),
                    );
                    if asc {
                        o
                    } else {
                        o.reverse()
                    }
                });
            }
        }
        for r in &mut out {
            r.children = self.sorted(&r.children);
        }
        out
    }

    fn fill(&self) {
        let Some(store) = self.inner.store.borrow().clone() else { return };
        let selected = self.selected();
        let expanded: Vec<PlayerRef> = {
            let mut v = Vec::new();
            store.foreach(|_, path, iter| {
                if self.inner.tree.row_expanded(path) {
                    v.push(store.get::<u32>(iter, 0));
                }
                false
            });
            v
        };
        store.clear();
        let rows = self.sorted(&self.inner.rows.borrow());
        let ncols = self.inner.columns.borrow().len();
        self.append_rows(&store, None, &rows, ncols);
        // Retrouver l'état d'avant
        store.foreach(|_, path, iter| {
            let r = store.get::<u32>(iter, 0);
            if expanded.contains(&r) {
                self.inner.tree.expand_row(path, false);
            }
            false
        });
        if !selected.is_empty() {
            self.select(&selected);
        }
        self.update_sort_indicators();
    }

    fn append_rows(&self, store: &gtk::TreeStore, parent: Option<&gtk::TreeIter>, rows: &[RowData], ncols: usize) {
        let kinds: Vec<CellKind> = self.inner.columns.borrow().iter().map(|c| c.kind).collect();
        for row in rows {
            let iter = store.append(parent);
            store.set_value(&iter, 0, &row.player.to_value());
            for (i, cell) in row.cells.iter().enumerate().take(ncols) {
                let col = (i + 1) as u32;
                match (cell, kinds[i]) {
                    (Cell::Text(t), CellKind::Text) => store.set_value(&iter, col, &t.to_value()),
                    (Cell::Icon(p), CellKind::Icon) => store.set_value(&iter, col, &p.to_value()),
                    (Cell::Toggle(b), CellKind::Toggle) => store.set_value(&iter, col, &b.to_value()),
                    (Cell::Progress(v), CellKind::Progress) => store.set_value(&iter, col, &v.to_value()),
                    _ => {}
                }
            }
            store.set_value(&iter, (ncols + 1) as u32, &(!row.dimmed).to_value());
            self.append_rows(store, Some(&iter), &row.children, ncols);
        }
    }

    fn update_sort_indicators(&self) {
        let sort = *self.inner.sort.borrow();
        for (i, c) in self.inner.tree.columns().iter().enumerate() {
            match sort {
                Some((col, asc)) if col == i => {
                    c.set_sort_indicator(true);
                    c.set_sort_order(if asc { gtk::SortType::Ascending } else { gtk::SortType::Descending });
                }
                _ => c.set_sort_indicator(false),
            }
        }
    }
}

fn key(c: &ColumnSpec) -> (String, CellKind, bool) {
    (c.code.clone(), c.kind, c.editable)
}
