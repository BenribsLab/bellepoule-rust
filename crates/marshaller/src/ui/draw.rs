//! Dessin cairo du Marshaller : pistes de la salle (`Piste`), frise
//! (`Timeline::Redraw`), QR code et impressions des arbitres
//! (`Marshaller::DrawPage`).

use std::f64::consts::FRAC_PI_2;

use bp_core::i18n::tr;
use gtk4 as gtk;
use gtk::gdk;
use gtk::prelude::*;

use crate::color::Rgb;
use crate::hall::Hall;
use crate::model::{HOURS_MONITORED, START_MARGIN};
use crate::piste::{Piste, BORDER_W, H, W};
use crate::time::{diff, HOUR, MINUTE, SECOND};

/// Image de `resources/glade/images` (mise en cache par GTK).
pub fn pixbuf(name: &str) -> Option<gtk::gdk_pixbuf::Pixbuf> {
    gtk::gdk_pixbuf::Pixbuf::from_file(crate::config::image_path(name)).ok()
}

fn set_rgb(cr: &cairo::Context, c: Rgb) {
    cr.set_source_rgb(c.red_f(), c.green_f(), c.blue_f());
}

fn set_rgba(cr: &cairo::Context, rgba: u32) {
    let r = ((rgba >> 24) & 0xff) as f64 / 255.0;
    let g = ((rgba >> 16) & 0xff) as f64 / 255.0;
    let b = ((rgba >> 8) & 0xff) as f64 / 255.0;
    let a = (rgba & 0xff) as f64 / 255.0;
    cr.set_source_rgba(r, g, b, a);
}

/// Texte (balisage Pango) ancré à gauche (`NW`) ou centré en haut (`N`).
fn text(cr: &cairo::Context, markup: &str, font: &str, x: f64, y: f64, centered: bool) {
    let layout = pangocairo::functions::create_layout(cr);
    layout.set_font_description(Some(&pango::FontDescription::from_string(font)));
    layout.set_markup(markup);
    let (w, _) = layout.pixel_size();
    let x = if centered { x - w as f64 / 2.0 } else { x };
    cr.move_to(x, y);
    pangocairo::functions::show_layout(cr, &layout);
}

fn image(cr: &cairo::Context, name: &str, cx: f64, cy: f64, scale: f64) {
    let Some(pb) = pixbuf(name) else { return };
    let (w, h) = (pb.width() as f64 * scale, pb.height() as f64 * scale);
    let _ = cr.save();
    cr.translate(cx - w / 2.0, cy - h / 2.0);
    cr.scale(scale, scale);
    cr.set_source_pixbuf(&pb, 0.0, 0.0);
    let _ = cr.paint();
    let _ = cr.restore();
}

/// `Piste` : fond (couleur du match), avancement, numéro, titre, arbitre,
/// plot (piste bloquée), drapeau d'arrivée et volet.
pub fn piste(cr: &cairo::Context, hall: &Hall, p: &Piste) {
    let _ = cr.save();
    cr.translate(p.tx, p.ty);
    if !p.horizontal {
        cr.rotate(FRAC_PI_2);
    }

    let slot = hall.piste_displayed_slot(p.key).and_then(|sk| hall.slots.get(&sk).map(|s| (sk, s)));
    let color = slot
        .and_then(|(_, s)| s.jobs.first())
        .and_then(|j| hall.jobs.get(j))
        .map(|j| j.color);

    // Fond
    cr.rectangle(0.0, 0.0, W, H);
    if p.focused {
        cr.set_source_rgb(0.745, 0.745, 0.745);
    } else if let Some(c) = color {
        set_rgb(cr, c);
    } else {
        cr.set_source_rgb(0.827, 0.827, 0.827);
    }
    let _ = cr.fill_preserve();
    cr.set_source_rgb(0.0, 0.0, 0.0);
    cr.set_line_width(if p.selected { BORDER_W * 4.0 } else { BORDER_W });
    let _ = cr.stroke();

    if let Some((sk, s)) = slot {
        // Avancement
        let elapsed = diff(hall.clock.now(), s.start);
        if elapsed > 0 && s.duration > 0 {
            let progress = (elapsed as f64 * W / s.duration as f64).min(W);
            set_rgba(cr, 0xd3d3d360);
            cr.rectangle(BORDER_W, BORDER_W, progress, H - 2.0 * BORDER_W);
            let _ = cr.fill();
            set_rgba(cr, 0x00000060);
            cr.rectangle(progress, BORDER_W, 0.7, H - 2.0 * BORDER_W);
            let _ = cr.fill();
        }
        // Titre
        cr.set_source_rgb(0.0, 0.0, 0.0);
        text(cr, &hall.piste_title_markup(sk), "Sans 12px", W / 2.0, 1.5, true);
        // Arbitre
        let referee = if s.additional_pistes.is_empty() { s.referees.first() }
            else if s.pistes().len() == s.referees.len() { s.pistes().iter().position(|key| *key == p.key).and_then(|i| s.referees.get(i)) }
            else { None };
        if let Some(r) = referee.and_then(|r| hall.referees.get(*r)) {
            image(cr, "referee.png", 22.0, H * 0.5 + 6.0, 0.8 * 0.5);
            cr.set_source_rgb(0.0, 0.0, 0.0);
            text(cr, &glib_escape(&r.player.name()), "Sans Bold 9px", 32.0, H * 0.5, false);
        }
        // Arrivée
        if hall.slot_is_over(sk) {
            image(cr, "finish.png", W / 2.0, H - 15.0, 0.5);
        }
    }

    // Numéro
    cr.set_source_rgb(0.0, 0.0, 0.0);
    text(cr, &p.id.to_string(), "Sans Bold 15px", 1.5, 1.5, false);

    // Plot
    if p.blocked {
        image(cr, "VLC.png", W / 2.0, H / 2.0, 0.5);
    }
    // Volet
    if p.blurred {
        set_rgba(cr, 0xf3f3f3c0);
        cr.rectangle(0.0, 0.0, W, H);
        let _ = cr.fill();
    }
    let _ = cr.restore();
}

fn glib_escape(s: &str) -> String {
    gtk::glib::markup_escape_text(s).to_string()
}

/// `Timeline::Redraw` : matchs planifiés par compétition, graduations,
/// heure courante et curseur.
pub fn timeline(cr: &cairo::Context, hall: &Hall, width: f64, height: f64) {
    use chrono::Timelike;
    let tl = &hall.timeline;
    let time_scale = width / (HOURS_MONITORED * HOUR + START_MARGIN) as f64;
    let comp_scale = height;

    cr.set_source_rgb(1.0, 1.0, 1.0);
    cr.paint().ok();

    // Créneaux (deux couches : couleur puis contour)
    for layer in 0..2 {
        for (i, cid) in tl.competitions.iter().enumerate() {
            let Some(c) = hall.competition(*cid) else { continue };
            for b in &c.batches {
                for j in &b.scheduled {
                    let Some(s) = hall.jobs.get(j).and_then(|j| j.slot).and_then(|s| hall.slots.get(&s)) else { continue };
                    let x = diff(s.start, tl.origin) as f64 * time_scale;
                    let w = (s.duration - 30 * SECOND) as f64 * time_scale;
                    let y = i as f64 * comp_scale / 10.0;
                    if layer == 0 {
                        set_rgb(cr, c.color);
                        cr.rectangle(x, y, w, comp_scale / 10.0);
                        let _ = cr.fill();
                    } else {
                        cr.set_source_rgb(0.0, 0.0, 0.0);
                        cr.set_line_width(1.0);
                        cr.rectangle(x + 0.5, y + 0.5, w - 1.0, comp_scale / 10.0 - 1.0);
                        let _ = cr.stroke();
                    }
                }
            }
        }
    }

    // Graduations
    let origin_hh = tl.origin.hour() as i64;
    let origin_mm = tl.origin.minute() as i64 + tl.origin.second() as i64 / 60;
    let first_mark = if origin_mm % 15 != 0 { origin_mm + 15 - origin_mm % 15 } else { origin_mm };
    let offset_mm = first_mark - origin_mm;
    for mark in 0..=(HOURS_MONITORED * 4) {
        let x = ((offset_mm + mark * 15) * MINUTE) as f64 * time_scale;
        cr.set_source_rgb(0.0, 0.0, 0.0);
        if (mark * 15 + first_mark) % 60 == 0 {
            cr.rectangle(x, 0.7 * comp_scale, 0.5, 0.3 * comp_scale);
            let _ = cr.fill();
            let hh = (origin_hh + (mark * 15 + first_mark) / 60) % 24;
            let layout = pangocairo::functions::create_layout(cr);
            layout.set_font_description(Some(&pango::FontDescription::from_string("Sans 10px")));
            layout.set_markup(&format!("<span background=\"white\" foreground=\"grey\">{hh:02}:00</span>"));
            let (w, h) = layout.pixel_size();
            cr.move_to(x - w as f64 / 2.0, comp_scale - h as f64);
            pangocairo::functions::show_layout(cr, &layout);
        } else {
            cr.rectangle(x, 0.95 * comp_scale, 0.3, 0.05 * comp_scale);
            let _ = cr.fill();
        }
    }

    // Heure courante
    let now_x = diff(hall.clock.now(), tl.origin) as f64 * time_scale;
    set_rgba(cr, 0x0000000f);
    cr.rectangle(0.0, 0.0, now_x.max(0.0), comp_scale);
    let _ = cr.fill();
    set_rgba(cr, 0x00000060);
    cr.rectangle(now_x, 0.0, 0.7, comp_scale);
    let _ = cr.fill();

    // Curseur
    let cx = tl.cursor as f64 * time_scale;
    cr.set_source_rgb(0.0, 0.0, 0.0);
    cr.rectangle(cx, 0.0, 2.0, comp_scale);
    let _ = cr.fill();
    let t = tl.cursor_time(false);
    let layout = pangocairo::functions::create_layout(cr);
    layout.set_font_description(Some(&pango::FontDescription::from_string("Sans 10px")));
    layout.set_markup(&format!(
        " <span weight=\"bold\" background=\"black\" foreground=\"white\"> {:02}:{:02} </span>",
        t.hour(),
        t.minute()
    ));
    let (_, h) = layout.pixel_size();
    cr.move_to(cx, comp_scale / 2.0 - h as f64 / 2.0);
    pangocairo::functions::show_layout(cr, &layout);
}

/// QR code en texture GTK.
pub fn qr_texture(text: &str) -> Option<gdk::Texture> {
    let png = bp_net::flash_code::QrMatrix::encode(text)?.to_image(4).to_png();
    gdk::Texture::from_bytes(&gtk::glib::Bytes::from_owned(png)).ok()
}

/// Nombre de tickets repas par page (`NB_TICKET_X/Y_PER_SHEET`).
const TICKETS_X: usize = 2;
const TICKETS_Y: usize = 6;
/// Arbitres par page du carnet de paiement (`NB_REFEREE_PER_SHEET`).
const REFEREES_PER_SHEET: usize = 20;

/// `Marshaller::PreparePrint` / `DrawPage` : tickets repas ou carnet de
/// paiement des arbitres.
pub struct RefereesPrint {
    /// Nom, prénom, niveau.
    pub referees: Vec<(String, String, String)>,
    pub meal_tickets: bool,
    pub title: String,
}

impl bp_gtk::print::PageSource for RefereesPrint {
    fn page_name(&self, _page: usize) -> String {
        self.title.clone()
    }

    fn begin(&mut self, _width: f64, _height: f64) -> usize {
        let per = if self.meal_tickets { TICKETS_X * TICKETS_Y } else { REFEREES_PER_SHEET };
        self.referees.len().div_ceil(per).max(1)
    }

    fn draw(&mut self, page: usize, cr: &cairo::Context, width: f64, height: f64) {
        let unit = width / 100.0;
        let font = |size: f64, bold: bool| format!("Sans {}{:.1}px", if bold { "Bold " } else { "" }, size * unit);
        let draw_text = |cr: &cairo::Context, t: &str, f: &str, x: f64, y: f64, max_w: Option<f64>| {
            let layout = pangocairo::functions::create_layout(cr);
            layout.set_font_description(Some(&pango::FontDescription::from_string(f)));
            layout.set_text(t);
            if let Some(w) = max_w {
                layout.set_width((w * pango::SCALE as f64) as i32);
                layout.set_ellipsize(pango::EllipsizeMode::End);
            }
            cr.move_to(x, y);
            pangocairo::functions::show_layout(cr, &layout);
        };
        cr.set_source_rgb(0.0, 0.0, 0.0);
        if self.meal_tickets {
            let spacing = 2.0 * unit;
            let tw = (width - (TICKETS_X - 1) as f64 * spacing) / TICKETS_X as f64;
            let th = (height - (TICKETS_Y - 1) as f64 * spacing) / TICKETS_Y as f64;
            let start = page * TICKETS_X * TICKETS_Y;
            for (i, (name, first, _)) in self.referees.iter().skip(start).take(TICKETS_X * TICKETS_Y).enumerate() {
                let (r, c) = (i / TICKETS_X, i % TICKETS_X);
                let (x, y) = (c as f64 * (tw + spacing), r as f64 * (th + spacing));
                cr.set_source_rgb(0.5, 0.5, 0.5);
                cr.set_line_width(0.7);
                cr.rectangle(x, y, tw, th);
                let _ = cr.stroke();
                cr.set_source_rgb(0.0, 0.0, 0.0);
                draw_text(cr, name, &font(4.0, true), x + unit, y + unit, Some(tw / 2.0));
                draw_text(cr, first, &font(3.0, false), x + unit, y + 6.0 * unit, Some(tw / 2.0));
                let items = [tr("Meal"), tr("Drink"), tr("Dessert"), tr("Coffee")];
                let ih = (th - 2.0 * unit) / items.len() as f64;
                for (k, item) in items.iter().enumerate() {
                    let iy = y + unit + k as f64 * ih;
                    cr.set_source_rgb(0.75, 0.75, 0.75);
                    cr.rectangle(x + tw / 2.0, iy, tw / 2.0 - unit, ih - 0.6);
                    let _ = cr.fill();
                    cr.set_source_rgb(0.0, 0.0, 0.0);
                    draw_text(cr, item, &font(3.6, false), x + tw / 2.0 + unit, iy, None);
                }
            }
        } else {
            draw_text(cr, &tr("Payment book"), &font(6.0, true), width / 2.0 - 20.0 * unit, 0.0, None);
            let row_h = (height - 14.0 * unit) / REFEREES_PER_SHEET as f64;
            let cols = [0.0, 25.0, 50.0, 62.0, 78.0];
            let currency = "€".to_string();
            for (i, (name, first, level)) in self.referees.iter().skip(page * REFEREES_PER_SHEET).take(REFEREES_PER_SHEET).enumerate() {
                let y = 10.0 * unit + i as f64 * row_h;
                cr.set_source_rgb(0.0, 0.0, 0.0);
                for (k, t) in [name, first, level].iter().enumerate() {
                    draw_text(cr, t, &font(2.0, false), cols[k] * unit, y + row_h / 3.0, Some(24.0 * unit));
                }
                cr.set_source_rgb(0.5, 0.5, 0.5);
                draw_text(cr, &currency, &font(2.0, false), cols[3] * unit, y + row_h / 3.0, None);
                draw_text(cr, &tr("Signature"), &font(2.0, false), cols[4] * unit, y + row_h / 3.0, None);
                cr.set_line_width(0.2);
                cr.move_to(0.0, y + row_h);
                cr.line_to(width, y + row_h);
                let _ = cr.stroke();
            }
            cr.set_source_rgb(0.0, 0.0, 0.0);
            draw_text(cr, &format!("{} {}", tr("Page"), page + 1), &font(2.0, true), width - 12.0 * unit, height - 4.0 * unit, None);
        }
    }
}
