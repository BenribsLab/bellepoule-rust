//! Formulaire d'ajout et de modification d'un joueur (port de
//! `actors/form.cpp`).
//!
//! Une page par type de joueur (tireur, équipe) ; une ligne par attribut
//! public de la liste fournie. La case à gauche d'un champ permet de le
//! sauter à la touche Tab pour accélérer la saisie. Après un ajout, les
//! champs « uniques » (nom, prénom…) sont vidés et ceux qui se répètent d'un
//! joueur à l'autre (club, ligue…) sont conservés.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use bp_core::attribute::{self, AttrType, AttributeDesc, Look, Rights, Uniqueness, Value};
use bp_core::formats::discrete;
use bp_core::i18n::tr;
use bp_core::player::{AttrId, Player, PlayerClass, PlayerRef};
use bp_gtk::glade::Glade;
use gtk4 as gtk;
use gtk::prelude::*;

/// Ce que l'utilisateur a validé.
pub enum FormEvent {
    /// Nouveau joueur : attributs saisis (`None` = à ne pas renseigner).
    New(PlayerClass, Vec<(String, Option<String>)>),
    /// Mise à jour d'un joueur existant.
    Update(PlayerRef, Vec<(String, Option<String>)>),
}

struct Field {
    code: String,
    widget: gtk::Widget,
    /// Case « inclure ce champ dans la saisie au clavier ».
    skip_check: gtk::CheckButton,
}

struct Page {
    class: PlayerClass,
    fields: Vec<Field>,
    /// Page proposée à l'utilisateur (la page des équipes n'existe que pour une épreuve par équipes).
    visible: Cell<bool>,
}

pub struct PlayerForm {
    glade: Glade,
    window: gtk::Window,
    notebook: gtk::Notebook,
    pages: RefCell<Vec<Page>>,
    editing: Cell<Option<PlayerRef>>,
    locked: Cell<bool>,
    close_on_add: Cell<bool>,
    on_event: RefCell<Option<Rc<dyn Fn(FormEvent)>>>,
}

impl PlayerForm {
    pub fn new() -> Option<Rc<PlayerForm>> {
        let glade = Glade::new("form.ui").ok()?;
        let window: gtk::Window = glade.get("FillInForm");
        let notebook: gtk::Notebook = glade.get("notebook");
        let form = Rc::new(PlayerForm {
            glade,
            window,
            notebook,
            pages: RefCell::new(Vec::new()),
            editing: Cell::new(None),
            locked: Cell::new(false),
            close_on_add: Cell::new(false),
            on_event: RefCell::new(None),
        });
        form.window.set_hide_on_close(true);
        form.connect_handlers();
        Some(form)
    }

    pub fn window(&self) -> &gtk::Window {
        &self.window
    }

    pub fn connect_event(&self, f: impl Fn(FormEvent) + 'static) {
        *self.on_event.borrow_mut() = Some(Rc::new(f));
    }

    pub fn set_close_on_add(&self, close: bool) {
        self.close_on_add.set(close);
    }

    /// Interdit les modifications (phase verrouillée).
    pub fn set_locked(&self, locked: bool) {
        self.locked.set(locked);
    }

    fn connect_handlers(self: &Rc<Self>) {
        let w = Rc::downgrade(self);
        self.glade.connect_simple("on_add_button_clicked", move |_| {
            if let Some(f) = w.upgrade() {
                f.on_add();
            }
        });
        let w = Rc::downgrade(self);
        self.glade.connect_simple("on_close_button_clicked", move |_| {
            if let Some(f) = w.upgrade() {
                f.on_close();
            }
        });
        let w = Rc::downgrade(self);
        self.glade.connect("on_delete_event", move |_| {
            if let Some(f) = w.upgrade() {
                f.on_close();
            }
            Some(true.to_value())
        });
        // Entrée = Ajouter (ou Appliquer en modification). En phase de capture :
        // sinon le champ de saisie consomme la touche avant la fenêtre.
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let w = Rc::downgrade(self);
        keys.connect_key_pressed(move |_, key, _, _| {
            if key == gtk::gdk::Key::Return || key == gtk::gdk::Key::KP_Enter {
                if let Some(f) = w.upgrade() {
                    if f.locked.get() {
                        return gtk::glib::Propagation::Proceed;
                    }
                    f.on_add();
                    return gtk::glib::Propagation::Stop;
                }
            }
            gtk::glib::Propagation::Proceed
        });
        self.window.add_controller(keys);
    }

    // ----------------------------------------------------------------------
    // Construction

    /// `Form::AddPage` : une page pour `attributes` (noms de code).
    pub fn add_page(self: &Rc<Self>, class: PlayerClass, title: &str, attributes: &[String]) {
        let grid = gtk::Grid::new();
        grid.set_row_spacing(2);
        grid.set_column_spacing(6);
        grid.set_margin_top(6);
        grid.set_margin_bottom(6);
        grid.set_margin_start(6);
        grid.set_margin_end(6);

        let mut fields = Vec::new();
        let mut row = 0;
        for code in attributes {
            let Some(desc) = attribute::registry().by_code(code) else { continue };
            if desc.rights != Rights::Public {
                continue;
            }
            let widget = Self::make_field(desc);
            let skip = gtk::CheckButton::new();
            skip.set_active(true);
            skip.set_tooltip_text(Some(&tr(
                "Uncheck this box to quicken the data input. The corresponding field will be ignored on TAB key pressed.",
            )));
            {
                let w = widget.clone();
                skip.connect_toggled(move |c| {
                    w.set_focusable(c.is_active());
                    w.set_can_focus(c.is_active());
                    if let Some(entry) = w.clone().downcast::<gtk::ComboBoxText>().ok().and_then(|c| c.child()) {
                        entry.set_can_focus(c.is_active());
                    }
                });
            }
            let label = gtk::Label::new(Some(&tr(desc.user_name)));
            label.set_xalign(0.0);
            grid.attach(&skip, 0, row, 1, 1);
            grid.attach(&label, 1, row, 1, 1);
            grid.attach(&widget, 2, row, 1, 1);
            fields.push(Field { code: code.clone(), widget, skip_check: skip });
            row += 1;
        }
        let scroll = gtk::ScrolledWindow::new();
        scroll.set_child(Some(&grid));
        scroll.set_min_content_height(380);
        scroll.set_hscrollbar_policy(gtk::PolicyType::Never);
        self.notebook.append_page(&scroll, Some(&gtk::Label::new(Some(&tr(title)))));
        self.pages.borrow_mut().push(Page { class, fields, visible: Cell::new(true) });
        self.connect_country_selector(self.pages.borrow().len() - 1);
        self.update_tabs();
    }

    /// `ShowPage` / `HidePage`.
    pub fn set_page_visible(&self, class: PlayerClass, visible: bool) {
        for p in self.pages.borrow().iter().filter(|p| p.class == class) {
            p.visible.set(visible);
        }
        self.update_tabs();
    }

    /// `ShowTabs` : les onglets n'apparaissent que si plusieurs pages sont proposées.
    fn update_tabs(&self) {
        let visible = self.pages.borrow().iter().filter(|p| p.visible.get()).count();
        self.notebook.set_show_tabs(visible > 1 && self.editing.get().is_none());
    }

    fn make_field(desc: &AttributeDesc) -> gtk::Widget {
        if desc.ty == AttrType::Bool {
            return gtk::CheckButton::new().upcast();
        }
        if desc.has_discrete_value() {
            let items = discrete_items(desc.code_name, None);
            let combo = if desc.free_value_allowed { gtk::ComboBoxText::with_entry() } else { gtk::ComboBoxText::new() };
            bp_gtk::util::scroll_passes_through(&combo);
            for (_, long) in &items {
                combo.append_text(long);
            }
            if !desc.free_value_allowed {
                combo.set_active(Some(0));
            } else if let Some(entry) = combo.child().and_then(|c| c.downcast::<gtk::Entry>().ok()) {
                entry.set_width_chars(28);
            }
            return combo.upcast();
        }
        let entry = gtk::Entry::new();
        entry.set_width_chars(30);
        entry.upcast()
    }

    /// Les listes de clubs, ligues et régions dépendent du pays choisi.
    fn connect_country_selector(self: &Rc<Self>, page_index: usize) {
        let pages = self.pages.borrow();
        let page = &pages[page_index];
        let Some(country) = page.fields.iter().find(|f| f.code == "country") else { return };
        let Ok(combo) = country.widget.clone().downcast::<gtk::ComboBoxText>() else { return };
        let weak = Rc::downgrade(self);
        let refill = move || {
            if let Some(f) = weak.upgrade() {
                f.refilter_localized(page_index);
            }
        };
        combo.connect_changed({
            let refill = refill.clone();
            move |_| refill()
        });
        if let Some(entry) = combo.child().and_then(|c| c.downcast::<gtk::Entry>().ok()) {
            entry.connect_changed(move |_| refill());
        }
    }

    fn refilter_localized(&self, page_index: usize) {
        let pages = self.pages.borrow();
        let page = &pages[page_index];
        let country_text = page.field("country").map(field_text).unwrap_or_default();
        let country_xml = discrete::catalog().map(|c| c.xml_image_from_user("country", &country_text)).unwrap_or(country_text);
        for code in ["region", "league", "club"] {
            let Some(field) = page.field(code) else { continue };
            let Ok(combo) = field.widget.clone().downcast::<gtk::ComboBoxText>() else { continue };
            let keep = field_text(field);
            combo.remove_all();
            for (_, long) in discrete_items(code, Some(&country_xml)) {
                combo.append_text(&long);
            }
            if let Some(e) = combo.child().and_then(|c| c.downcast::<gtk::Entry>().ok()) {
                e.set_text(&keep);
            }
        }
    }

    /// Valeurs proposées pour un attribut dont la liste est dynamique
    /// (noms d'équipes).
    pub fn set_dynamic_values(&self, code: &str, values: &[String]) {
        for page in self.pages.borrow().iter() {
            let Some(field) = page.field(code) else { continue };
            let Ok(combo) = field.widget.clone().downcast::<gtk::ComboBoxText>() else { continue };
            let keep = field_text(field);
            combo.remove_all();
            for v in values {
                combo.append_text(v);
            }
            if let Some(e) = combo.child().and_then(|c| c.downcast::<gtk::Entry>().ok()) {
                e.set_text(&keep);
            }
        }
    }

    // ----------------------------------------------------------------------
    // Affichage

    /// `Form::Show` : nouveau joueur (`None`) ou modification de `player`.
    pub fn show(self: &Rc<Self>, over: Option<&gtk::Window>, player: Option<(PlayerRef, &Player)>) {
        self.editing.set(player.map(|(r, _)| r));
        if let Some(over) = over {
            self.window.set_transient_for(Some(over));
        }
        let pages = self.pages.borrow();
        if let Some((_, p)) = player {
            self.notebook.set_show_tabs(false);
            if let Some(i) = pages.iter().position(|pg| pg.class == p.class) {
                self.notebook.set_current_page(Some(i as u32));
                for field in &pages[i].fields {
                    Self::load_field(field, p);
                }
            }
        } else {
            self.notebook.set_show_tabs(pages.iter().filter(|p| p.visible.get()).count() > 1);
        }
        let editing = player.is_some();
        self.glade.get::<gtk::Widget>("add_button").set_visible(!editing);
        self.glade.get::<gtk::Widget>("apply_button").set_visible(editing && !self.locked.get());
        for page in pages.iter() {
            for f in &page.fields {
                f.widget.set_sensitive(!self.locked.get());
                f.skip_check.set_sensitive(!self.locked.get());
            }
        }
        if let Some(first) = pages.get(self.notebook.current_page().unwrap_or(0) as usize).and_then(|p| p.fields.first()) {
            first.widget.grab_focus();
        }
        drop(pages);
        self.window.present();
    }

    fn load_field(field: &Field, player: &Player) {
        let Some(desc) = attribute::registry().by_code(&field.code) else { return };
        let value = player.get(&AttrId::global(&field.code));
        match (desc.ty, value) {
            (AttrType::Bool, v) => {
                if let Ok(c) = field.widget.clone().downcast::<gtk::CheckButton>() {
                    c.set_active(v.map(Value::as_bool).unwrap_or(false));
                }
            }
            (_, None) => {
                if desc.uniqueness == Uniqueness::NotSingular || !desc.has_discrete_value() {
                    set_field_text(field, "");
                }
            }
            (_, Some(v)) => {
                let text = if desc.has_discrete_value() {
                    discrete::attribute_user_image(desc, v, Look::LongText)
                } else {
                    v.xml_image()
                };
                if desc.has_discrete_value() && !desc.free_value_allowed {
                    if let Ok(c) = field.widget.clone().downcast::<gtk::ComboBoxText>() {
                        let items = discrete_items(&field.code, None);
                        if let Some(i) = items.iter().position(|(_, l)| bp_core::formats::encoding::same_text(l, &text)) {
                            c.set_active(Some(i as u32));
                        }
                    }
                } else {
                    set_field_text(field, &text);
                }
            }
        }
    }

    // ----------------------------------------------------------------------
    // Validation

    /// `Form::ReadAndWipe` : lit la page courante, puis vide les champs uniques.
    fn read_and_wipe(&self, page_index: usize, wipe_only: bool) -> Vec<(String, Option<String>)> {
        let pages = self.pages.borrow();
        let page = &pages[page_index];
        let mut out = Vec::new();
        for field in &page.fields {
            let Some(desc) = attribute::registry().by_code(&field.code) else { continue };
            let singular = desc.uniqueness == Uniqueness::Singular;
            if desc.ty == AttrType::Bool {
                let Ok(c) = field.widget.clone().downcast::<gtk::CheckButton>() else { continue };
                out.push((field.code.clone(), Some(if c.is_active() { "1" } else { "0" }.to_string())));
                if singular {
                    c.set_active(false);
                }
            } else if desc.has_discrete_value() && !desc.free_value_allowed {
                if let Ok(c) = field.widget.clone().downcast::<gtk::ComboBoxText>() {
                    if let Some(i) = c.active() {
                        if let Some((xml, _)) = discrete_items(&field.code, None).get(i as usize) {
                            out.push((field.code.clone(), Some(xml.clone())));
                        }
                    }
                }
            } else {
                let text = field_text(field).trim().to_string();
                if !text.is_empty() || singular {
                    let xml = if desc.has_discrete_value() {
                        discrete::catalog().map(|c| c.xml_image_from_user(&field.code, &text)).unwrap_or(text)
                    } else {
                        text
                    };
                    out.push((field.code.clone(), Some(xml)));
                } else {
                    out.push((field.code.clone(), None));
                }
                if singular {
                    set_field_text(field, "");
                }
            }
        }
        if wipe_only {
            out.clear();
        }
        out
    }

    fn on_add(self: &Rc<Self>) {
        let Some(page_index) = self.notebook.current_page().map(|p| p as usize) else { return };
        if page_index >= self.pages.borrow().len() {
            return;
        }
        let class = self.pages.borrow()[page_index].class;
        let values = self.read_and_wipe(page_index, false);
        let editing = self.editing.get();
        let cb = self.on_event.borrow().clone();
        if let Some(cb) = cb {
            match editing {
                Some(r) => {
                    cb(FormEvent::Update(r, values));
                    self.on_close();
                }
                None => {
                    cb(FormEvent::New(class, values));
                    if self.close_on_add.get() {
                        self.on_close();
                    }
                }
            }
        }
        if let Some(first) = self.pages.borrow().get(page_index).and_then(|p| p.fields.first()) {
            first.widget.grab_focus();
        }
    }

    fn on_close(&self) {
        self.window.set_visible(false);
        for i in 0..self.pages.borrow().len() {
            self.read_and_wipe(i, true);
        }
        self.editing.set(None);
    }
}

impl Page {
    fn field(&self, code: &str) -> Option<&Field> {
        self.fields.iter().find(|f| f.code == code)
    }
}

/// Valeurs `(code XML, texte long)` d'un attribut à valeurs discrètes.
fn discrete_items(code: &str, country: Option<&str>) -> Vec<(String, String)> {
    if let Some(cat) = discrete::catalog() {
        let items = match country {
            Some(c) => cat.values_for_country(code, c),
            None => {
                let desc = attribute::registry().by_code(code);
                let localized = desc.map(|d| matches!(d.discrete_source, Some(attribute::DiscreteSource::Localized(_)))).unwrap_or(false);
                if localized {
                    Vec::new()
                } else {
                    cat.values(code)
                }
            }
        };
        return items.into_iter().map(|i| (i.xml, i.long_text)).collect();
    }
    attribute::registry()
        .by_code(code)
        .map(|d| d.discrete.iter().map(|v| (v.xml.clone(), tr(&v.user))).collect())
        .unwrap_or_default()
}

fn field_text(field: &Field) -> String {
    if let Ok(e) = field.widget.clone().downcast::<gtk::Entry>() {
        return e.text().to_string();
    }
    if let Ok(c) = field.widget.clone().downcast::<gtk::ComboBoxText>() {
        if let Some(e) = c.child().and_then(|w| w.downcast::<gtk::Entry>().ok()) {
            return e.text().to_string();
        }
        return c.active_text().map(|s| s.to_string()).unwrap_or_default();
    }
    String::new()
}

fn set_field_text(field: &Field, text: &str) {
    if let Ok(e) = field.widget.clone().downcast::<gtk::Entry>() {
        e.set_text(text);
    } else if let Ok(c) = field.widget.clone().downcast::<gtk::ComboBoxText>() {
        if let Some(e) = c.child().and_then(|w| w.downcast::<gtk::Entry>().ok()) {
            e.set_text(text);
        }
    }
}
