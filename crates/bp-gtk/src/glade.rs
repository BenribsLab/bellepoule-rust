//! Chargement des interfaces `.ui` (port de `common/util/glade.cpp`).
//!
//! Le C++ appelait `gtk_builder_connect_signals` : chaque `handler` du
//! fichier était une fonction `extern "C"` exportée. Ici :
//! - le fichier est lu, ses chaînes `translatable="yes"` sont traduites avec
//!   `bp_core::i18n::tr` et ses chemins d'images relatifs rendus absolus ;
//! - chaque nom de gestionnaire (`handler="..."`) est enregistré dans un
//!   `BuilderRustScope` qui le redirige vers la table de gestionnaires de la
//!   `Glade` : on les branche avec [`Glade::connect`], avant ou après le
//!   chargement ;
//! - les entrées des menus (modèles `<menu>` produits par la conversion des
//!   `GtkMenu`) deviennent des actions `préfixe.id` dont l'activation appelle
//!   le gestionnaire indiqué par l'attribut `bp-handler`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk4 as gtk;
use gtk::gio;
use gtk::glib;
use gtk::prelude::*;

use crate::resources;

/// Signaux GTK4 dont le gestionnaire doit renvoyer un booléen.
const BOOL_SIGNALS: &[&str] = &["close-request", "key-pressed", "query-tooltip", "activate-link", "scroll"];

/// Appel d'un gestionnaire.
#[derive(Debug, Clone)]
pub struct Signal {
    /// Nom du gestionnaire (`on_new_menuitem_activate`…).
    pub handler: String,
    /// Arguments du signal (le premier est l'émetteur). Vide pour un menu.
    pub values: Vec<glib::Value>,
    /// Identifiant de l'objet émetteur (ou de l'entrée de menu).
    pub object_id: Option<String>,
    /// Cible d'une entrée de menu radio (identifiant de l'entrée choisie).
    pub target: Option<String>,
    /// État d'une entrée cochable ou radio après activation.
    pub active: Option<bool>,
}

impl Signal {
    /// Émetteur du signal converti dans le type voulu.
    pub fn source<T: IsA<glib::Object>>(&self) -> Option<T> {
        self.values.first().and_then(|v| v.get::<glib::Object>().ok()).and_then(|o| o.downcast::<T>().ok())
    }

    /// Argument `i` (0 = émetteur).
    pub fn arg<T: for<'a> glib::value::FromValue<'a> + 'static>(&self, i: usize) -> Option<T> {
        self.values.get(i).and_then(|v| v.get::<T>().ok())
    }
}

/// Gestionnaire de signal : renvoie éventuellement une valeur de retour.
pub type Handler = Rc<dyn Fn(&Signal) -> Option<glib::Value>>;

/// Description d'une entrée de menu lue dans le fichier.
#[derive(Debug, Clone, Default)]
pub struct MenuItemInfo {
    pub id: String,
    pub label: String,
    pub action: String,
    pub target: Option<String>,
    pub handler: Option<String>,
    pub accel: Option<String>,
    pub check: bool,
    pub active: bool,
    pub hidden: bool,
    pub insensitive: bool,
}

struct Inner {
    builder: gtk::Builder,
    path: PathBuf,
    prefix: String,
    handlers: RefCell<HashMap<String, Handler>>,
    bool_handlers: Vec<String>,
    actions: gio::SimpleActionGroup,
    menu_items: Vec<MenuItemInfo>,
    hidden: RefCell<HashMap<String, (gio::Menu, i32, gio::MenuItem)>>,
}

/// Interface chargée (équivalent de la classe `Glade`).
#[derive(Clone)]
pub struct Glade {
    inner: Rc<Inner>,
}

impl std::fmt::Debug for Glade {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Glade").field("path", &self.inner.path).finish()
    }
}

/// Erreur de chargement d'une interface.
#[derive(Debug)]
pub struct GladeError(pub String);

impl std::fmt::Display for GladeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for GladeError {}

fn escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            _ => o.push(c),
        }
    }
    o
}

/// Résultat de l'analyse préalable du fichier.
struct Prepared {
    text: String,
    handlers: Vec<(String, String)>,
    menu_items: Vec<MenuItemInfo>,
}

/// Traduit, absolutise les chemins et recense gestionnaires et menus.
fn prepare(source: &str, base_dir: &Path) -> Result<Prepared, GladeError> {
    let doc = roxmltree::Document::parse(source).map_err(|e| GladeError(format!("XML invalide : {e}")))?;
    let mut replacements: Vec<(std::ops::Range<usize>, String)> = Vec::new();
    let mut handlers = Vec::new();
    let mut menu_items = Vec::new();

    for node in doc.descendants().filter(|n| n.is_element()) {
        let tag = node.tag_name().name();
        if tag == "signal" {
            if let (Some(h), Some(n)) = (node.attribute("handler"), node.attribute("name")) {
                handlers.push((h.to_string(), n.to_string()));
            }
        }
        if (tag == "property" || tag == "attribute") && node.attribute("translatable") == Some("yes") {
            if let Some(t) = node.first_child().filter(|c| c.is_text()) {
                let text = t.text().unwrap_or("");
                let ctx = node.attribute("context");
                let msgid = match ctx {
                    Some(c) => format!("{c}\u{4}{text}"),
                    None => text.to_string(),
                };
                let mut tr = bp_core::i18n::tr(&msgid);
                if tr == msgid {
                    tr = bp_core::i18n::tr(text);
                }
                if tr != text {
                    replacements.push((t.range(), escape(&tr)));
                }
            }
        }
        if tag == "property" && matches!(node.attribute("name"), Some("file") | Some("logo")) {
            if let Some(t) = node.first_child().filter(|c| c.is_text()) {
                let rel = t.text().unwrap_or("").trim();
                if !rel.is_empty() && !Path::new(rel).is_absolute() {
                    let abs = base_dir.join(rel);
                    let abs = abs.canonicalize().unwrap_or(abs);
                    let mut s = abs.to_string_lossy().to_string();
                    if let Some(stripped) = s.strip_prefix(r"\\?\") {
                        s = stripped.to_string();
                    }
                    replacements.push((t.range(), escape(&s)));
                }
            }
        }
        if tag == "item" {
            let mut info = MenuItemInfo::default();
            for a in node.children().filter(|c| c.is_element() && c.tag_name().name() == "attribute") {
                let v = a.text().unwrap_or("").to_string();
                match a.attribute("name").unwrap_or("") {
                    "bp-id" => info.id = v,
                    "label" => info.label = v,
                    "action" => info.action = v,
                    "target" => info.target = Some(v),
                    "bp-handler" => info.handler = Some(v),
                    "accel" => info.accel = Some(v),
                    "bp-check" => info.check = true,
                    "bp-active" => info.active = true,
                    "bp-hidden" => info.hidden = true,
                    "bp-insensitive" => info.insensitive = true,
                    _ => {}
                }
            }
            if !info.action.is_empty() {
                menu_items.push(info);
            }
        }
        if tag == "submenu" {
            let mut id = String::new();
            let mut hidden = false;
            for a in node.children().filter(|c| c.is_element() && c.tag_name().name() == "attribute") {
                match a.attribute("name").unwrap_or("") {
                    "bp-id" => id = a.text().unwrap_or("").to_string(),
                    "bp-hidden" => hidden = true,
                    _ => {}
                }
            }
            if hidden && !id.is_empty() {
                menu_items.push(MenuItemInfo { id, hidden: true, ..Default::default() });
            }
        }
    }

    replacements.sort_by_key(|(r, _)| std::cmp::Reverse(r.start));
    let mut text = source.to_string();
    for (r, s) in replacements {
        text.replace_range(r, &s);
    }
    Ok(Prepared { text, handlers, menu_items })
}

impl Glade {
    /// Charge `resources/ui/<file_name>` ; le préfixe des actions de menus est
    /// le nom du fichier sans extension, `_` remplacés par `-`.
    pub fn new(file_name: &str) -> Result<Glade, GladeError> {
        let path = resources::ui_path(file_name);
        let prefix = Path::new(file_name)
            .file_stem()
            .map(|s| s.to_string_lossy().replace('_', "-"))
            .unwrap_or_else(|| "module".into());
        Glade::from_path(&path, &prefix)
    }

    /// Charge un fichier `.ui` quelconque.
    pub fn from_path(path: &Path, prefix: &str) -> Result<Glade, GladeError> {
        let source = std::fs::read_to_string(path).map_err(|e| GladeError(format!("<<{}>> {e}", path.display())))?;
        let base = path.parent().unwrap_or(Path::new("."));
        Glade::from_string(&source, base, prefix, path.to_path_buf())
    }

    /// Charge une interface depuis une chaîne (chemins relatifs à `base_dir`).
    pub fn from_string(source: &str, base_dir: &Path, prefix: &str, path: PathBuf) -> Result<Glade, GladeError> {
        let prepared = prepare(source, base_dir)?;
        let builder = gtk::Builder::new();
        let scope = gtk::BuilderRustScope::new();

        let mut bool_handlers = Vec::new();
        for (h, sig) in &prepared.handlers {
            if BOOL_SIGNALS.contains(&sig.as_str()) && !bool_handlers.contains(h) {
                bool_handlers.push(h.clone());
            }
        }

        let inner = Rc::new_cyclic(|weak: &std::rc::Weak<Inner>| {
            let mut registered: Vec<String> = Vec::new();
            for (h, _) in &prepared.handlers {
                if registered.contains(h) {
                    continue;
                }
                registered.push(h.clone());
                let weak = weak.clone();
                let name = h.clone();
                let returns_bool = bool_handlers.contains(h);
                scope.add_callback(h.as_str(), move |values| {
                    let inner = weak.upgrade()?;
                    let object_id = values
                        .first()
                        .and_then(|v| v.get::<glib::Object>().ok())
                        .and_then(|o| o.downcast::<gtk::Buildable>().ok())
                        .and_then(|b| b.buildable_id())
                        .map(|s| s.to_string());
                    let signal = Signal { handler: name.clone(), values: values.to_vec(), object_id, target: None, active: None };
                    let result = dispatch(&inner, &signal);
                    match result {
                        Some(v) => Some(v),
                        None if returns_bool => Some(false.to_value()),
                        None => None,
                    }
                });
            }
            Inner {
                builder: builder.clone(),
                path,
                prefix: prefix.to_string(),
                handlers: RefCell::new(HashMap::new()),
                bool_handlers: bool_handlers.clone(),
                actions: gio::SimpleActionGroup::new(),
                menu_items: prepared.menu_items.clone(),
                hidden: RefCell::new(HashMap::new()),
            }
        });

        builder.set_scope(Some(&scope));
        builder
            .add_from_string(&prepared.text)
            .map_err(|e| GladeError(format!("<<{}>> {e}", inner.path.display())))?;
        crate::appearance::polish(&builder, inner.path.file_name().and_then(|name| name.to_str()).unwrap_or(""));

        let glade = Glade { inner };
        glade.create_actions();
        glade.hide_flagged_menu_items();
        for obj in glade.inner.builder.objects() {
            if let Ok(w) = obj.downcast::<gtk::Window>() {
                w.insert_action_group(&glade.inner.prefix, Some(&glade.inner.actions));
            }
        }
        Ok(glade)
    }

    fn create_actions(&self) {
        let prefix = format!("{}.", self.inner.prefix);
        let mut done: Vec<String> = Vec::new();
        for item in &self.inner.menu_items {
            let Some(name) = item.action.strip_prefix(&prefix) else { continue };
            if done.iter().any(|d| d == name) {
                continue;
            }
            done.push(name.to_string());
            let group_items: Vec<MenuItemInfo> =
                self.inner.menu_items.iter().filter(|i| i.action == item.action).cloned().collect();
            let action = if item.target.is_some() {
                let initial = group_items
                    .iter()
                    .find(|i| i.active)
                    .or(group_items.first())
                    .and_then(|i| i.target.clone())
                    .unwrap_or_default();
                gio::SimpleAction::new_stateful(name, Some(glib::VariantTy::STRING), &initial.to_variant())
            } else if item.check {
                gio::SimpleAction::new_stateful(name, None, &item.active.to_variant())
            } else {
                gio::SimpleAction::new(name, None)
            };
            if item.insensitive {
                action.set_enabled(false);
            }
            let weak = Rc::downgrade(&self.inner);
            let leader = group_items.iter().find(|i| i.handler.is_some()).cloned().unwrap_or_else(|| item.clone());
            let is_radio = item.target.is_some();
            let is_check = item.check;
            action.connect_activate(move |action, param| {
                let Some(inner) = weak.upgrade() else { return };
                let mut signal = Signal {
                    handler: leader.handler.clone().unwrap_or_default(),
                    values: Vec::new(),
                    object_id: Some(leader.id.clone()),
                    target: None,
                    active: None,
                };
                if is_radio {
                    let target = param.and_then(|p| p.get::<String>()).unwrap_or_default();
                    action.set_state(&target.to_variant());
                    signal.active = Some(Some(&target) == leader.target.as_ref());
                    signal.target = Some(target);
                } else if is_check {
                    let state = action.state().and_then(|s| s.get::<bool>()).unwrap_or(false);
                    action.set_state(&(!state).to_variant());
                    signal.active = Some(!state);
                }
                if !signal.handler.is_empty() {
                    dispatch(&inner, &signal);
                }
            });
            self.inner.actions.add_action(&action);
        }
    }

    fn hide_flagged_menu_items(&self) {
        let ids: Vec<String> = self.inner.menu_items.iter().filter(|i| i.hidden).map(|i| i.id.clone()).collect();
        for id in ids {
            self.set_menu_item_visible(&id, false);
        }
    }

    /// Branche un gestionnaire (remplace le précédent de même nom).
    pub fn connect(&self, handler: &str, f: impl Fn(&Signal) -> Option<glib::Value> + 'static) {
        self.inner.handlers.borrow_mut().insert(handler.to_string(), Rc::new(f));
    }

    /// Branche un gestionnaire sans valeur de retour.
    pub fn connect_simple(&self, handler: &str, f: impl Fn(&Signal) + 'static) {
        self.connect(handler, move |s| {
            f(s);
            None
        });
    }

    /// Noms des gestionnaires déclarés dans le fichier (signaux et menus).
    pub fn handler_names(&self) -> Vec<String> {
        let mut v: Vec<String> = Vec::new();
        let text = std::fs::read_to_string(&self.inner.path).unwrap_or_default();
        if let Ok(p) = prepare(&text, Path::new(".")) {
            for (h, _) in p.handlers {
                if !v.contains(&h) {
                    v.push(h);
                }
            }
            for m in p.menu_items {
                if let Some(h) = m.handler {
                    if !v.contains(&h) {
                        v.push(h);
                    }
                }
            }
        }
        v
    }

    /// Vrai si le gestionnaire doit renvoyer un booléen.
    pub fn handler_returns_bool(&self, handler: &str) -> bool {
        self.inner.bool_handlers.iter().any(|h| h == handler)
    }

    /// Appelle directement un gestionnaire branché.
    pub fn emit(&self, signal: &Signal) -> Option<glib::Value> {
        dispatch(&self.inner, signal)
    }

    pub fn builder(&self) -> &gtk::Builder {
        &self.inner.builder
    }

    pub fn path(&self) -> &Path {
        &self.inner.path
    }

    /// Préfixe des actions de menus (`tournament`…).
    pub fn action_prefix(&self) -> &str {
        &self.inner.prefix
    }

    /// Groupe d'actions des menus.
    pub fn actions(&self) -> &gio::SimpleActionGroup {
        &self.inner.actions
    }

    /// Action d'une entrée de menu (par identifiant de l'entrée).
    pub fn menu_action(&self, item_id: &str) -> Option<gio::SimpleAction> {
        let item = self.inner.menu_items.iter().find(|i| i.id == item_id)?;
        let name = item.action.split_once('.').map(|(_, n)| n)?;
        self.inner.actions.lookup_action(name).and_then(|a| a.downcast::<gio::SimpleAction>().ok())
    }

    /// Entrées de menus décrites dans le fichier.
    pub fn menu_items(&self) -> &[MenuItemInfo] {
        &self.inner.menu_items
    }

    /// Raccourcis clavier (`action détaillée`, `accélérateur`).
    pub fn accels(&self) -> Vec<(String, String)> {
        self.inner
            .menu_items
            .iter()
            .filter_map(|i| i.accel.clone().map(|a| (i.action.clone(), a)))
            .collect()
    }

    /// Rend les actions de menus accessibles depuis `widget`.
    pub fn attach_actions(&self, widget: &impl IsA<gtk::Widget>) {
        widget.insert_action_group(&self.inner.prefix, Some(&self.inner.actions));
    }

    /// `GetGObject`.
    pub fn object<T: IsA<glib::Object>>(&self, name: &str) -> Option<T> {
        self.inner.builder.object::<T>(name)
    }

    /// `GetWidget` : widget de type quelconque.
    pub fn widget(&self, name: &str) -> Option<gtk::Widget> {
        self.inner.builder.object::<gtk::Widget>(name)
    }

    /// Widget typé ; panique si absent (erreur de programmation).
    pub fn get<T: IsA<glib::Object>>(&self, name: &str) -> T {
        self.inner
            .builder
            .object::<T>(name)
            .unwrap_or_else(|| panic!("objet « {name} » absent de {}", self.inner.path.display()))
    }

    /// `GetRootWidget` : le widget « root ».
    pub fn root_widget(&self) -> Option<gtk::Widget> {
        self.widget("root")
    }

    /// `GetObjectList`.
    pub fn objects(&self) -> Vec<glib::Object> {
        self.inner.builder.objects()
    }

    /// `DetachFromParent` : retire le widget de son parent.
    pub fn detach_from_parent(widget: &impl IsA<gtk::Widget>) {
        detach(widget.as_ref());
    }

    // --- Menus ----------------------------------------------------------

    /// Modèles de menus du fichier (tous les `GMenu`).
    fn menus(&self) -> Vec<gio::Menu> {
        self.inner.builder.objects().into_iter().filter_map(|o| o.downcast::<gio::Menu>().ok()).collect()
    }

    /// Retrouve une entrée par son `bp-id` : (menu parent, position).
    pub fn find_menu_item(&self, id: &str) -> Option<(gio::Menu, i32)> {
        fn search(model: &gio::MenuModel, id: &str) -> Option<(gio::Menu, i32)> {
            for i in 0..model.n_items() {
                let v = model.item_attribute_value(i, "bp-id", Some(glib::VariantTy::STRING));
                if v.and_then(|v| v.get::<String>()).as_deref() == Some(id) {
                    if let Ok(m) = model.clone().downcast::<gio::Menu>() {
                        return Some((m, i));
                    }
                }
                for link in ["section", "submenu"] {
                    if let Some(sub) = model.item_link(i, link) {
                        if let Some(r) = search(&sub, id) {
                            return Some(r);
                        }
                    }
                }
            }
            None
        }
        for m in self.menus() {
            if let Some(r) = search(m.upcast_ref(), id) {
                return Some(r);
            }
        }
        None
    }

    /// Change le libellé d'une entrée de menu (`gtk_menu_item_set_label`).
    pub fn set_menu_item_label(&self, id: &str, label: &str) {
        if let Some((menu, pos)) = self.find_menu_item(id) {
            let item = gio::MenuItem::from_model(&menu, pos);
            item.set_label(Some(label));
            menu.remove(pos);
            menu.insert_item(pos, &item);
        } else if let Some((menu, pos, item)) = self.inner.hidden.borrow_mut().get_mut(id).map(|(m, p, i)| (m.clone(), *p, i.clone())) {
            item.set_label(Some(label));
            let _ = (menu, pos);
        }
    }

    /// Libellé courant d'une entrée de menu.
    pub fn menu_item_label(&self, id: &str) -> Option<String> {
        let (menu, pos) = self.find_menu_item(id)?;
        menu.item_attribute_value(pos, "label", Some(glib::VariantTy::STRING)).and_then(|v| v.get::<String>())
    }

    /// Montre ou cache une entrée de menu (`gtk_widget_show/hide`).
    pub fn set_menu_item_visible(&self, id: &str, visible: bool) {
        if visible {
            if let Some((menu, pos, item)) = self.inner.hidden.borrow_mut().remove(id) {
                let pos = pos.min(menu.n_items());
                menu.insert_item(pos, &item);
            }
        } else if let Some((menu, pos)) = self.find_menu_item(id) {
            let item = gio::MenuItem::from_model(&menu, pos);
            menu.remove(pos);
            self.inner.hidden.borrow_mut().insert(id.to_string(), (menu, pos, item));
        }
    }

    /// Rend une entrée de menu active ou non (`gtk_widget_set_sensitive`).
    pub fn set_menu_item_sensitive(&self, id: &str, sensitive: bool) {
        if let Some(a) = self.menu_action(id) {
            a.set_enabled(sensitive);
        }
    }

    /// État d'une entrée cochable ou radio (`gtk_check_menu_item_get_active`).
    pub fn menu_item_active(&self, id: &str) -> bool {
        let Some(item) = self.inner.menu_items.iter().find(|i| i.id == id) else { return false };
        let Some(action) = self.menu_action(id) else { return false };
        let state = action.state();
        match &item.target {
            Some(t) => state.and_then(|s| s.get::<String>()).as_deref() == Some(t.as_str()),
            None => state.and_then(|s| s.get::<bool>()).unwrap_or(false),
        }
    }

    /// Coche une entrée radio ou cochable (`gtk_check_menu_item_set_active`).
    pub fn set_menu_item_active(&self, id: &str, active: bool) {
        let Some(item) = self.inner.menu_items.iter().find(|i| i.id == id).cloned() else { return };
        let Some(action) = self.menu_action(id) else { return };
        match &item.target {
            Some(t) if active => action.activate(Some(&t.to_variant())),
            Some(_) => {}
            None => {
                if self.menu_item_active(id) != active {
                    action.activate(None);
                }
            }
        }
    }
}

fn dispatch(inner: &Inner, signal: &Signal) -> Option<glib::Value> {
    let handler = inner.handlers.borrow().get(&signal.handler).cloned();
    match handler {
        Some(h) => h(signal),
        None => default_handler(signal),
    }
}

/// Gestionnaires GTK2 génériques utilisés directement dans les fichiers.
fn default_handler(signal: &Signal) -> Option<glib::Value> {
    match signal.handler.as_str() {
        "gtk_widget_hide" => {
            if let Some(w) = signal.source::<gtk::Widget>() {
                w.set_visible(false);
            }
            Some(true.to_value())
        }
        "gtk_widget_destroy" => {
            if let Some(w) = signal.source::<gtk::Window>() {
                w.destroy();
            }
            None
        }
        "gtk_true" => Some(true.to_value()),
        "gtk_false" => Some(false.to_value()),
        _ => {
            log::debug!("gestionnaire non branché : {}", signal.handler);
            None
        }
    }
}

/// Retire un widget de son parent, quel que soit le type du parent.
pub fn detach(widget: &gtk::Widget) {
    let Some(parent) = widget.parent() else { return };
    if let Some(b) = parent.downcast_ref::<gtk::Box>() {
        b.remove(widget);
    } else if let Some(g) = parent.downcast_ref::<gtk::Grid>() {
        g.remove(widget);
    } else if let Some(n) = parent.downcast_ref::<gtk::Notebook>() {
        if let Some(p) = n.page_num(widget) {
            n.remove_page(Some(p));
        } else {
            widget.unparent();
        }
    } else if let Some(f) = parent.downcast_ref::<gtk::Fixed>() {
        f.remove(widget);
    } else if let Some(f) = parent.downcast_ref::<gtk::Frame>() {
        f.set_child(gtk::Widget::NONE);
    } else if let Some(v) = parent.downcast_ref::<gtk::Viewport>() {
        v.set_child(gtk::Widget::NONE);
    } else if let Some(s) = parent.downcast_ref::<gtk::ScrolledWindow>() {
        s.set_child(gtk::Widget::NONE);
    } else if let Some(w) = parent.downcast_ref::<gtk::Window>() {
        w.set_child(gtk::Widget::NONE);
    } else if let Some(b) = parent.downcast_ref::<gtk::Button>() {
        b.set_child(gtk::Widget::NONE);
    } else if let Some(a) = parent.downcast_ref::<gtk::AspectFrame>() {
        a.set_child(gtk::Widget::NONE);
    } else if let Some(o) = parent.downcast_ref::<gtk::Overlay>() {
        o.remove_overlay(widget);
    } else if let Some(p) = parent.downcast_ref::<gtk::Paned>() {
        if p.start_child().as_ref() == Some(widget) {
            p.set_start_child(gtk::Widget::NONE);
        } else {
            p.set_end_child(gtk::Widget::NONE);
        }
    } else if let Some(s) = parent.downcast_ref::<gtk::Stack>() {
        s.remove(widget);
    } else if let Some(p) = parent.downcast_ref::<gtk::Popover>() {
        p.set_child(gtk::Widget::NONE);
    } else {
        widget.unparent();
    }
}

/// Ajoute `child` dans `container` selon le type du conteneur
/// (`gtk_container_add`).
pub fn container_add(container: &gtk::Widget, child: &impl IsA<gtk::Widget>) {
    let child = child.as_ref();
    detach(child);
    if let Some(b) = container.downcast_ref::<gtk::Box>() {
        b.append(child);
    } else if let Some(f) = container.downcast_ref::<gtk::Frame>() {
        f.set_child(Some(child));
    } else if let Some(v) = container.downcast_ref::<gtk::Viewport>() {
        v.set_child(Some(child));
    } else if let Some(s) = container.downcast_ref::<gtk::ScrolledWindow>() {
        s.set_child(Some(child));
    } else if let Some(w) = container.downcast_ref::<gtk::Window>() {
        w.set_child(Some(child));
    } else if let Some(b) = container.downcast_ref::<gtk::Button>() {
        b.set_child(Some(child));
    } else if let Some(a) = container.downcast_ref::<gtk::AspectFrame>() {
        a.set_child(Some(child));
    } else if let Some(g) = container.downcast_ref::<gtk::Grid>() {
        g.attach_next_to(child, None::<&gtk::Widget>, gtk::PositionType::Bottom, 1, 1);
    } else if let Some(n) = container.downcast_ref::<gtk::Notebook>() {
        n.append_page(child, None::<&gtk::Widget>);
    } else if let Some(p) = container.downcast_ref::<gtk::Popover>() {
        p.set_child(Some(child));
    } else if let Some(s) = container.downcast_ref::<gtk::Stack>() {
        s.add_child(child);
    } else {
        child.set_parent(container);
    }
}

/// Premier enfant d'un conteneur (`gtk_container_get_children (…)->data`).
pub fn first_child(container: &gtk::Widget) -> Option<gtk::Widget> {
    container.first_child()
}

/// Enfants directs d'un widget.
pub fn children(container: &gtk::Widget) -> Vec<gtk::Widget> {
    let mut v = Vec::new();
    let mut c = container.first_child();
    while let Some(w) = c {
        c = w.next_sibling();
        v.push(w);
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepare_translates_and_collects() {
        let src = r#"<interface><menu id="m"><section><item><attribute name="label" translatable="yes">File</attribute><attribute name="bp-id">x</attribute><attribute name="action">t.x</attribute><attribute name="bp-handler">on_x</attribute></item></section></menu>
<object class="GtkImage"><property name="file">../glade/images/male.png</property></object>
<object class="GtkButton"><signal name="clicked" handler="on_b"/></object></interface>"#;
        let p = prepare(src, &resources::resources_dir().join("ui")).unwrap();
        assert_eq!(p.handlers, vec![("on_b".to_string(), "clicked".to_string())]);
        assert_eq!(p.menu_items.len(), 1);
        assert_eq!(p.menu_items[0].handler.as_deref(), Some("on_x"));
        assert!(!p.text.contains("../glade"));
        assert!(p.text.contains("male.png"));
    }
}
