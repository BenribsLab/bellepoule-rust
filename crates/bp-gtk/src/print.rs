//! Contexte d'impression partagé par le canevas et les phases.

#![allow(deprecated)]

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

/// Source de pages : calcule sa pagination pour un format de page puis dessine
/// chaque page. Sert à l'aperçu comme à l'impression.
pub trait PageSource {
    /// Prépare la pagination pour une page de `width` × `height` points et
    /// renvoie le nombre de pages.
    fn begin(&mut self, width: f64, height: f64) -> usize;
    fn draw(&mut self, page: usize, cr: &cairo::Context, width: f64, height: f64);
}

/// Plusieurs sources imprimées l'une après l'autre (impression complète d'une
/// compétition).
pub struct Concat {
    parts: Vec<Box<dyn PageSource>>,
    counts: Vec<usize>,
}

impl Concat {
    pub fn new(parts: Vec<Box<dyn PageSource>>) -> Self {
        Concat { parts, counts: Vec::new() }
    }
}

impl PageSource for Concat {
    fn begin(&mut self, width: f64, height: f64) -> usize {
        self.counts = self.parts.iter_mut().map(|p| p.begin(width, height)).collect();
        self.counts.iter().sum()
    }

    fn draw(&mut self, mut page: usize, cr: &cairo::Context, width: f64, height: f64) {
        for (part, count) in self.parts.iter_mut().zip(self.counts.iter()) {
            if page < *count {
                part.draw(page, cr, width, height);
                return;
            }
            page -= *count;
        }
    }
}

/// Aperçu puis impression d'un document. Renvoie vrai si l'impression a
/// bien été lancée (`make` est rappelé pour chaque usage : aperçu, impression).
pub fn run_document(
    parent: Option<&gtk::Window>,
    job_name: &str,
    make: impl Fn() -> Box<dyn PageSource> + 'static,
) -> Result<bool, gtk::glib::Error> {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    let make: Rc<dyn Fn() -> Box<dyn PageSource>> = Rc::new(make);

    // Aperçu
    let setup = gtk::PageSetup::new();
    let (paper_w, paper_h) = (setup.paper_width(gtk::Unit::Points), setup.paper_height(gtk::Unit::Points));
    let (left, top) = (setup.left_margin(gtk::Unit::Points), setup.top_margin(gtk::Unit::Points));
    let (page_w, page_h) = (setup.page_width(gtk::Unit::Points), setup.page_height(gtk::Unit::Points));
    let source = Rc::new(RefCell::new(make()));
    let count = source.borrow_mut().begin(page_w, page_h).max(1);
    let current = Rc::new(Cell::new(0_usize));

    let dialog = gtk::Dialog::new();
    dialog.set_title(Some(job_name));
    dialog.set_transient_for(parent);
    dialog.set_default_size(620, 760);
    let area = gtk::DrawingArea::new();
    area.set_hexpand(true);
    area.set_vexpand(true);
    {
        let source = source.clone();
        let current = current.clone();
        area.set_draw_func(move |_, cr, w, h| {
            cr.set_source_rgb(0.55, 0.55, 0.55);
            let _ = cr.paint();
            let factor = ((w as f64 - 20.0) / paper_w).min((h as f64 - 20.0) / paper_h).max(0.01);
            let _ = cr.save();
            cr.translate((w as f64 - paper_w * factor) / 2.0, (h as f64 - paper_h * factor) / 2.0);
            cr.scale(factor, factor);
            cr.set_source_rgb(1.0, 1.0, 1.0);
            cr.rectangle(0.0, 0.0, paper_w, paper_h);
            let _ = cr.fill();
            cr.rectangle(left, top, page_w, page_h);
            cr.clip();
            cr.translate(left, top);
            cr.set_source_rgb(0.0, 0.0, 0.0);
            source.borrow_mut().draw(current.get(), cr, page_w, page_h);
            let _ = cr.restore();
        });
    }
    let bar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    bar.set_margin_top(6);
    bar.set_margin_bottom(6);
    bar.set_halign(gtk::Align::Center);
    let previous = gtk::Button::from_icon_name("go-previous-symbolic");
    let next = gtk::Button::from_icon_name("go-next-symbolic");
    let label = gtk::Label::new(None);
    let update = {
        let label = label.clone();
        let previous = previous.clone();
        let next = next.clone();
        let area = area.clone();
        let current = current.clone();
        Rc::new(move || {
            label.set_text(&format!("{} / {}", current.get() + 1, count));
            previous.set_sensitive(current.get() > 0);
            next.set_sensitive(current.get() + 1 < count);
            area.queue_draw();
        })
    };
    {
        let current = current.clone();
        let update = update.clone();
        previous.connect_clicked(move |_| {
            current.set(current.get().saturating_sub(1));
            update();
        });
    }
    {
        let current = current.clone();
        let update = update.clone();
        next.connect_clicked(move |_| {
            current.set((current.get() + 1).min(count - 1));
            update();
        });
    }
    bar.append(&previous);
    bar.append(&label);
    bar.append(&next);
    let content = dialog.content_area();
    content.append(&bar);
    content.append(&area);
    dialog.add_button(&bp_core::i18n::tr("_Close"), gtk::ResponseType::Cancel);
    dialog.add_button(&bp_core::i18n::tr("_Print"), gtk::ResponseType::Accept);
    update();
    let response = crate::dialogs::run_dialog(&dialog);
    dialog.destroy();
    if response != gtk::ResponseType::Accept {
        return Ok(false);
    }

    // Impression
    let printing: Rc<RefCell<Option<Box<dyn PageSource>>>> = Rc::new(RefCell::new(None));
    let operation = gtk::PrintOperation::new();
    operation.set_job_name(job_name);
    operation.set_unit(gtk::Unit::Points);
    {
        let printing = printing.clone();
        let make = make.clone();
        operation.connect_begin_print(move |operation, ctx| {
            let mut source = make();
            let pages = source.begin(ctx.width(), ctx.height()).max(1);
            operation.set_n_pages(pages as i32);
            *printing.borrow_mut() = Some(source);
        });
    }
    operation.connect_draw_page(move |_, ctx, number| {
        if let Some(source) = printing.borrow_mut().as_mut() {
            source.draw(number as usize, &ctx.cairo_context(), ctx.width(), ctx.height());
        }
    });
    let result = operation.run(gtk::PrintOperationAction::PrintDialog, parent)?;
    Ok(result == gtk::PrintOperationResult::Apply)
}

/// Blocs cairo imprimés sans être coupés, avec titre et numéro de page.
pub struct Tiled {
    pub title: String,
    pub subtitle: String,
    pub canvas: Canvas,
    pub groups: Vec<ItemId>,
    pub rows: usize,
    pub columns: usize,
    pages: Vec<Vec<(ItemId, f64, f64)>>,
    scale: f64,
}

impl Tiled {
    /// `rows = 0` choisit le nombre de lignes selon la hauteur disponible ;
    /// sinon la densité est imposée.
    pub fn new(title: &str, subtitle: &str, canvas: Canvas, groups: Vec<ItemId>, rows: usize, columns: usize) -> Self {
        Tiled {
            title: title.to_string(),
            subtitle: subtitle.to_string(),
            canvas,
            groups,
            rows,
            columns: columns.max(1),
            pages: Vec::new(),
            scale: 1.0,
        }
    }
}

impl PageSource for Tiled {
    fn begin(&mut self, page_width: f64, page_height: f64) -> usize {
        let columns = self.columns;
        let width = self.groups.iter().map(|id| self.canvas.bounds(*id).width()).fold(1.0_f64, f64::max);
        let height = self.groups.iter().map(|id| self.canvas.bounds(*id).height()).fold(1.0_f64, f64::max);
        let available_height = (page_height - 85.0).max(1.0);
        let factor = ((page_width - (columns - 1) as f64 * 20.0) / (columns as f64 * width))
            .min(available_height / (height + 20.0) / self.rows.max(1) as f64)
            .clamp(0.001, 1.0);
        self.scale = factor;
        let rows = if self.rows == 0 {
            (available_height / ((height + 20.0) * factor)).floor().max(1.0) as usize
        } else {
            self.rows
        };
        let capacity = rows * columns;
        self.pages.clear();
        for chunk in self.groups.chunks(capacity) {
            self.pages.push(
                chunk
                    .iter()
                    .enumerate()
                    .map(|(i, id)| {
                        (
                            *id,
                            (i % columns) as f64 * (page_width / columns as f64),
                            60.0 + (i / columns) as f64 * (height + 20.0) * factor,
                        )
                    })
                    .collect(),
            );
        }
        if self.pages.is_empty() {
            self.pages.push(Vec::new());
        }
        self.pages.len()
    }

    fn draw(&mut self, number: usize, cr: &cairo::Context, width: f64, height: f64) {
        let heading = Canvas::new();
        let title_item = heading.text(heading.root(), &self.title, 0.0, 0.0, width, Anchor::NW);
        heading.set(title_item, [Prop::Font("Sans Bold 16px".into())]);
        let subtitle_item = heading.text(heading.root(), &self.subtitle, 0.0, 28.0, width, Anchor::NW);
        heading.set(subtitle_item, [Prop::Font("Sans 12px".into())]);
        let footer = heading.text(
            heading.root(),
            &format!("Page {}/{}", number + 1, self.pages.len()),
            0.0,
            height - 18.0,
            -1.0,
            Anchor::NW,
        );
        heading.set(footer, [Prop::Font("Sans 10px".into())]);
        heading.render(cr, None, 1.0);
        if let Some(items) = self.pages.get(number) {
            for (id, x, y) in items {
                let bounds = self.canvas.bounds(*id);
                let _ = cr.save();
                cr.translate(*x, *y);
                cr.scale(self.scale, self.scale);
                cr.translate(-bounds.x1, -bounds.y1);
                self.canvas.render_item(*id, cr);
                let _ = cr.restore();
            }
        }
    }
}

/// Imprime des blocs cairo sans les couper (aperçu puis impression).
pub fn run_tiled(
    parent: Option<&gtk::Window>,
    title: &str,
    subtitle: &str,
    canvas: Canvas,
    groups: Vec<ItemId>,
    rows: usize,
    columns: usize,
) -> Result<(), gtk::glib::Error> {
    let (t, s) = (title.to_string(), subtitle.to_string());
    run_document(parent, title, move || {
        Box::new(Tiled::new(&t, &s, canvas.clone(), groups.clone(), rows, columns))
    })?;
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
