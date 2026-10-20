//! Contexte d'impression partagé par le canevas et les phases.

use gtk4 as gtk;

/// Dimensions d'une page (équivalent de `GtkPrintContext`), en unités de
/// cairo (points d'imprimante).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageContext {
    pub width: f64,
    pub height: f64,
    pub dpi_x: f64,
    pub dpi_y: f64,
}

impl PageContext {
    /// Page A4 à 72 ppp.
    pub const A4: PageContext = PageContext { width: 595.0, height: 842.0, dpi_x: 72.0, dpi_y: 72.0 };

    pub fn new(width: f64, height: f64, dpi_x: f64, dpi_y: f64) -> Self {
        PageContext { width, height, dpi_x, dpi_y }
    }

    pub fn from_gtk(ctx: &gtk::PrintContext) -> Self {
        PageContext { width: ctx.width(), height: ctx.height(), dpi_x: ctx.dpi_x(), dpi_y: ctx.dpi_y() }
    }
}
