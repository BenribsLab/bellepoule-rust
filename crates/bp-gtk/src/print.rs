//! Contexte d'impression partagé par le canevas et les phases.

use gtk::prelude::*;
use gtk4 as gtk;

use crate::canvas::{Anchor, Canvas, ItemId, Prop};

/// Dimensions d'une page (équivalent de `GtkPrintContext`), en unités de
/// cairo (points d'imprimante).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageContext {
    pub width: f64,
    pub height: f64,
    pub dpi_x: f64,
    pub dpi_y: f64,
}

/// Imprime des blocs cairo sans les couper. `rows = 0` choisit le nombre
/// de lignes selon la hauteur disponible ; sinon la densité est imposée.
pub fn run_tiled(
    parent: Option<&gtk::Window>,
    title: &str,
    subtitle: &str,
    canvas: Canvas,
    groups: Vec<ItemId>,
    rows: usize,
    columns: usize,
) -> Result<(), gtk::glib::Error> {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;
    let columns = columns.max(1);
    let pages = Rc::new(RefCell::new(Vec::<Vec<(ItemId, f64, f64)>>::new()));
    let scale = Rc::new(Cell::new(1.0_f64));
    let operation = gtk::PrintOperation::new();
    operation.set_job_name(title);
    operation.set_unit(gtk::Unit::Points);
    {
        let canvas = canvas.clone();
        let pages = pages.clone();
        let scale = scale.clone();
        operation.connect_begin_print(move |operation, ctx| {
            let width = groups
                .iter()
                .map(|id| canvas.bounds(*id).width())
                .fold(1.0_f64, f64::max);
            let height = groups
                .iter()
                .map(|id| canvas.bounds(*id).height())
                .fold(1.0_f64, f64::max);
            let available_height = (ctx.height() - 85.0).max(1.0);
            let factor = ((ctx.width() - (columns - 1) as f64 * 20.0) / (columns as f64 * width))
                .min(available_height / (height + 20.0) / rows.max(1) as f64)
                .clamp(0.001, 1.0);
            scale.set(factor);
            let rows = if rows == 0 {
                (available_height / ((height + 20.0) * factor))
                    .floor()
                    .max(1.0) as usize
            } else {
                rows
            };
            let capacity = rows * columns;
            let mut pages = pages.borrow_mut();
            pages.clear();
            for chunk in groups.chunks(capacity) {
                pages.push(
                    chunk
                        .iter()
                        .enumerate()
                        .map(|(i, id)| {
                            (
                                *id,
                                (i % columns) as f64 * (ctx.width() / columns as f64),
                                60.0 + (i / columns) as f64 * (height + 20.0) * factor,
                            )
                        })
                        .collect(),
                );
            }
            if pages.is_empty() {
                pages.push(Vec::new());
            }
            operation.set_n_pages(pages.len() as i32);
        });
    }
    {
        let title = title.to_string();
        let subtitle = subtitle.to_string();
        operation.connect_draw_page(move |_, ctx, number| {
            let cr = ctx.cairo_context();
            let heading = Canvas::new();
            let title_item =
                heading.text(heading.root(), &title, 0.0, 0.0, ctx.width(), Anchor::NW);
            heading.set(title_item, [Prop::Font("Sans Bold 16px".into())]);
            let subtitle_item = heading.text(
                heading.root(),
                &subtitle,
                0.0,
                28.0,
                ctx.width(),
                Anchor::NW,
            );
            heading.set(subtitle_item, [Prop::Font("Sans 12px".into())]);
            let footer = heading.text(
                heading.root(),
                &format!("Page {}/{}", number + 1, pages.borrow().len()),
                0.0,
                ctx.height() - 18.0,
                -1.0,
                Anchor::NW,
            );
            heading.set(footer, [Prop::Font("Sans 10px".into())]);
            heading.render(&cr, None, 1.0);
            if let Some(items) = pages.borrow().get(number as usize) {
                for (id, x, y) in items {
                    let bounds = canvas.bounds(*id);
                    let _ = cr.save();
                    cr.translate(*x, *y);
                    cr.scale(scale.get(), scale.get());
                    cr.translate(-bounds.x1, -bounds.y1);
                    canvas.render_item(*id, &cr);
                    let _ = cr.restore();
                }
            }
        });
    }
    operation.run(gtk::PrintOperationAction::PrintDialog, parent)?;
    Ok(())
}

impl PageContext {
    /// Page A4 à 72 ppp.
    pub const A4: PageContext = PageContext {
        width: 595.0,
        height: 842.0,
        dpi_x: 72.0,
        dpi_y: 72.0,
    };

    pub fn new(width: f64, height: f64, dpi_x: f64, dpi_y: f64) -> Self {
        PageContext {
            width,
            height,
            dpi_x,
            dpi_y,
        }
    }

    pub fn from_gtk(ctx: &gtk::PrintContext) -> Self {
        PageContext {
            width: ctx.width(),
            height: ctx.height(),
            dpi_x: ctx.dpi_x(),
            dpi_y: ctx.dpi_y(),
        }
    }
}
