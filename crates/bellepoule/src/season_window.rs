//! Fenêtre « Classement de saison » : cumul des points des compétitions
//! d'un dossier (celles dont les points sont activés), par catégorie, avec
//! les corrections à la main (regrouper, séparer) enregistrées dans le
//! dossier (`saison.json`).

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use bp_core::contest::Contest;
use bp_core::i18n::tr;
use bp_core::points;
use bp_core::season::{self, Overrides, SeasonContest, SeasonRow};
use bp_gtk::dialogs;
use gtk::glib;
use gtk::prelude::*;
use gtk4 as gtk;

use crate::app::App;

struct Season {
    app: std::rc::Weak<App>,
    window: gtk::Window,
    folder_label: gtk::Label,
    folder: RefCell<Option<PathBuf>>,
    contests: RefCell<Vec<SeasonContest>>,
    overrides: RefCell<Overrides>,
    category: gtk::DropDown,
    categories: RefCell<Vec<String>>,
    tree: gtk::TreeView,
    rows: RefCell<Vec<SeasonRow>>,
    status: gtk::Label,
}

/// Ouvre la fenêtre (dossier de la saison mémorisé).
pub fn open(app: &Rc<App>) {
    let window = gtk::Window::new();
    window.set_title(Some(&tr("Season ranking")));
    window.set_transient_for(Some(&app.window));
    window.set_default_size(1150, 720);
    let content = gtk::Box::new(gtk::Orientation::Vertical, 8);
    for f in [gtk::Widget::set_margin_top, gtk::Widget::set_margin_bottom, gtk::Widget::set_margin_start, gtk::Widget::set_margin_end] {
        f(content.upcast_ref(), 12);
    }

    let folder_line = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    folder_line.append(&gtk::Label::new(Some(&tr("Season folder:"))));
    let folder_label = gtk::Label::new(None);
    folder_label.set_hexpand(true);
    folder_label.set_xalign(0.0);
    folder_label.set_ellipsize(gtk::pango::EllipsizeMode::Start);
    folder_line.append(&folder_label);
    let choose = gtk::Button::with_label(&tr("Choose…"));
    let reload = gtk::Button::from_icon_name("view-refresh-symbolic");
    reload.set_tooltip_text(Some(&tr("Reload")));
    folder_line.append(&choose);
    folder_line.append(&reload);
    content.append(&folder_line);

    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    actions.append(&gtk::Label::new(Some(&tr("Category"))));
    let category = gtk::DropDown::from_strings(&[]);
    bp_gtk::util::scroll_passes_through(&category);
    actions.append(&category);
    let merge = gtk::Button::with_label(&tr("Group the selection"));
    merge.set_tooltip_text(Some(&tr("The selected lines are one and the same fencer")));
    let split = gtk::Button::with_label(&tr("Separate this first name"));
    split.set_tooltip_text(Some(&tr("Fencers with this first name are told apart by last name and category")));
    let reset = gtk::Button::with_label(&tr("Undo the corrections"));
    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    let export = gtk::Button::with_label(&tr("Export (CSV)"));
    let print = gtk::Button::with_label(&tr("Print"));
    for w in [merge.upcast_ref::<gtk::Widget>(), split.upcast_ref(), reset.upcast_ref(), spacer.upcast_ref(), export.upcast_ref(), print.upcast_ref()] {
        actions.append(w);
    }
    content.append(&actions);

    let tree = gtk::TreeView::new();
    tree.selection().set_mode(gtk::SelectionMode::Multiple);
    let scrolled = gtk::ScrolledWindow::new();
    scrolled.set_vexpand(true);
    scrolled.set_child(Some(&tree));
    content.append(&scrolled);
    let status = gtk::Label::new(None);
    status.set_xalign(0.0);
    status.add_css_class("bp-muted");
    content.append(&status);
    window.set_child(Some(&content));

    let folder = app.tournament.borrow().config.get_string("Season", "folder").map(PathBuf::from).filter(|p| p.is_dir());
    let season = Rc::new(Season {
        app: Rc::downgrade(app),
        window: window.clone(),
        folder_label,
        folder: RefCell::new(folder),
        contests: RefCell::new(Vec::new()),
        overrides: RefCell::new(Overrides::default()),
        category,
        categories: RefCell::new(Vec::new()),
        tree,
        rows: RefCell::new(Vec::new()),
        status,
    });
    let s = season.clone();
    choose.connect_clicked(move |_| s.choose_folder());
    let s = season.clone();
    reload.connect_clicked(move |_| s.load());
    let s = season.clone();
    season.category.connect_selected_notify(move |_| s.show());
    let s = season.clone();
    merge.connect_clicked(move |_| s.on_merge());
    let s = season.clone();
    split.connect_clicked(move |_| s.on_split());
    let s = season.clone();
    reset.connect_clicked(move |_| {
        if dialogs::confirm(Some(&s.window), &tr("Undo every manual correction?"), None, None) {
            *s.overrides.borrow_mut() = Overrides::default();
            s.save_overrides();
            s.show();
        }
    });
    let s = season.clone();
    export.connect_clicked(move |_| s.on_export());
    let s = season.clone();
    print.connect_clicked(move |_| s.on_print());
    window.present();
    season.load();
}

/// Fichiers de compétition d'un dossier (et de ses sous-dossiers directs).
fn competition_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut dirs = vec![(dir.to_path_buf(), 0)];
    while let Some((d, depth)) = dirs.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else { continue };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() && depth < 2 {
                dirs.push((p, depth + 1));
            } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("cotcot")) {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

impl Season {
    fn overrides_path(&self) -> Option<PathBuf> {
        self.folder.borrow().as_ref().map(|d| d.join("saison.json"))
    }

    fn choose_folder(&self) {
        let title = tr("Season folder");
        let mut chooser = dialogs::FileChooser::new(&title, gtk::FileChooserAction::SelectFolder);
        chooser.current_folder = self.folder.borrow().clone();
        let (response, path) = chooser.run();
        if response != gtk::ResponseType::Accept {
            return;
        }
        let Some(path) = path else { return };
        if let Some(app) = self.app.upgrade() {
            let mut t = app.tournament.borrow_mut();
            t.config.set_string("Season", "folder", &path.to_string_lossy());
            let _ = t.save_config();
        }
        *self.folder.borrow_mut() = Some(path);
        self.load();
    }

    /// Relit les compétitions du dossier et les corrections.
    fn load(&self) {
        let Some(dir) = self.folder.borrow().clone() else {
            self.folder_label.set_text(&tr("(choose the folder holding the season's competitions)"));
            self.status.set_text("");
            return;
        };
        self.folder_label.set_text(&dir.to_string_lossy());
        let mut contests = Vec::new();
        for path in competition_files(&dir) {
            let mut c = Contest::new(None, false);
            if c.load_file(&path).is_ok() {
                if let Some(s) = SeasonContest::from_contest(&path.to_string_lossy(), &c) {
                    contests.push(s);
                }
            }
        }
        contests.sort_by(|a, b| date_key(&a.date).cmp(&date_key(&b.date)).then(a.name.cmp(&b.name)));
        *self.overrides.borrow_mut() = self
            .overrides_path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        let mut categories: Vec<String> = contests.iter().map(|c| c.category.clone()).filter(|c| !c.is_empty()).collect();
        categories.sort();
        categories.dedup();
        let mut labels = vec![tr("All")];
        labels.extend(categories.iter().cloned());
        let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
        *self.contests.borrow_mut() = contests;
        *self.categories.borrow_mut() = categories;
        self.category.set_model(Some(&gtk::StringList::new(&refs)));
        self.category.set_selected(0);
        self.show();
    }

    fn selected_contests(&self) -> Vec<SeasonContest> {
        let i = self.category.selected() as usize;
        let filter = i.checked_sub(1).and_then(|i| self.categories.borrow().get(i).cloned());
        self.contests.borrow().iter().filter(|c| filter.as_ref().is_none_or(|f| c.category == *f)).cloned().collect()
    }

    /// Recalcule et affiche le classement.
    fn show(&self) {
        let contests = self.selected_contests();
        let rows = season::ranking(&contests, &self.overrides.borrow());
        while let Some(c) = self.tree.column(0) {
            self.tree.remove_column(&c);
        }
        let mut titles = vec![tr("Rank"), tr("First name"), tr("Last name"), tr("Category"), tr("Competitions"), tr("Total")];
        titles.extend(contests.iter().map(|c| if c.date.is_empty() { c.name.clone() } else { format!("{}\n{}", c.name, c.date) }));
        let types: Vec<glib::Type> = titles.iter().map(|_| glib::Type::STRING).collect();
        let store = gtk::ListStore::new(&types);
        for (i, title) in titles.iter().enumerate() {
            let renderer = gtk::CellRendererText::new();
            renderer.set_padding(8, 4);
            if i == 5 {
                renderer.set_weight(700);
            }
            let column = gtk::TreeViewColumn::new();
            column.set_title(title);
            column.set_resizable(true);
            column.pack_start(&renderer, true);
            column.add_attribute(&renderer, "text", i as i32);
            self.tree.append_column(&column);
        }
        for row in &rows {
            let iter = store.append();
            let mut values: Vec<String> = vec![
                row.rank.to_string(),
                row.first_name.clone(),
                row.last_name.clone(),
                row.category.clone(),
                row.competitions.to_string(),
                points::image(row.total),
            ];
            values.extend(row.points.iter().map(|p| p.map(points::image).unwrap_or_default()));
            for (col, v) in values.iter().enumerate() {
                store.set_value(&iter, col as u32, &v.to_value());
            }
        }
        self.tree.set_model(Some(&store));
        let corrections = { let o = self.overrides.borrow(); o.merges.len() + o.splits.len() };
        self.status.set_text(&format!(
            "{} {} · {} {}{}",
            contests.len(),
            tr("competition(s) with points"),
            rows.len(),
            tr("fencer(s)"),
            if corrections > 0 { format!(" · {corrections} {}", tr("manual correction(s)")) } else { String::new() }
        ));
        *self.rows.borrow_mut() = rows;
    }

    fn selected_rows(&self) -> Vec<SeasonRow> {
        let (paths, _) = self.tree.selection().selected_rows();
        let rows = self.rows.borrow();
        paths.iter().filter_map(|p| p.indices().first().and_then(|i| rows.get(*i as usize).cloned())).collect()
    }

    fn save_overrides(&self) {
        if let Some(path) = self.overrides_path() {
            if let Err(e) = std::fs::write(&path, serde_json::to_string_pretty(&*self.overrides.borrow()).unwrap_or_default()) {
                dialogs::error(Some(&self.window), &format!("{}: {e}", path.display()));
            }
        }
    }

    fn on_merge(&self) {
        let selected = self.selected_rows();
        if selected.len() < 2 {
            dialogs::error(Some(&self.window), &tr("Select at least two lines (Ctrl + click)."));
            return;
        }
        let into = selected[0].key.clone();
        let others: Vec<String> = selected[1..].iter().map(|r| r.key.clone()).collect();
        self.overrides.borrow_mut().merge(&into, &others);
        self.save_overrides();
        self.show();
    }

    fn on_split(&self) {
        let Some(row) = self.selected_rows().into_iter().next() else { return };
        self.overrides.borrow_mut().split(&row.first_name);
        self.save_overrides();
        self.show();
    }

    /// Tableau (CSV, séparateur « ; ») du classement affiché.
    fn csv(&self) -> String {
        let contests = self.selected_contests();
        let mut out = String::new();
        let mut head = vec![tr("Rank"), tr("First name"), tr("Last name"), tr("Category"), tr("Competitions"), tr("Total")];
        head.extend(contests.iter().map(|c| format!("{} {}", c.name, c.date).trim().to_string()));
        out.push_str(&head.join(";"));
        out.push('\n');
        for r in self.rows.borrow().iter() {
            let mut line = vec![r.rank.to_string(), r.first_name.clone(), r.last_name.clone(), r.category.clone(), r.competitions.to_string(), points::image(r.total)];
            line.extend(r.points.iter().map(|p| p.map(points::image).unwrap_or_default()));
            out.push_str(&line.iter().map(|v| v.replace(';', ",")).collect::<Vec<_>>().join(";"));
            out.push('\n');
        }
        out
    }

    fn on_export(&self) {
        let title = tr("Export (CSV)");
        let mut chooser = dialogs::FileChooser::new(&title, gtk::FileChooserAction::Save);
        chooser.current_folder = self.folder.borrow().clone();
        chooser.current_name = Some(format!("{}.csv", tr("season-ranking")));
        let (response, path) = chooser.run();
        let Some(path) = path.filter(|_| response == gtk::ResponseType::Accept) else { return };
        // BOM : accents lisibles dans un tableur.
        if let Err(e) = std::fs::write(&path, format!("\u{feff}{}", self.csv())) {
            dialogs::error(Some(&self.window), &e.to_string());
        }
    }

    /// Impression : page HTML ouverte dans le navigateur.
    fn on_print(&self) {
        let contests = self.selected_contests();
        let esc = |s: &str| glib::markup_escape_text(s).to_string();
        let mut html = format!(
            "<!doctype html><html><head><meta charset=\"utf-8\"><title>{t}</title><style>body{{font-family:sans-serif}}table{{border-collapse:collapse}}td,th{{border:1px solid #999;padding:4px 8px;text-align:left}}td.n{{text-align:right}}</style></head><body><h1>{t}</h1><table><tr>",
            t = esc(&tr("Season ranking"))
        );
        let mut head = vec![tr("Rank"), tr("First name"), tr("Last name"), tr("Category"), tr("Competitions"), tr("Total")];
        head.extend(contests.iter().map(|c| format!("{} {}", c.name, c.date)));
        for h in head {
            html.push_str(&format!("<th>{}</th>", esc(&h)));
        }
        html.push_str("</tr>");
        for r in self.rows.borrow().iter() {
            html.push_str(&format!(
                "<tr><td class=n>{}</td><td>{}</td><td>{}</td><td>{}</td><td class=n>{}</td><td class=n><b>{}</b></td>",
                r.rank,
                esc(&r.first_name),
                esc(&r.last_name),
                esc(&r.category),
                r.competitions,
                points::image(r.total)
            ));
            for p in &r.points {
                html.push_str(&format!("<td class=n>{}</td>", p.map(points::image).unwrap_or_default()));
            }
            html.push_str("</tr>");
        }
        html.push_str("</table><script>window.print()</script></body></html>");
        let path = std::env::temp_dir().join("managecomp-saison.html");
        match std::fs::write(&path, html) {
            Ok(()) => dialogs::show_uri(&gtk::gio::File::for_path(&path).uri()),
            Err(e) => dialogs::error(Some(&self.window), &e.to_string()),
        }
    }
}

/// Date « jj/mm/aaaa » comparable.
fn date_key(date: &str) -> String {
    let parts: Vec<&str> = date.split(['/', '.', '-']).collect();
    match parts[..] {
        [d, m, y] if y.len() == 4 => format!("{y}{m:0>2}{d:0>2}"),
        _ => date.to_string(),
    }
}
