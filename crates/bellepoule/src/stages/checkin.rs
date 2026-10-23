//! Écran du pointage, « Appel » (port de `supervisor/rounds/checkin` et
//! `actors/checkin.cpp`, parties interface) : liste des tireurs (ou des
//! équipes), présence, ajout, suppression, import, classement.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::{Rc, Weak};

use bp_core::attribute::Value;
use bp_core::contest::players::new_player;
use bp_core::i18n::tr;
use bp_core::player::{AttrId, Owner, Player, PlayerClass, PlayerRef};
use bp_core::rounds::checkin::{CheckinStage, ToggleOutcome};
use bp_core::rounds::checkin::RankImporter;
use bp_core::stage::{Stage, StageContext};
use bp_gtk::dialogs::{self, FileChooser};
use bp_gtk::glade::Glade;
use gtk4 as gtk;
use gtk::prelude::*;

use super::StageScreen;
use crate::contest_page::ContestPage;
use crate::widgets::filter_dialog;
use crate::widgets::list_document::{self, ListDocument};
use crate::widgets::form::{FormEvent, PlayerForm};
use crate::widgets::players_list::{Cell as ListCell, ColumnSpec, PlayersListView, RowData};
use crate::widgets::render;

/// Contenu de l'écran à un instant donné.
struct Snapshot {
    specs: Vec<ColumnSpec>,
    rows: Vec<RowData>,
    presents: u32,
    absents: u32,
    team_event: bool,
    locked: bool,
    minimum_team_size: u32,
    team_size: u32,
    substitutes: u32,
    manual_classification: bool,
    default_classification: u32,
    team_names: Vec<String>,
}

pub struct CheckinScreen {
    page: Weak<ContestPage>,
    glade: Glade,
    root: gtk::Widget,
    list: PlayersListView,
    form: Rc<PlayerForm>,
    /// Les changements venant du code ne sont pas des actions de l'utilisateur.
    updating: Cell<bool>,
    search: RefCell<String>,
    attending_filter: Cell<Option<bool>>,
    footer: gtk::Label,
    /// Composition des équipes d'une rencontre : tireurs et remplaçants.
    team_players: gtk::SpinButton,
    team_substitutes: gtk::SpinButton,
}

impl CheckinScreen {
    pub fn new(page: &Rc<ContestPage>) -> Rc<CheckinScreen> {
        let glade = Glade::new("checkin.ui").expect("checkin.ui");
        let root: gtk::Widget = glade.get("root");
        Glade::detach_from_parent(&root);
        let list = PlayersListView::in_scrolled(glade.get("scrolledwindow3"));
        let form = PlayerForm::new().expect("form.ui");
        let footer = gtk::Label::new(None);
        footer.set_xalign(0.0);
        footer.add_css_class("bp-table-footer");
        if let Some(container) = root.downcast_ref::<gtk::Box>() { container.append(&footer); }

        // Pages du formulaire : tireur, et équipe pour les épreuves par équipes.
        let attrs = page
            .with_contest(|c| {
                c.schedule.checkin().map(|ch| (ch.filter.attr_list().to_vec(), ch.team_filter.attr_list().to_vec()))
            })
            .flatten();
        if let Some((fencer, team)) = attrs {
            form.add_page(PlayerClass::Fencer, "Fencer", &fencer);
            form.add_page(PlayerClass::Team, "Team", &team);
        }

        // Composition des équipes, à côté de la taille minimale.
        let team_players = gtk::SpinButton::with_range(1.0, 20.0, 1.0);
        let team_substitutes = gtk::SpinButton::with_range(0.0, 10.0, 1.0);
        if let Some(row) = glade.object::<gtk::Box>("hbox5") {
            for (label, spin) in [("Tireurs par équipe", &team_players), ("Remplaçants", &team_substitutes)] {
                let l = gtk::Label::new(Some(label));
                l.set_margin_start(12);
                row.append(&l);
                row.append(spin);
            }
        }

        let screen = Rc::new(CheckinScreen {
            page: Rc::downgrade(page),
            glade,
            root,
            list,
            form,
            updating: Cell::new(false),
            search: RefCell::new(String::new()),
            attending_filter: Cell::new(None),
            footer,
            team_players,
            team_substitutes,
        });
        for spin in [&screen.team_players, &screen.team_substitutes] {
            let weak = Rc::downgrade(&screen);
            spin.connect_value_changed(move |_| {
                if let Some(s) = weak.upgrade() {
                    s.on_team_size();
                }
            });
        }
        screen.connect();
        screen.add_search_controls();
        screen.refresh();
        screen
    }

    fn page(&self) -> Option<Rc<ContestPage>> {
        self.page.upgrade()
    }

    fn add_search_controls(self: &Rc<Self>) {
        let header: gtk::Box = self.glade.get("hbox1");
        let Some(stats) = header.first_child().and_downcast::<gtk::FlowBox>() else { return };
        let filters = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        filters.set_valign(gtk::Align::Center);
        filters.add_css_class("bp-segmented");
        let all = gtk::ToggleButton::with_label(&tr("All"));
        all.set_active(true);
        let present = gtk::ToggleButton::with_label(&tr("Presents"));
        let absent = gtk::ToggleButton::with_label(&tr("Absents"));
        present.set_group(Some(&all));
        absent.set_group(Some(&all));
        for (button, status) in [(all, None), (present, Some(true)), (absent, Some(false))] {
            let weak = Rc::downgrade(self);
            button.connect_toggled(move |button| {
                if button.is_active() {
                    if let Some(screen) = weak.upgrade() {
                        screen.attending_filter.set(status);
                        screen.apply_search();
                    }
                }
            });
            filters.append(&button);
        }
        stats.insert(&filters, -1);
        let search = gtk::SearchEntry::new();
        search.set_placeholder_text(Some(&tr("Search a participant…")));
        search.set_width_request(190);
        search.set_valign(gtk::Align::Center);
        let weak = Rc::downgrade(self);
        search.connect_search_changed(move |entry| {
            if let Some(screen) = weak.upgrade() {
                *screen.search.borrow_mut() = entry.text().to_string();
                screen.apply_search();
            }
        });
        stats.insert(&search, -1);
    }

    fn apply_search(&self) {
        self.list.set_filter(&self.search.borrow(), self.attending_filter.get());
        self.update_footer();
    }

    fn update_footer(&self) {
        let (visible, total) = self.list.row_counts();
        self.footer.set_text(&format!("{visible} / {total} {}", tr("rows displayed")));
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
        on!("on_add_player_button_clicked", on_add);
        on!("on_remove_player_button_clicked", on_remove);
        on!("on_import_toolbutton_clicked", on_import);
        on!("on_ranking_toolbutton_clicked", on_ranking);
        on!("on_players_list_filter_button_clicked", on_filter);
        on!("on_checkin_print_toolbutton_clicked", on_print);
        on!("on_all_present_button_clicked", on_all_present);
        on!("on_all_absent_button_clicked", on_all_absent);
        on!("on_expand_eventbox_button_press_event", on_expand);
        on!("on_collapse_eventbox_button_press_event", on_collapse);
        on!("on_teamsize_entry_value_changed", on_team_config);
        on!("on_manual_radiobutton_toggled", on_team_config);
        on!("on_worst_entry_activate", on_team_config);
        on!("on_worst_entry_changed", on_team_config);
        on!("on_worst_entry_focus_out_event", on_team_config);

        {
            let w = Rc::downgrade(self);
            self.list.connect_toggled(move |targets, code, value| {
                if let Some(s) = w.upgrade() {
                    s.on_toggled(targets, code, value);
                }
            });
        }
        {
            let w = Rc::downgrade(self);
            self.list.connect_activated(move |r| {
                if let Some(s) = w.upgrade() {
                    s.on_row_activated(r);
                }
            });
        }
        {
            let w = Rc::downgrade(self);
            self.form.connect_event(move |event| {
                if let Some(s) = w.upgrade() {
                    s.on_form_event(event);
                }
            });
        }
    }

    // ----------------------------------------------------------------------
    // Contenu

    fn snapshot(c: &mut bp_core::contest::Contest) -> Option<Snapshot> {
        let team_event = c.settings.team_event;
        let settings = c.settings.clone();
        let checkin = c.schedule.checkin()?;
        let store = &c.players;
        let owner: Owner = checkin.base().owner();
        let specs = render::column_specs(checkin.filter.layout(), !checkin.base().locked);
        let tally = checkin.tally(store, team_event);

        let row = |r: PlayerRef, children: Vec<RowData>| -> Option<RowData> {
            let p = store.get(r)?;
            Some(RowData {
                player: r,
                cells: specs.iter().map(|s| render::cell_for(p, s, owner)).collect(),
                sort: render::sort_values(p, &specs, owner),
                children,
                dimmed: !p.attending(),
            })
        };

        let mut rows = Vec::new();
        if team_event {
            // « ** Sans équipe ** » : les tireurs qui n'ont pas d'équipe
            let loose: Vec<RowData> =
                checkin.fencers(store).into_iter().filter(|r| store.get(*r).map(|p| p.team.is_none()).unwrap_or(false)).filter_map(|r| row(r, Vec::new())).collect();
            let mut null_team = RowData {
                player: 0,
                cells: specs.iter().map(|_| ListCell::Text(String::new())).collect(),
                sort: specs.iter().map(|_| None).collect(),
                children: loose,
                dimmed: false,
            };
            if let Some(i) = specs.iter().position(|s| s.code == "name") {
                null_team.cells[i] = ListCell::Text(tr("** Without team **"));
            }
            rows.push(null_team);
            for t in checkin.teams(store) {
                let members: Vec<RowData> =
                    store.get(t).map(|p| p.members.clone()).unwrap_or_default().into_iter().filter_map(|m| row(m, Vec::new())).collect();
                if let Some(r) = row(t, members) {
                    rows.push(r);
                }
            }
        } else {
            for r in checkin.fencers(store) {
                if let Some(r) = row(r, Vec::new()) {
                    rows.push(r);
                }
            }
        }
        Some(Snapshot {
            specs,
            rows,
            presents: tally.presents(),
            absents: tally.absents(),
            team_event,
            locked: checkin.base().locked,
            minimum_team_size: settings.minimum_team_size,
            team_size: settings.team_size,
            substitutes: settings.substitutes,
            manual_classification: settings.manual_classification,
            default_classification: settings.default_classification,
            team_names: checkin.team_names(store),
        })
    }

    fn apply(&self, s: Snapshot) {
        self.updating.set(true);
        self.list.set_locked(s.locked);
        self.list.set_columns(s.specs);
        self.list.set_rows(s.rows);
        self.update_footer();
        self.glade.get::<gtk::Label>("attending_label").set_text(&s.presents.to_string());
        self.glade.get::<gtk::Label>("absent_label").set_text(&s.absents.to_string());

        self.glade.get::<gtk::Widget>("team_hbox").set_visible(s.team_event);
        self.glade.get::<gtk::Widget>("tree_control_hbox").set_visible(s.team_event);
        self.glade.get::<gtk::SpinButton>("teamsize_entry").set_value(s.minimum_team_size as f64);
        self.team_players.set_value(s.team_size as f64);
        self.team_substitutes.set_value(s.substitutes as f64);
        self.glade.get::<gtk::CheckButton>("manual_radiobutton").set_active(s.manual_classification);
        self.glade.get::<gtk::CheckButton>("derived_radiobutton").set_active(!s.manual_classification);
        let worst: gtk::Entry = self.glade.get("worst_entry");
        if worst.text().as_str() != s.default_classification.to_string() && !crate::widgets::entry_has_focus(&worst) {
            worst.set_text(&s.default_classification.to_string());
        }
        for id in [
            "add_player_button",
            "remove_player_button",
            "import_toolbutton",
            "ranking_toolbutton",
            "all_present_button",
            "all_absent_button",
            "team_hbox",
        ] {
            self.glade.get::<gtk::Widget>(id).set_sensitive(!s.locked);
        }
        self.form.set_locked(s.locked);
        self.form.set_dynamic_values("team", &s.team_names);
        self.form.set_page_visible(PlayerClass::Team, s.team_event);
        self.updating.set(false);
    }

    /// Modifie le pointage avec le contexte des phases ; relit l'écran ensuite.
    fn edit<R>(&self, f: impl FnOnce(&mut CheckinStage, &mut StageContext) -> R) -> Option<R> {
        let page = self.page()?;
        let result = page.with_contest(|c| c.with_schedule(|s, ctx| s.checkin_mut().map(|ch| f(ch, ctx)))).flatten();
        page.mark_dirty();
        self.refresh();
        if let Some(sv) = page.schedule.borrow().as_ref() {
            sv.refresh_sensitivity();
        }
        result
    }

    // ----------------------------------------------------------------------
    // Actions

    fn selected_or_none(&self) -> Vec<PlayerRef> {
        self.list.selected().into_iter().filter(|r| *r != 0).collect()
    }

    fn on_add(self: &Rc<Self>) {
        self.form.show(self.page().and_then(|p| p.window()).as_ref(), None);
    }

    fn on_row_activated(self: &Rc<Self>, r: PlayerRef) {
        if r == 0 {
            return;
        }
        let Some(page) = self.page() else { return };
        let player: Option<Player> = page.with_contest(|c| c.players.get(r).cloned()).flatten();
        if let Some(p) = player {
            self.form.show(page.window().as_ref(), Some((r, &p)));
        }
    }

    fn on_form_event(self: &Rc<Self>, event: FormEvent) {
        match event {
            FormEvent::New(class, values) => {
                if self.team_full(None, &values) {
                    return;
                }
                let mut player = new_player(class);
                for (code, value) in &values {
                    if let Some(v) = value {
                        if let Some(desc) = bp_core::attribute::registry().by_code(code) {
                            player.set(&AttrId::global(code), Value::parse(desc.ty, v));
                        }
                    }
                }
                self.edit(|ch, ctx| ch.form_new_player(ctx, player));
            }
            FormEvent::Update(r, values) => {
                if self.team_full(Some(r), &values) {
                    return;
                }
                self.edit(|ch, ctx| {
                    for (code, value) in &values {
                        ch.set_player_attribute(ctx, r, code, value.as_deref());
                    }
                    ch.form_update_player(ctx, r);
                });
            }
        }
    }

    /// `PlayersList::RemoveSelection` : les équipes entraînent leurs membres.
    fn on_remove(self: &Rc<Self>) {
        let selected = self.selected_or_none();
        if selected.is_empty() {
            return;
        }
        self.edit(|ch, ctx| {
            for r in selected {
                if ctx.players.get(r).is_some() {
                    ch.remove(ctx, r);
                }
            }
        });
    }

    /// Case de présence (ou autre case) cochée.
    fn on_toggled(self: &Rc<Self>, targets: &[PlayerRef], code: &str, value: bool) {
        let targets: Vec<PlayerRef> = targets.iter().copied().filter(|r| *r != 0).collect();
        let outcome = self.edit(|ch, ctx| {
            let mut last = ToggleOutcome::Done;
            for r in &targets {
                if code == "attending" {
                    let o = ch.set_attending(ctx, *r, value);
                    if o != ToggleOutcome::Done {
                        last = o;
                    }
                } else {
                    ch.set_player_attribute(ctx, *r, code, Some(if value { "1" } else { "0" }));
                }
            }
            last
        });
        if let Some(ToggleOutcome::TeamNotToggleable { minimum_team_size }) = outcome {
            let message = format!(
                "{}\n{}\n\n{}",
                tr("Tick the present players of the team."),
                tr("Untick the absent players of the team."),
                tr("You have configured the minimum team size to <b>%d</b> fencers.").replace("%d", &minimum_team_size.to_string())
            );
            dialogs::run_message(
                self.page().and_then(|p| p.window()).as_ref(),
                gtk::MessageType::Info,
                gtk::ButtonsType::Close,
                &message,
                None,
                None,
            );
        }
    }

    fn on_all_present(self: &Rc<Self>) {
        self.edit(|ch, ctx| ch.set_all_attending(ctx, true));
    }

    fn on_all_absent(self: &Rc<Self>) {
        self.edit(|ch, ctx| ch.set_all_attending(ctx, false));
    }

    fn on_expand(&self) {
        self.list.expand_all();
    }

    fn on_collapse(&self) {
        self.list.collapse_all();
    }

    /// `Checkin::OnImport`.
    fn on_import(self: &Rc<Self>) {
        let Some(page) = self.page() else { return };
        let title = tr("Choose a fencer file to import...");
        let mut chooser = FileChooser::new(&title, gtk::FileChooserAction::Open);
        chooser.filters.push(dialogs::file_filter(Some(&tr("Fencer files")), &["*.fff", "*.csv"]));
        chooser.filters.push(dialogs::all_files_filter());
        let app = page.app.upgrade();
        if let Some(app) = &app {
            chooser.current_folder =
                app.tournament.borrow().config.get_string("Checkin", "default_import_dir_name").map(PathBuf::from).filter(|d| d.is_dir());
        }
        let (response, file) = chooser.run();
        if response != gtk::ResponseType::Accept {
            return;
        }
        let Some(file) = file else { return };
        if let Some(app) = &app {
            let mut t = app.tournament.borrow_mut();
            if let Some(dir) = file.parent() {
                t.config.set_string("Checkin", "default_import_dir_name", &dir.to_string_lossy());
            }
            t.add_recent(&file);
        }
        let result = self.edit(|ch, ctx| ch.import(ctx, &file));
        match result {
            Some(Ok(r)) if r.encoding_error => dialogs::info(
                page.window().as_ref(),
                &tr("<b><big>Encoding issue</big></b>"),
                Some(&tr("The imported CSV file is probably based on template generated from another computer.\n\nUse a template generated from this computer!")),
            ),
            Some(Err(e)) => dialogs::error(page.window().as_ref(), &format!("{}: {e}", file.display())),
            _ => {}
        }
    }

    /// `CheckinSupervisor::OnImportRanking`.
    fn on_ranking(self: &Rc<Self>) {
        let Some(page) = self.page() else { return };
        let title = tr("Choose a ranking file to import...");
        let mut chooser = FileChooser::new(&title, gtk::FileChooserAction::Open);
        chooser.filters.push(dialogs::file_filter(Some(&tr("Ranking files")), &["*.txt", "*.xml", "*.cotcot"]));
        chooser.filters.push(dialogs::all_files_filter());
        let (response, file) = chooser.run();
        if response != gtk::ResponseType::Accept {
            return;
        }
        let Some(file) = file else { return };
        match RankImporter::load(&file) {
            Ok(importer) => {
                self.edit(|ch, ctx| ch.import_ranking(ctx, &importer));
            }
            Err(e) => dialogs::error(page.window().as_ref(), &format!("{}: {e}", file.display())),
        }
    }

    /// `SelectAttributes`.
    fn on_filter(self: &Rc<Self>) {
        let Some(page) = self.page() else { return };
        let Some(app) = page.app.upgrade() else { return };
        let window = page.window();
        let changed = {
            let mut t = app.tournament.borrow_mut();
            let mut config = std::mem::take(&mut t.config);
            let changed = t
                .contest_mut(page.id)
                .and_then(|c| c.schedule.checkin_mut())
                .map(|ch| filter_dialog::select_attributes(window.as_ref(), &mut ch.filter, Some(&mut config)))
                .unwrap_or(false);
            t.config = config;
            changed
        };
        if changed {
            page.mark_dirty();
        }
        self.refresh();
    }

    /// `Checkin::OnPrintClicked` : liste des tireurs (ou des équipes) avec leur présence.
    fn on_print(self: &Rc<Self>) {
        let Some(page) = self.page() else { return };
        let document = page
            .with_contest(|c| {
                let team_event = c.settings.team_event;
                let name = c.name.clone();
                let checkin = c.schedule.checkin()?;
                let owner: Owner = checkin.base().owner();
                let specs = render::column_specs(checkin.filter.layout(), false);
                let refs = if team_event { checkin.teams(&c.players) } else { checkin.fencers(&c.players) };
                let rows = list_document::rows_for(&c.players, &refs, &specs, owner);
                Some(ListDocument { title: name, subtitle: tr("Check-in"), specs, rows, classification_borders: false })
            })
            .flatten();
        let Some(document) = document else { return };
        let window = page.window();
        if let Err(e) = document.print(window.as_ref()) {
            dialogs::error(window.as_ref(), &e.to_string());
        }
    }

    /// Tireurs et remplaçants d'une équipe (le score des rencontres en relais suit).
    fn on_team_size(self: &Rc<Self>) {
        if self.updating.get() {
            return;
        }
        let Some(page) = self.page() else { return };
        let (players, subs) = (self.team_players.value_as_int() as u32, self.team_substitutes.value_as_int() as u32);
        if page.with_contest(|c| c.set_team_size(players, subs)).unwrap_or(false) {
            page.mark_dirty();
            if let Some(view) = page.schedule.borrow().clone() {
                view.refresh_sensitivity();
            }
        }
    }

    /// Une équipe ne dépasse pas ses tireurs et remplaçants : message sinon.
    fn team_full(&self, fencer: Option<PlayerRef>, values: &[(String, Option<String>)]) -> bool {
        let Some(team) = values.iter().find(|(code, _)| code == "team").and_then(|(_, v)| v.clone()) else { return false };
        let Some(page) = self.page() else { return false };
        let error = page
            .with_contest(|c| {
                let settings = c.settings.clone();
                c.schedule.checkin().and_then(|ch| ch.team_full_error(&c.players, &settings, fencer, &team))
            })
            .flatten();
        match error {
            Some(message) => {
                dialogs::error(page.window().as_ref(), &message);
                true
            }
            None => false,
        }
    }

    /// `OnConfigChanged` : taille minimale des équipes et classement par défaut.
    fn on_team_config(self: &Rc<Self>) {
        if self.updating.get() {
            return;
        }
        let size = self.glade.get::<gtk::SpinButton>("teamsize_entry").value() as u32;
        let manual = self.glade.get::<gtk::CheckButton>("manual_radiobutton").is_active();
        let worst = self.glade.get::<gtk::Entry>("worst_entry").text().parse::<u32>().ok();
        let Some(page) = self.page() else { return };
        let changed = page
            .with_contest(|c| {
                let before = (c.settings.minimum_team_size, c.settings.manual_classification, c.settings.default_classification);
                c.settings.minimum_team_size = size;
                c.settings.manual_classification = manual;
                if let Some(w) = worst {
                    c.settings.default_classification = w;
                }
                before != (c.settings.minimum_team_size, c.settings.manual_classification, c.settings.default_classification)
            })
            .unwrap_or(false);
        if changed {
            self.edit(|ch, ctx| ch.apply_config(ctx));
        }
    }
}

impl StageScreen for CheckinScreen {
    fn widget(&self) -> gtk::Widget {
        self.root.clone()
    }

    fn refresh(&self) {
        let Some(page) = self.page() else { return };
        if let Some(Some(snapshot)) = page.with_contest(Self::snapshot) {
            self.apply(snapshot);
        }
    }
}
