//! Rencontres par équipes : impression des feuilles de match en relais
//! (celles des rencontres de la phase, ou des feuilles vierges), à l'unité
//! ou par trois (une par équipe et une pour l'arbitre).

use bp_core::relay::RelayFormat;
use bp_core::relay_sheet::{self, RelaySheetInfo, Shape};
use bp_gtk::dialogs;
use gtk4::pango;
use gtk4::{self as gtk, prelude::*};

use crate::contest_page::ContestPage;

/// Compétition par équipes ?
pub fn is_team(page: &ContestPage) -> bool {
    page.with_contest(|c| c.settings.team_event).unwrap_or(false)
}

/// Choix : rencontres de la phase ou feuilles vierges, et nombre d'exemplaires.
fn choose(parent: Option<&gtk::Window>, bouts: usize, blank_first: bool) -> Option<(bool, u32, bool)> {
    let dialog = gtk::Dialog::with_buttons(
        Some("Feuilles de match par équipes"),
        parent,
        gtk::DialogFlags::MODAL,
        &[("Annuler", gtk::ResponseType::Cancel), ("Imprimer", gtk::ResponseType::Ok)],
    );
    dialog.set_default_response(gtk::ResponseType::Ok);
    let content = gtk::Box::new(gtk::Orientation::Vertical, 8);
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    content.set_margin_start(16);
    content.set_margin_end(16);
    let matches = gtk::CheckButton::with_label(&format!("Les rencontres de la phase ({bouts}) : remplies dès que l'arbitre a saisi la composition sur SmartPoule"));
    let blank = gtk::CheckButton::with_label("Des feuilles vierges");
    blank.set_group(Some(&matches));
    let count = gtk::SpinButton::with_range(1.0, 200.0, 1.0);
    count.set_value(1.0);
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.append(&blank);
    row.append(&count);
    if bouts == 0 || blank_first {
        blank.set_active(true);
    } else {
        matches.set_active(true);
    }
    matches.set_sensitive(bouts > 0);
    content.append(&matches);
    content.append(&row);
    let three = gtk::CheckButton::with_label("Par 3 : une feuille pour chaque équipe et une pour l'arbitre");
    content.append(&three);
    let hint = gtk::Label::new(Some("Les tireurs ne sont jamais préremplis : chaque équipe donne son ordre à l'arbitre au début de la rencontre."));
    hint.set_wrap(true);
    hint.set_xalign(0.0);
    hint.add_css_class("dim-label");
    content.append(&hint);
    dialog.content_area().append(&content);
    let ok = dialogs::run_dialog(&dialog) == gtk::ResponseType::Ok;
    let result = ok.then(|| (blank.is_active(), count.value_as_int() as u32, three.is_active()));
    dialog.destroy();
    result
}

/// Feuilles des rencontres de la phase `index` (ou de la poule `pool`), ou vierges.
pub fn print(page: &ContestPage, index: usize, pool: Option<usize>, blank_first: bool) {
    let Some(Some((format, bouts))) = page.with_contest(|c| c.relay_sheets(index, pool)) else { return };
    let window = page.window();
    let Some((blank, count, three)) = choose(window.as_ref(), bouts.len(), blank_first) else { return };
    let base: Vec<RelaySheetInfo> = if blank {
        let sheet = page.with_contest(|c| c.blank_relay_sheet()).unwrap_or_default();
        vec![sheet; count as usize]
    } else {
        bouts
    };
    let sheets: Vec<RelaySheetInfo> = base.iter().flat_map(|s| relay_sheet::copies(s, three)).collect();
    if sheets.is_empty() {
        return;
    }
    if let Err(e) = bp_gtk::print::run_document_without_header(window.as_ref(), "Feuilles par équipes", move || Box::new(RelayPrint { format, sheets: sheets.clone() })) {
        dialogs::error(window.as_ref(), &e.to_string());
    }
}

/// Feuilles des rencontres de la phase (ou une vierge s'il n'y en a pas), pour
/// les rendre en image.
pub fn source(page: &ContestPage, index: usize) -> Option<Box<dyn bp_gtk::print::PageSource>> {
    let (format, mut sheets) = page.with_contest(|c| c.relay_sheets(index, None)).flatten()?;
    // Une rencontre déjà commencée en premier (feuille remplie).
    sheets.sort_by_key(|s| s.log.is_none());
    if sheets.is_empty() {
        sheets.push(page.with_contest(|c| c.blank_relay_sheet())?);
    }
    Some(Box::new(RelayPrint { format, sheets }))
}

struct RelayPrint {
    format: RelayFormat,
    sheets: Vec<RelaySheetInfo>,
}

fn rgb(color: &str) -> (f64, f64, f64) {
    let hex = color.trim_start_matches('#');
    let v = |i: usize| u8::from_str_radix(hex.get(i..i + 2).unwrap_or("00"), 16).unwrap_or(0) as f64 / 255.0;
    (v(0), v(2), v(4))
}

impl bp_gtk::print::PageSource for RelayPrint {
    fn begin(&mut self, _width: f64, _height: f64) -> usize {
        self.sheets.len()
    }

    fn page_name(&self, page: usize) -> String {
        self.sheets.get(page).map(|s| [s.round.clone(), s.bout.clone()].join(" ")).unwrap_or_default()
    }

    fn draw(&mut self, page: usize, cr: &cairo::Context, width: f64, height: f64) {
        let Some(info) = self.sheets.get(page) else { return };
        let scale = (width / relay_sheet::WIDTH).min(height / relay_sheet::HEIGHT);
        cr.save().ok();
        cr.translate((width - relay_sheet::WIDTH * scale) / 2.0, 0.0);
        cr.scale(scale, scale);
        for shape in relay_sheet::layout(&self.format, info) {
            match shape {
                Shape::Rect { x, y, w, h, stroke, fill, line } => {
                    cr.rectangle(x, y, w, h);
                    if let Some(f) = fill {
                        let (r, g, b) = rgb(&f);
                        cr.set_source_rgb(r, g, b);
                        if stroke.is_some() {
                            let _ = cr.fill_preserve();
                        } else {
                            let _ = cr.fill();
                        }
                    }
                    if let Some(s) = stroke {
                        let (r, g, b) = rgb(&s);
                        cr.set_source_rgb(r, g, b);
                        cr.set_line_width(line);
                        let _ = cr.stroke();
                    }
                }
                Shape::Line { x1, y1, x2, y2, color, line } => {
                    let (r, g, b) = rgb(&color);
                    cr.set_source_rgb(r, g, b);
                    cr.set_line_width(line);
                    cr.move_to(x1, y1);
                    cr.line_to(x2, y2);
                    let _ = cr.stroke();
                }
                Shape::Text { x, y, size, bold, color, text, anchor } => {
                    let layout = pangocairo::functions::create_layout(cr);
                    let mut font = pango::FontDescription::from_string("Sans");
                    font.set_absolute_size(size * pango::SCALE as f64);
                    if bold {
                        font.set_weight(pango::Weight::Bold);
                    }
                    layout.set_font_description(Some(&font));
                    layout.set_text(&text);
                    let (w, _) = layout.pixel_size();
                    let baseline = layout.baseline() as f64 / pango::SCALE as f64;
                    let left = match anchor {
                        "middle" => x - w as f64 / 2.0,
                        "end" => x - w as f64,
                        _ => x,
                    };
                    let (r, g, b) = rgb(&color);
                    cr.set_source_rgb(r, g, b);
                    cr.move_to(left, y - baseline);
                    pangocairo::functions::show_layout(cr, &layout);
                }
            }
        }
        cr.restore().ok();
    }
}
