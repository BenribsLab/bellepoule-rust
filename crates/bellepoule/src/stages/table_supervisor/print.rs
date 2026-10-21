//! Pagination du modèle, aperçu et impressions natives des tableaux.

use std::rc::Rc;

use bp_core::i18n::tr;
use bp_core::rounds::table::{NodeId, PrintSession, PrintType, SheetCompositor};
use bp_gtk::{
    canvas::{Anchor, Bounds, Canvas, Prop},
    dialogs,
    glade::Glade,
};
use gtk::prelude::*;
use gtk4 as gtk;

use super::{scene, Snapshot, METRICS};

struct Pages {
    canvas: Canvas,
    session: PrintSession,
}

fn prepare(snapshot: &Snapshot, rows: u32, width: f64, height: f64) -> Option<Pages> {
    let mut set = snapshot.set.clone()?;
    if set.tables().is_empty() || set.table(set.from_table()).size() <= 1 {
        return None;
    }
    let mut session = PrintSession::new(
        PrintType::DisplayedTables,
        &snapshot.base.name,
        Some(&set),
        None,
    );
    if set.prepare_print(&mut session, rows, &METRICS, width, height, 0.0) == 0 {
        return None;
    }
    let canvas = Canvas::new();
    scene::draw(&canvas, &set, snapshot, &METRICS, false);
    Some(Pages { canvas, session })
}

fn draw_page(
    pages: &Pages,
    _snapshot: &Snapshot,
    cr: &cairo::Context,
    page: usize,
    width: f64,
    _height: f64,
) {
    let Some(bounds) = pages.session.page_bounds(page) else {
        return;
    };
    let factor = pages.session.scale();
    if !factor.is_finite() || factor <= 0.0 {
        return;
    }
    let _ = cr.save();
    cr.scale(factor, factor);
    // Répéter les noms des tableaux sur chaque page.
    let _ = cr.save();
    cr.rectangle(0.0, 0.0, width / factor, METRICS.header_height);
    cr.clip();
    pages.canvas.render(
        cr,
        Some(&Bounds::new(
            0.0,
            0.0,
            width / factor,
            METRICS.header_height,
        )),
        1.0,
    );
    let _ = cr.restore();
    cr.translate(0.0, METRICS.header_height - bounds.y1);
    let body = Bounds::new(0.0, bounds.y1, width / factor, bounds.y2);
    cr.rectangle(body.x1, body.y1, body.width(), body.height());
    cr.clip();
    pages.canvas.render(cr, Some(&body), 1.0);
    let _ = cr.restore();
}

/// Les tableaux affichés, paginés.
pub(super) struct TablesDoc {
    snapshot: Snapshot,
    rows: u32,
    pages: Option<Pages>,
}

impl TablesDoc {
    pub(super) fn new(snapshot: Snapshot, rows: u32) -> Self {
        TablesDoc { snapshot, rows, pages: None }
    }
}

impl bp_gtk::print::PageSource for TablesDoc {
    fn page_name(&self, _page: usize) -> String {
        self.snapshot.base.name.clone()
    }

    fn begin(&mut self, width: f64, height: f64) -> usize {
        self.pages = prepare(&self.snapshot, self.rows, width, height);
        self.pages.as_ref().map(|p| p.session.nb_pages()).unwrap_or(1).max(1)
    }

    fn draw(&mut self, page: usize, cr: &cairo::Context, width: f64, height: f64) {
        if let Some(pages) = self.pages.as_ref() {
            draw_page(pages, &self.snapshot, cr, page, width, height);
        }
    }
}

fn score_sheet(snapshot: &Snapshot, node: NodeId) -> Option<Canvas> {
    let set = snapshot.set.as_ref()?;
    let sheet = set.score_sheet_with_referees(
        node,
        &snapshot.players,
        &snapshot.referees,
        snapshot.contest_net_id,
        snapshot.base.net_id,
    )?;
    let canvas = Canvas::new();
    let root = canvas.root();
    let heading = canvas.text(
        root,
        &format!("{} — {}", snapshot.base.name, sheet.name),
        0.0,
        0.0,
        680.0,
        Anchor::NW,
    );
    canvas.set(heading, [Prop::Font("Sans Bold 20px".into())]);
    let details = format!(
        "{}: {} {}     {}: {}",
        sheet.referee_label,
        sheet
            .referee
            .as_ref()
            .map(|r| r.0.as_str())
            .unwrap_or("________"),
        sheet.referee.as_ref().map(|r| r.1.as_str()).unwrap_or(""),
        sheet.piste_label,
        sheet.piste.as_deref().unwrap_or("________")
    );
    canvas.text(root, &details, 0.0, 32.0, 680.0, Anchor::NW);
    for (i, player) in sheet.fencers.into_iter().enumerate() {
        let Some(player) = player else { continue };
        let y = 75.0 + i as f64 * 62.0;
        let identity = scene::identity(
            &snapshot.players,
            player,
            &snapshot.base.filter,
            snapshot.base.owner(),
            false,
        );
        let name = canvas.text(root, &identity, 0.0, y, 500.0, Anchor::NW);
        canvas.set(
            name,
            [
                Prop::Font("Sans Bold 16px".into()),
                Prop::Ellipsize(gtk::pango::EllipsizeMode::End),
            ],
        );
        canvas.text(root, "____", 515.0, y, -1.0, Anchor::NW);
        let boxes = sheet
            .score_boxes
            .map(|n| {
                (1..=n)
                    .map(|n| format!("□{n}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default();
        let hits = canvas.text(root, &boxes, 0.0, y + 25.0, 540.0, Anchor::NW);
        canvas.set(hits, [Prop::Font("Sans 11px".into())]);
    }
    let signature = canvas.text(
        root,
        &format!("{}: ____________________", sheet.signature_label),
        0.0,
        205.0,
        -1.0,
        Anchor::NW,
    );
    canvas.set(signature, [Prop::Font("Sans 12px".into())]);
    if let Some(reference) = sheet.flash_ref {
        let label = canvas.text(root, &reference, 0.0, 230.0, -1.0, Anchor::NW);
        canvas.set(label, [Prop::Font("Sans 10px".into())]);
    }
    Some(canvas)
}

pub(super) fn score_sheets(
    parent: Option<&gtk::Window>,
    snapshot: &Snapshot,
    table: Option<usize>,
    node: Option<NodeId>,
) -> Result<Vec<NodeId>, glib::Error> {
    let Some(set) = snapshot.set.as_ref() else {
        return Ok(Vec::new());
    };
    let all = if node.is_some() {
        true
    } else {
        let chooser = Glade::new("table.ui").expect("table.ui");
        let dialog: gtk::MessageDialog = chooser.get("score_sheet_dialog");
        dialog.set_transient_for(parent);
        let accepted = dialogs::run_dialog(&dialog) == gtk::ResponseType::Ok;
        let all = chooser
            .get::<gtk::CheckButton>("table_all_radiobutton")
            .is_active();
        dialog.destroy();
        if !accepted {
            return Ok(Vec::new());
        }
        all
    };
    let nodes = node
        .map(|n| vec![n])
        .unwrap_or_else(|| set.matches_to_print(table, all));
    if nodes.is_empty() {
        return Ok(Vec::new());
    }
    let doc_nodes = nodes.clone();
    let doc_snapshot = snapshot.clone();
    let job = format!("{} — {}", snapshot.title, tr("Score sheets"));
    let printed = bp_gtk::print::run_document(parent, &job, move || {
        Box::new(ScoreSheetsDoc { snapshot: doc_snapshot.clone(), nodes: doc_nodes.clone(), pages: Vec::new() })
    })?;
    Ok(if printed { nodes } else { Vec::new() })
}

/// Feuilles de match (4 par page en portrait, 2 en paysage).
pub(super) struct ScoreSheetsDoc {
    pub(super) snapshot: Snapshot,
    pub(super) nodes: Vec<NodeId>,
    pages: Vec<Vec<NodeId>>,
}

impl ScoreSheetsDoc {
    pub(super) fn new(snapshot: Snapshot, nodes: Vec<NodeId>) -> Self {
        ScoreSheetsDoc { snapshot, nodes, pages: Vec::new() }
    }
}

impl bp_gtk::print::PageSource for ScoreSheetsDoc {
    fn page_name(&self, _page: usize) -> String {
        format!("{} - {}", self.snapshot.base.name, tr("Score sheets"))
    }

    fn begin(&mut self, width: f64, height: f64) -> usize {
        self.pages.clear();
        let Some(set) = self.snapshot.set.as_ref() else { return 1 };
        let mut compositor = SheetCompositor::new();
        for node in &self.nodes {
            compositor.add_match(set, *node);
        }
        compositor.compose(false);
        let per_page = if width > height { 2 } else { 4 };
        // Le compositeur historique groupe quatre feuilles. En paysage,
        // scinder chaque groupe évite de perdre les troisième et quatrième.
        for page in compositor.pages() {
            let matches: Vec<_> = page.matches().collect();
            self.pages.extend(matches.chunks(per_page).map(|chunk| chunk.to_vec()));
        }
        self.pages.len().max(1)
    }

    fn draw(&mut self, page: usize, cr: &cairo::Context, width: f64, height: f64) {
        let Some(nodes) = self.pages.get(page) else { return };
        let count = if width > height { 2 } else { 4 };
        let sheet_height = height / count as f64;
        for (i, node) in nodes.iter().enumerate() {
            let Some(canvas) = score_sheet(&self.snapshot, *node) else { continue };
            let bounds = canvas.scene_bounds();
            let factor = (width / bounds.width().max(1.0)).min((sheet_height - 10.0) / bounds.height().max(1.0));
            let _ = cr.save();
            cr.translate(0.0, i as f64 * sheet_height);
            cr.scale(factor, factor);
            canvas.render(cr, None, 1.0);
            let _ = cr.restore();
            if i + 1 < nodes.len() {
                cr.set_source_rgb(0.5, 0.5, 0.5);
                cr.set_line_width(0.5);
                cr.set_dash(&[3.0, 3.0], 0.0);
                cr.move_to(0.0, (i + 1) as f64 * sheet_height - 5.0);
                cr.line_to(width, (i + 1) as f64 * sheet_height - 5.0);
                let _ = cr.stroke();
                cr.set_dash(&[], 0.0);
            }
        }
    }
}

pub(super) fn classification(
    parent: Option<&gtk::Window>,
    snapshot: &Snapshot,
) -> Result<(), glib::Error> {
    let job = snapshot.title.clone();
    let snapshot = snapshot.clone();
    bp_gtk::print::run_document(parent, &job, move || classification_source(&snapshot))?;
    Ok(())
}

/// Classement des tableaux, paginé par 25 lignes.
pub(super) fn classification_source(snapshot: &Snapshot) -> Box<dyn bp_gtk::print::PageSource> {
    let canvas = Canvas::new();
    let mut groups = Vec::new();
    for rows in snapshot.rows.chunks(25) {
        let table = canvas.table(canvas.root());
        canvas.set(table, [Prop::ColumnSpacing(12.0), Prop::RowSpacing(4.0)]);
        for (i, spec) in snapshot.specs.iter().enumerate() {
            let header = canvas.put_text_in_table(table, &spec.title, 0, i as u32);
            canvas.set(header, [Prop::Font("Sans Bold 14px".into())]);
        }
        for (row, data) in rows.iter().enumerate() {
            for (col, spec) in snapshot.specs.iter().enumerate() {
                let text = snapshot
                    .players
                    .get(data.player)
                    .and_then(|p| {
                        crate::widgets::render::value_of(p, &spec.code, snapshot.base.owner())
                    })
                    .and_then(|v| {
                        bp_core::attribute::registry().by_code(&spec.code).map(|d| {
                            crate::widgets::render::user_image(
                                d,
                                v,
                                bp_core::attribute::Look::LongText,
                            )
                        })
                    })
                    .unwrap_or_default();
                canvas.put_text_in_table(table, &text, row as u32 + 1, col as u32);
            }
        }
        groups.push(table);
    }
    Box::new(bp_gtk::print::Tiled::new(&snapshot.title, &snapshot.base.name, canvas, groups, 1, 1))
}

/// Bouton « Imprimer » : au choix le tableau, les feuilles de match, ou les
/// deux. Renvoie les matchs dont la feuille a été imprimée.
pub(super) fn choose_and_print(
    parent: Option<&gtk::Window>,
    snapshot: &Snapshot,
) -> Result<Vec<NodeId>, glib::Error> {
    let Some(set) = snapshot.set.as_ref() else {
        return Ok(Vec::new());
    };
    let dialog = gtk::Dialog::new();
    dialog.set_title(Some(&tr("Print")));
    dialog.set_transient_for(parent);
    dialog.set_resizable(false);
    let content = dialog.content_area();
    content.set_spacing(6);
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    content.set_margin_start(12);
    content.set_margin_end(12);

    let table_check = gtk::CheckButton::with_label(&tr("The table"));
    table_check.set_active(true);
    content.append(&table_check);
    let table_options = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    table_options.set_margin_start(24);
    table_options.append(&gtk::Label::new(Some(&tr("Matches per page"))));
    let rows = Rc::new(std::cell::Cell::new(32_u32));
    let mut group: Option<gtk::CheckButton> = None;
    for (label, value) in [("4", 8_u32), ("8", 16), ("16", 32)] {
        let radio = gtk::CheckButton::with_label(label);
        if let Some(first) = group.as_ref() {
            radio.set_group(Some(first));
        } else {
            group = Some(radio.clone());
        }
        radio.set_active(value == 32);
        let rows = rows.clone();
        radio.connect_toggled(move |b| {
            if b.is_active() {
                rows.set(value);
            }
        });
        table_options.append(&radio);
    }
    content.append(&table_options);

    let sheets_check = gtk::CheckButton::with_label(&tr("The score sheets"));
    content.append(&sheets_check);
    let sheet_options = gtk::Box::new(gtk::Orientation::Vertical, 2);
    sheet_options.set_margin_start(24);
    let all_radio = gtk::CheckButton::with_label(&tr("All the score sheets"));
    let pending_radio = gtk::CheckButton::with_label(&tr("The pending score sheets"));
    pending_radio.set_group(Some(&all_radio));
    pending_radio.set_active(true);
    sheet_options.append(&all_radio);
    sheet_options.append(&pending_radio);
    sheet_options.set_sensitive(false);
    content.append(&sheet_options);
    {
        let table_options = table_options.clone();
        table_check.connect_toggled(move |b| table_options.set_sensitive(b.is_active()));
        let sheet_options = sheet_options.clone();
        sheets_check.connect_toggled(move |b| sheet_options.set_sensitive(b.is_active()));
    }
    dialog.add_button(&tr("_Cancel"), gtk::ResponseType::Cancel);
    let ok = dialog.add_button(&tr("_OK"), gtk::ResponseType::Ok);
    ok.add_css_class("suggested-action");
    let update_ok: Rc<dyn Fn()> = {
        let ok = ok.clone();
        let table_check = table_check.clone();
        let sheets_check = sheets_check.clone();
        Rc::new(move || ok.set_sensitive(table_check.is_active() || sheets_check.is_active()))
    };
    {
        let f = update_ok.clone();
        table_check.connect_toggled(move |_| f());
        let f = update_ok.clone();
        sheets_check.connect_toggled(move |_| f());
    }
    let accepted = dialogs::run_dialog(&dialog) == gtk::ResponseType::Ok;
    let want_table = table_check.is_active();
    let want_sheets = sheets_check.is_active();
    let all = all_radio.is_active();
    dialog.destroy();
    if !accepted || !(want_table || want_sheets) {
        return Ok(Vec::new());
    }

    let nodes = if want_sheets { set.matches_to_print(None, all) } else { Vec::new() };
    if !want_table && nodes.is_empty() {
        return Ok(Vec::new());
    }
    let job = format!("{} — {}", snapshot.title, snapshot.base.name);
    let snapshot = snapshot.clone();
    let factory_nodes = nodes.clone();
    let printed = bp_gtk::print::run_document(parent, &job, move || {
        let mut parts: Vec<Box<dyn bp_gtk::print::PageSource>> = Vec::new();
        if want_table {
            parts.push(Box::new(TablesDoc::new(snapshot.clone(), rows.get())));
        }
        if !factory_nodes.is_empty() {
            parts.push(Box::new(ScoreSheetsDoc::new(snapshot.clone(), factory_nodes.clone())));
        }
        Box::new(bp_gtk::print::Concat::new(parts))
    })?;
    Ok(if printed { nodes } else { Vec::new() })
}
