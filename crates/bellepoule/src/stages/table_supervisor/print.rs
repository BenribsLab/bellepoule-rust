//! Pagination du modèle, aperçu et impressions natives des tableaux.

use std::cell::RefCell;
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
    if set.prepare_print(&mut session, rows, &METRICS, width, height, 60.0) == 0 {
        return None;
    }
    let canvas = Canvas::new();
    scene::draw(&canvas, &set, snapshot, &METRICS, false);
    Some(Pages { canvas, session })
}

fn heading(cr: &cairo::Context, title: &str, page: usize, total: usize, width: f64, height: f64) {
    let canvas = Canvas::new();
    let item = canvas.text(canvas.root(), title, 0.0, 0.0, width, Anchor::NW);
    canvas.set(
        item,
        [
            Prop::Font("Sans Bold 16px".into()),
            Prop::Ellipsize(gtk::pango::EllipsizeMode::End),
        ],
    );
    let footer = canvas.text(
        canvas.root(),
        &format!("Page {}/{}", page + 1, total),
        0.0,
        height - 15.0,
        -1.0,
        Anchor::NW,
    );
    canvas.set(footer, [Prop::Font("Sans 10px".into())]);
    canvas.render(cr, None, 1.0);
}

fn draw_page(
    pages: &Pages,
    snapshot: &Snapshot,
    cr: &cairo::Context,
    page: usize,
    width: f64,
    height: f64,
) {
    heading(
        cr,
        &format!("{} — {}", snapshot.title, snapshot.base.name),
        page,
        pages.session.nb_pages(),
        width,
        height,
    );
    let Some(bounds) = pages.session.page_bounds(page) else {
        return;
    };
    let factor = pages.session.scale();
    if !factor.is_finite() || factor <= 0.0 {
        return;
    }
    let _ = cr.save();
    cr.translate(0.0, 40.0);
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

pub(super) fn tables(parent: Option<&gtk::Window>, snapshot: &Snapshot) -> Result<(), glib::Error> {
    let chooser = Glade::new("table.ui").expect("table.ui");
    let dialog: gtk::MessageDialog = chooser.get("preview_dialog");
    dialog.set_transient_for(parent);
    let area = gtk::DrawingArea::new();
    area.set_content_width(300);
    area.set_content_height(420);
    chooser
        .get::<gtk::ScrolledWindow>("preview_scrolledwindow")
        .set_child(Some(&area));
    let four: gtk::CheckButton = chooser.get("4_radiobutton");
    let eight: gtk::CheckButton = chooser.get("8_radiobutton");
    let sixteen: gtk::CheckButton = chooser.get("16_radiobutton");
    let rows = Rc::new(std::cell::Cell::new(32_u32));
    for (button, value) in [(&four, 8), (&eight, 16), (&sixteen, 32)] {
        let rows = rows.clone();
        let area = area.clone();
        button.connect_toggled(move |b| {
            if b.is_active() {
                rows.set(value);
                area.queue_draw();
            }
        });
    }
    sixteen.set_active(true);
    let source = snapshot.clone();
    let choice = rows.clone();
    area.set_draw_func(move |_, cr, w, h| {
        cr.set_source_rgb(1.0, 1.0, 1.0);
        let _ = cr.paint();
        if let Some(pages) = prepare(&source, choice.get(), 550.0, 780.0) {
            let factor = (w as f64 / 550.0).min(h as f64 / 780.0);
            let _ = cr.save();
            cr.scale(factor, factor);
            draw_page(&pages, &source, cr, 0, 550.0, 780.0);
            let _ = cr.restore();
        }
    });
    let accepted = dialogs::run_dialog(&dialog) == gtk::ResponseType::Other(1);
    dialog.destroy();
    if !accepted {
        return Ok(());
    }
    let pages = Rc::new(RefCell::new(None::<Pages>));
    let operation = gtk::PrintOperation::new();
    operation.set_unit(gtk::Unit::Points);
    operation.set_job_name(&format!("{} — {}", snapshot.title, tr("Table")));
    {
        let pages = pages.clone();
        let snapshot = snapshot.clone();
        operation.connect_begin_print(move |operation, ctx| {
            let p = prepare(&snapshot, rows.get(), ctx.width(), ctx.height());
            operation.set_n_pages(p.as_ref().map(|p| p.session.nb_pages()).unwrap_or(1) as i32);
            *pages.borrow_mut() = p;
        });
    }
    let snapshot = snapshot.clone();
    operation.connect_draw_page(move |_, ctx, page| {
        if let Some(pages) = pages.borrow().as_ref() {
            draw_page(
                pages,
                &snapshot,
                &ctx.cairo_context(),
                page as usize,
                ctx.width(),
                ctx.height(),
            );
        }
    });
    operation.run(gtk::PrintOperationAction::PrintDialog, parent)?;
    Ok(())
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
    let mut compositor = SheetCompositor::new();
    for node in &nodes {
        compositor.add_match(set, *node);
    }
    compositor.compose(false);
    let pages = Rc::new(RefCell::new(Vec::<Vec<NodeId>>::new()));
    let operation = gtk::PrintOperation::new();
    operation.set_job_name(&format!("{} — {}", snapshot.title, tr("Score sheets")));
    operation.set_unit(gtk::Unit::Points);
    {
        let pages = pages.clone();
        operation.connect_begin_print(move |operation, ctx| {
            let per_page = if ctx.width() > ctx.height() { 2 } else { 4 };
            let mut pages = pages.borrow_mut();
            pages.clear();
            // Le compositeur historique groupe quatre feuilles. En paysage,
            // scinder chaque groupe évite de perdre les troisième et quatrième.
            for page in compositor.pages() {
                let matches: Vec<_> = page.matches().collect();
                pages.extend(matches.chunks(per_page).map(|chunk| chunk.to_vec()));
            }
            operation.set_n_pages(pages.len().max(1) as i32);
        });
    }
    let snapshot = snapshot.clone();
    operation.connect_draw_page(move |_, ctx, page| {
        let cr = ctx.cairo_context();
        let pages = pages.borrow();
        let Some(nodes) = pages.get(page as usize) else {
            return;
        };
        heading(
            &cr,
            &snapshot.title,
            page as usize,
            pages.len(),
            ctx.width(),
            ctx.height(),
        );
        let count = if ctx.width() > ctx.height() { 2 } else { 4 };
        let height = (ctx.height() - 55.0) / count as f64;
        for (i, node) in nodes.iter().enumerate() {
            let Some(canvas) = score_sheet(&snapshot, *node) else {
                continue;
            };
            let bounds = canvas.scene_bounds();
            let factor = (ctx.width() / bounds.width().max(1.0))
                .min((height - 10.0) / bounds.height().max(1.0));
            let _ = cr.save();
            cr.translate(0.0, 35.0 + i as f64 * height);
            cr.scale(factor, factor);
            canvas.render(&cr, None, 1.0);
            let _ = cr.restore();
            if i + 1 < nodes.len() {
                cr.set_source_rgb(0.5, 0.5, 0.5);
                cr.set_line_width(0.5);
                cr.set_dash(&[3.0, 3.0], 0.0);
                cr.move_to(0.0, 35.0 + (i + 1) as f64 * height - 5.0);
                cr.line_to(ctx.width(), 35.0 + (i + 1) as f64 * height - 5.0);
                let _ = cr.stroke();
                cr.set_dash(&[], 0.0);
            }
        }
    });
    let result = operation.run(gtk::PrintOperationAction::PrintDialog, parent)?;
    Ok(if result == gtk::PrintOperationResult::Apply {
        nodes
    } else {
        Vec::new()
    })
}

pub(super) fn classification(
    parent: Option<&gtk::Window>,
    snapshot: &Snapshot,
) -> Result<(), glib::Error> {
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
    bp_gtk::print::run_tiled(
        parent,
        &snapshot.title,
        &snapshot.base.name,
        canvas,
        groups,
        1,
        1,
    )
}
