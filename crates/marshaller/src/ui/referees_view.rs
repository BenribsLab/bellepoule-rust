//! Listes d'arbitres, un onglet par arme (port de la partie graphique de
//! `RefereesList` et `Checkin` utilisée par le Marshaller).
//!
//! Vue réduite : arbitres présents (nom, charge, club, ligue, région,
//! pays), glissés vers les pistes. Vue étendue (« appel ») : tous les
//! arbitres avec leur présence et leur connexion, ajout, import, retrait.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use bp_core::i18n::tr;
use bp_core::player::AttrId;
use bp_core::{Player, PlayerClass};
use bp_gtk::{dialogs, glade::Glade};
use gtk::prelude::*;
use gtk4 as gtk;

use super::hall_view::{drag_source, Payload};
use super::Ui;
use crate::weapon::WEAPONS;

/// Colonnes du modèle de la liste.
const COL_REF: u32 = 0;
const COL_ATTENDING: u32 = 1;
const COL_CONNECTION: u32 = 2;
const COL_NAME: u32 = 3;
const COL_FIRST_NAME: u32 = 4;
const COL_WORKLOAD: u32 = 5;
const COL_CLUB: u32 = 6;
const COL_LEAGUE: u32 = 7;
const COL_REGION: u32 = 8;
const COL_COUNTRY: u32 = 9;
const COL_BACKGROUND: u32 = 10;
const COL_FOREGROUND: u32 = 11;

struct WeaponList {
    glade: Glade,
    store: gtk::ListStore,
    tree: gtk::TreeView,
    /// Références dans l'ordre des lignes.
    rows: Rc<RefCell<Vec<u32>>>,
    expanded: Cell<bool>,
}

pub struct RefereesView {
    ui: Weak<Ui>,
    lists: Vec<WeaponList>,
}

impl RefereesView {
    pub fn new(ui: &Rc<Ui>, notebook: &gtk::Notebook) -> Rc<RefereesView> {
        let mut lists = Vec::new();
        for weapon in WEAPONS.iter() {
            let glade = Glade::new("referees.ui").expect("referees.ui");
            if ui.local_output.is_some() { super::embedded_layout::referees(&glade); }
            let root: gtk::Widget = glade.get("root");
            Glade::detach_from_parent(&root);
            let header: gtk::Widget = glade.get("tab_header");
            Glade::detach_from_parent(&header);
            glade.get::<gtk::Label>("weapon").set_text(&tr(weapon.image));
            glade.get::<gtk::Widget>("players_list_filter_button").set_visible(false);
            glade.get::<gtk::Widget>("checkin_print_toolbutton").set_visible(false);
            notebook.append_page(&root, Some(&header));

            let store = gtk::ListStore::new(&[
                glib::Type::U32,
                glib::Type::BOOL,
                glib::Type::STRING,
                glib::Type::STRING,
                glib::Type::STRING,
                glib::Type::STRING,
                glib::Type::STRING,
                glib::Type::STRING,
                glib::Type::STRING,
                glib::Type::STRING,
                glib::Type::STRING,
                glib::Type::STRING,
            ]);
            let tree: gtk::TreeView = glade.get("players_list");
            tree.set_model(Some(&store));
            tree.set_reorderable(false);
            tree.selection().set_mode(gtk::SelectionMode::Multiple);
            lists.push(WeaponList { glade, store, tree, rows: Rc::new(RefCell::new(Vec::new())), expanded: Cell::new(false) });
        }
        let view = Rc::new(RefereesView { ui: Rc::downgrade(ui), lists });
        for i in 0..view.lists.len() {
            view.build_columns(i);
            view.connect(i);
        }
        view
    }

    fn ui(&self) -> Option<Rc<Ui>> {
        self.ui.upgrade()
    }

    /// En mode intégré, limiter les onglets aux armes des compétitions ouvertes.
    /// Sans compétition, garder le catalogue complet pour préparer les arbitres.
    pub fn set_visible_weapons(&self, weapons: &[&str]) {
        // Réafficher d'abord les pages utiles pour que GTK puisse choisir une
        // page visible quand l'arme de la dernière compétition change.
        for visible in [true, false] {
            for (weapon, list) in WEAPONS.iter().zip(&self.lists) {
                let show = weapons.is_empty()
                    || weapons.iter().any(|code| weapon.xml.eq_ignore_ascii_case(code));
                if show == visible {
                    for id in ["root", "tab_header"] {
                        let widget = list.glade.get::<gtk::Widget>(id);
                        if widget.is_visible() != show {
                            widget.set_visible(show);
                        }
                    }
                }
            }
        }
    }

    /// Colonnes de la vue réduite (`_collapsed_filter`) ou étendue
    /// (`_expanded_filter`).
    fn build_columns(self: &Rc<Self>, list: usize) {
        let l = &self.lists[list];
        for c in l.tree.columns() {
            l.tree.remove_column(&c);
        }
        let expanded = l.expanded.get();
        let text_column = |title: &str, col: u32| {
            let renderer = gtk::CellRendererText::new();
            let column = gtk::TreeViewColumn::new();
            column.set_title(title);
            column.pack_start(&renderer, true);
            column.add_attribute(&renderer, "text", col as i32);
            column.add_attribute(&renderer, "cell-background", COL_BACKGROUND as i32);
            column.add_attribute(&renderer, "foreground", COL_FOREGROUND as i32);
            column.set_resizable(true);
            l.tree.append_column(&column);
        };
        if expanded {
            text_column(&tr("Connection"), COL_CONNECTION);
            let toggle = gtk::CellRendererToggle::new();
            let column = gtk::TreeViewColumn::new();
            column.set_title(&tr("Presence"));
            column.pack_start(&toggle, false);
            column.add_attribute(&toggle, "active", COL_ATTENDING as i32);
            l.tree.append_column(&column);
            let w = Rc::downgrade(self);
            toggle.connect_toggled(move |_, path| {
                let Some(v) = w.upgrade() else { return };
                let Some(i) = path.indices().first().copied() else { return };
                let Some(r) = v.lists[list].rows.borrow().get(i as usize).copied() else { return };
                if let Some(ui) = v.ui() {
                    let attending = ui.hall.borrow().referees.get(r).map(|x| x.player.attending()).unwrap_or(false);
                    ui.hall.borrow_mut().set_referee_attending(r, !attending);
                    ui.after_change();
                }
            });
        }
        text_column(&tr("Last name"), COL_NAME);
        if expanded {
            text_column(&tr("First name"), COL_FIRST_NAME);
        }
        text_column(&tr("Workload"), COL_WORKLOAD);
        text_column(&tr("Club"), COL_CLUB);
        text_column(&tr("League"), COL_LEAGUE);
        if !expanded {
            text_column(&tr("Region"), COL_REGION);
        }
        text_column(&tr("Country"), COL_COUNTRY);
    }

    fn connect(self: &Rc<Self>, list: usize) {
        let l = &self.lists[list];
        macro_rules! on {
            ($name:expr, $body:expr) => {{
                let w = Rc::downgrade(self);
                let f = $body;
                l.glade.connect_simple($name, move |_| {
                    if let Some(v) = w.upgrade() {
                        f(&v);
                    }
                });
            }};
        }
        on!("on_checkin_clicked", move |v: &Rc<RefereesView>| v.expand(list));
        on!("on_all_present_button_clicked", move |v: &Rc<RefereesView>| v.act(|h| h.set_list_attending(list, true)));
        on!("on_all_absent_button_clicked", move |v: &Rc<RefereesView>| v.act(|h| h.set_list_attending(list, false)));
        on!("on_add_player_button_clicked", move |v: &Rc<RefereesView>| v.edit(list, None));
        on!("on_remove_player_button_clicked", move |v: &Rc<RefereesView>| {
            let selection = v.selected_in(list);
            if !selection.is_empty() {
                v.act(|h| h.remove_referees(list, &selection));
            }
        });
        on!("on_import_toolbutton_clicked", move |v: &Rc<RefereesView>| v.import(list));
        {
            let w = Rc::downgrade(self);
            l.tree.connect_row_activated(move |_, path, _| {
                let Some(v) = w.upgrade() else { return };
                if !v.lists[list].expanded.get() {
                    return;
                }
                let Some(i) = path.indices().first().copied() else { return };
                let r = v.lists[list].rows.borrow().get(i as usize).copied();
                if let Some(r) = r {
                    v.edit(list, Some(r));
                }
            });
        }
        // Glisser d'un arbitre vers une piste (vue réduite).
        {
            let w = Rc::downgrade(self);
            drag_source(&l.tree, move |row| {
                let v = w.upgrade()?;
                if v.lists[list].expanded.get() && v.ui().is_none_or(|ui| ui.local_output.is_none()) {
                    return None;
                }
                let under = row.and_then(|r| v.lists[list].rows.borrow().get(r).copied());
                under.or_else(|| v.selected_in(list).first().copied()).map(Payload::Referee)
            });
        }
        // Disponibilité pour l'arme (menu contextuel « RefereesList::JobActions »).
        {
            let click = gtk::GestureClick::new();
            click.set_button(3);
            let w = Rc::downgrade(self);
            click.connect_pressed(move |_, _, x, y| {
                let Some(v) = w.upgrade() else { return };
                v.popup(list, x, y);
            });
            l.tree.add_controller(click);
        }
    }

    fn act(&self, f: impl FnOnce(&mut crate::hall::Hall)) {
        if let Some(ui) = self.ui() {
            f(&mut ui.hall.borrow_mut());
            ui.after_change();
        }
    }

    fn selected_in(&self, list: usize) -> Vec<u32> {
        let l = &self.lists[list];
        let (paths, _) = l.tree.selection().selected_rows();
        paths.iter().filter_map(|p| p.indices().first().and_then(|i| l.rows.borrow().get(*i as usize).copied())).collect()
    }

    /// Arbitres sélectionnés dans la liste d'une arme (`RetreiveSelectedPlayers`).
    pub fn selected(&self, weapon: &str) -> Vec<u32> {
        match crate::weapon::index_of(weapon) {
            Some(i) if i < self.lists.len() => self.selected_in(i),
            _ => Vec::new(),
        }
    }

    /// Menu contextuel : disponible / indisponible pour l'arme.
    fn popup(self: &Rc<Self>, list: usize, x: f64, y: f64) {
        let l = &self.lists[list];
        if l.expanded.get() {
            return;
        }
        let popover = gtk::Popover::new();
        let vbox = gtk::Box::new(gtk::Orientation::Vertical, 2);
        for (label, available) in [(tr("Available"), true), (tr("Not available"), false)] {
            let b = gtk::Button::with_label(&label);
            b.set_has_frame(false);
            let w = Rc::downgrade(self);
            let p = popover.clone();
            b.connect_clicked(move |_| {
                p.popdown();
                if let Some(v) = w.upgrade() {
                    let selection = v.selected_in(list);
                    v.act(|h| h.set_referee_availability(list, &selection, available));
                }
            });
            vbox.append(&b);
        }
        popover.set_child(Some(&vbox));
        popover.set_parent(&l.tree);
        popover.set_pointing_to(Some(&gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
        popover.connect_closed(|p| p.unparent());
        popover.popup();
    }

    /// `OnCheckinClicked` → `Marshaller::OnOpenCheckin` + `ExpandAll`.
    fn expand(self: &Rc<Self>, list: usize) {
        for (i, l) in self.lists.iter().enumerate() {
            l.expanded.set(true);
            l.glade.get::<gtk::Widget>("edit_panel").set_visible(true);
            self.build_columns(i);
        }
        if let Some(ui) = self.ui() {
            ui.open_checkin(list as u32);
        }
        self.refresh();
    }

    /// `CollapseAll`.
    pub fn set_expanded(self: &Rc<Self>, expanded: bool) {
        for (i, list) in self.lists.iter().enumerate() {
            list.expanded.set(expanded);
            list.glade.get::<gtk::Widget>("edit_panel").set_visible(expanded);
            self.build_columns(i);
        }
        self.refresh();
    }

    pub fn collapse_all(self: &Rc<Self>) {
        for (i, l) in self.lists.iter().enumerate() {
            l.expanded.set(false);
            l.glade.get::<gtk::Widget>("edit_panel").set_visible(false);
            self.build_columns(i);
        }
    }

    /// Réaffiche les listes (sélection conservée).
    pub fn refresh(&self) {
        let Some(ui) = self.ui() else { return };
        let hall = ui.hall.borrow();
        for (i, l) in self.lists.iter().enumerate() {
            let selected = self.selected_in(i);
            let members = &hall.referees.lists[i].members;
            let (present, absent) = hall.referees.tally(i);
            l.glade.get::<gtk::Label>("attending_tab").set_text(&present.to_string());
            l.glade.get::<gtk::Label>("absent_tab").set_text(&absent.to_string());
            l.glade.get::<gtk::Label>("attending_label").set_text(&present.to_string());
            l.glade.get::<gtk::Label>("absent_label").set_text(&absent.to_string());

            l.store.clear();
            let mut rows = Vec::new();
            for r in members {
                let Some(referee) = hall.referees.get(*r) else { continue };
                let p = &referee.player;
                if !l.expanded.get() && !p.attending() {
                    continue;
                }
                let get = |code: &str| p.attr(code).map(|v| v.xml_image()).unwrap_or_default();
                let iter = l.store.append();
                let workload = format!("{}%", p.attr("workload_rate").map(|v| v.as_uint()).unwrap_or(0));
                let foreground = if hall.referees.available_for_weapon(*r, i) { "black" } else { "grey" };
                l.store.set(
                    &iter,
                    &[
                        (COL_REF, r),
                        (COL_ATTENDING, &p.attending()),
                        (COL_CONNECTION, &get("connection")),
                        (COL_NAME, &p.name()),
                        (COL_FIRST_NAME, &p.first_name()),
                        (COL_WORKLOAD, &workload),
                        (COL_CLUB, &get("club")),
                        (COL_LEAGUE, &get("league")),
                        (COL_REGION, &get("region")),
                        (COL_COUNTRY, &get("country")),
                        (COL_FOREGROUND, &foreground),
                    ],
                );
                if let Some(c) = referee.cell_color {
                    l.store.set(&iter, &[(COL_BACKGROUND, &c.to_css())]);
                }
                rows.push(*r);
            }
            for (k, r) in rows.iter().enumerate() {
                if selected.contains(r) {
                    l.tree.selection().select_path(&gtk::TreePath::from_indices(&[k as i32]));
                }
            }
            *l.rows.borrow_mut() = rows;
        }
    }

    /// Import d'un fichier d'arbitres (FFF ou CSV).
    fn import(&self, list: usize) {
        let Some(ui) = self.ui() else { return };
        let title = tr("Choose a referee file to import...");
        let mut chooser = dialogs::FileChooser::new(&title, gtk::FileChooserAction::Open);
        chooser.filters.push(dialogs::file_filter(Some(&tr("All referee files (FFF, CSV)")), &["*.fff", "*.FFF", "*.csv", "*.CSV"]));
        chooser.filters.push(dialogs::all_files_filter());
        let (response, file) = chooser.run();
        if response != gtk::ResponseType::Accept {
            return;
        }
        let Some(path) = file else { return };
        let result = ui.hall.borrow_mut().import_referees(list, &path);
        if let Err(e) = result {
            dialogs::error(Some(ui.window()), &e);
        }
        ui.after_change();
    }

    /// Formulaire d'un arbitre (création ou modification).
    fn edit(&self, list: usize, existing: Option<u32>) {
        let Some(ui) = self.ui() else { return };
        let current = existing.and_then(|r| ui.hall.borrow().referees.get(r).map(|x| x.player.clone()));
        let dialog = gtk::Dialog::with_buttons(
            Some(&tr("Referee")),
            Some(ui.window()),
            gtk::DialogFlags::MODAL,
            &[(&tr("_Cancel"), gtk::ResponseType::Cancel), (&tr("_OK"), gtk::ResponseType::Ok)],
        );
        dialog.set_default_response(gtk::ResponseType::Ok);
        let grid = gtk::Grid::new();
        grid.set_row_spacing(4);
        grid.set_column_spacing(8);
        grid.set_margin_start(10);
        grid.set_margin_end(10);
        grid.set_margin_top(10);
        grid.set_margin_bottom(10);
        let fields = [
            ("name", tr("Last name")),
            ("first_name", tr("First name")),
            ("club", tr("Club")),
            ("league", tr("League")),
            ("country", tr("Country")),
            ("licence", tr("Licence")),
            ("level", tr("Level")),
        ];
        let mut entries = Vec::new();
        for (row, (code, label)) in fields.iter().enumerate() {
            let l = gtk::Label::new(Some(label));
            l.set_xalign(1.0);
            let e = gtk::Entry::new();
            e.set_activates_default(true);
            if let Some(p) = &current {
                e.set_text(&p.attr(code).map(|v| v.xml_image()).unwrap_or_default());
            }
            grid.attach(&l, 0, row as i32, 1, 1);
            grid.attach(&e, 1, row as i32, 1, 1);
            entries.push((*code, e));
        }
        let attending = gtk::CheckButton::with_label(&tr("Present"));
        attending.set_active(current.as_ref().map(|p| p.attending()).unwrap_or(true));
        grid.attach(&attending, 1, fields.len() as i32, 1, 1);
        // Les QR codes SmartPoule sont dans BellePoule (BellePoule2D > SmartPoule…).
        dialog.content_area().append(&grid);
        let ok = dialogs::run_dialog(&dialog) == gtk::ResponseType::Ok;
        if ok {
            let mut player = current.clone().unwrap_or_else(|| Player::new(PlayerClass::Referee));
            for (code, e) in &entries {
                let text = e.text().to_string();
                if !text.trim().is_empty() || player.attr(code).is_some() {
                    player.set_str(&AttrId::global(code), text.trim());
                }
            }
            player.set(&AttrId::global("attending"), bp_core::attribute::Value::Bool(attending.is_active()));
            if !player.name().trim().is_empty() {
                ui.hall.borrow_mut().save_referee_form(list, player, existing);
                ui.after_change();
            }
        }
        dialog.destroy();
    }
}

use gtk::glib;
