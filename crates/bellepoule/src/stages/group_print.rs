//! Entraînement des jeunes : une page par groupe — ses poules (feuilles
//! d'arbitre avec les matchs et les cases de touches) et son tableau vierge,
//! comme la version web (`Contest::youth_group_sheets`).

use bp_gtk::dialogs;
use gtk::prelude::*;
use gtk4 as gtk;
use gtk4::pango;
use serde_json::Value;

use crate::contest_page::ContestPage;

/// Le tour de poules `index` (composition ou poules) appartient-il à un
/// entraînement des jeunes ?
pub fn is_youth(page: &ContestPage, index: usize) -> bool {
    page.with_contest(|c| {
        let stage = c.schedule.stage(index)?;
        let round = if let Some(a) = stage.as_any().downcast_ref::<bp_core::rounds::pool::PoolAllocatorStage>() {
            a.shared_round()
        } else {
            stage.as_any().downcast_ref::<bp_core::rounds::pool::PoolSupervisorStage>()?.shared_round()
        };
        let youth = round.lock().map(|r| r.is_youth()).unwrap_or(false);
        Some(youth)
    })
    .flatten()
    .unwrap_or(false)
}

/// Aperçu et impression des feuilles des groupes : celui de la poule
/// `selected` ou tous, au choix.
pub fn print(page: &ContestPage, index: usize, selected: Option<usize>) {
    let data = page.with_contest(|c| c.youth_group_sheets(index));
    let Some(result) = data else { return };
    let window = page.window();
    let mut data = match result {
        Ok(data) => data,
        Err(e) => return dialogs::error(window.as_ref(), &e),
    };
    let groups = arr(&data, "groups");
    let numbers: Vec<String> = groups.iter().map(|g| s(g, "group")).collect();
    let current = selected
        .and_then(|p| groups.iter().position(|g| arr(g, "indices").iter().any(|i| i.as_u64() == Some(p as u64))))
        .unwrap_or(0);
    let Some(only) = choose(window.as_ref(), &numbers, current, selected.is_some()) else { return };
    if let (Some(k), Some(list)) = (only, data.get_mut("groups").and_then(Value::as_array_mut)) {
        let keep = list[k].clone();
        *list = vec![keep];
    }
    if let Err(e) = bp_gtk::print::run_document(window.as_ref(), "Groupes", move || Box::new(GroupPrint { data: data.clone() })) {
        dialogs::error(window.as_ref(), &e.to_string());
    }
}

/// « Ce groupe » ou « tous les groupes » : `Some(Some(k))` pour le seul
/// groupe d'indice `k`, `Some(None)` pour tous, `None` si annulé.
fn choose(parent: Option<&gtk::Window>, numbers: &[String], current: usize, prefer_one: bool) -> Option<Option<usize>> {
    if numbers.len() <= 1 {
        return Some(None);
    }
    let dialog = gtk::Dialog::with_buttons(
        Some("Imprimer"),
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
    let one = gtk::CheckButton::with_label("Seulement le groupe");
    let labels: Vec<String> = numbers.iter().map(|n| format!("Groupe {n}")).collect();
    let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    let list = gtk::DropDown::from_strings(&refs);
    list.set_selected(current as u32);
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.append(&one);
    row.append(&list);
    let all = gtk::CheckButton::with_label(&format!("Tous les groupes ({})", numbers.len()));
    all.set_group(Some(&one));
    if prefer_one { one.set_active(true) } else { all.set_active(true) }
    list.set_sensitive(prefer_one);
    {
        let list = list.clone();
        one.connect_toggled(move |b| list.set_sensitive(b.is_active()));
    }
    content.append(&row);
    content.append(&all);
    dialog.content_area().append(&content);
    let response = dialogs::run_dialog(&dialog);
    let result = (response == gtk::ResponseType::Ok).then(|| one.is_active().then(|| list.selected() as usize));
    dialog.destroy();
    result
}

/// Pages des groupes (toutes), pour les ajouter à une autre impression.
pub fn source(page: &ContestPage, index: usize) -> Option<Box<dyn bp_gtk::print::PageSource>> {
    let data = page.with_contest(|c| c.youth_group_sheets(index).ok()).flatten()?;
    Some(Box::new(GroupPrint { data }))
}

struct GroupPrint {
    data: Value,
}

fn arr(v: &Value, k: &str) -> Vec<Value> {
    v.get(k).and_then(Value::as_array).cloned().unwrap_or_default()
}

fn s(v: &Value, k: &str) -> String {
    match v.get(k) {
        Some(Value::String(x)) => x.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

fn f(v: &Value, k: &str) -> f64 {
    v.get(k).and_then(Value::as_f64).unwrap_or(0.0)
}

/// Écrit `text` en haut à gauche de (x, y) ; renvoie (largeur, hauteur).
fn text(cr: &cairo::Context, x: f64, y: f64, value: &str, font: &str, width: Option<f64>, align: pango::Alignment) -> (f64, f64) {
    let layout = pangocairo::functions::create_layout(cr);
    layout.set_font_description(Some(&pango::FontDescription::from_string(font)));
    layout.set_text(value);
    if let Some(w) = width {
        layout.set_width((w * pango::SCALE as f64) as i32);
        layout.set_ellipsize(pango::EllipsizeMode::End);
        layout.set_alignment(align);
    }
    let (w, h) = layout.pixel_size();
    cr.move_to(x, y);
    pangocairo::functions::show_layout(cr, &layout);
    (w as f64, h as f64)
}

// Feuille de poule (unités « naturelles », mises à l'échelle de la colonne).
const NAME_W: f64 = 110.0;
const POS_W: f64 = 16.0;
const CELL: f64 = 20.0;
const SMALL: f64 = 18.0;
const HEAD_H: f64 = 14.0;
const BOUT_H: f64 = 24.0;
const TOUCH: f64 = 8.0;
const RESULTS: [&str; 5] = ["V", "TD", "TR", "Ind.", "Pl."];
/// Colonne « piste · arbitre » de chaque match.
const RES_W: f64 = 66.0;

/// Les matchs de la poule ont-ils une piste ou un arbitre attribués ?
fn has_dispatch(pool: &Value) -> bool {
    arr(pool, "matches").iter().any(|m| !s(m, "referee").is_empty() || m.get("piste").is_some_and(|p| !p.is_null()))
}

/// « DUPONT Hugo (CLUB B) » pour chaque arbitre de la poule.
fn referee_names(pool: &Value) -> Vec<String> {
    arr(pool, "referees")
        .iter()
        .map(|r| {
            let club = s(r, "club");
            if club.is_empty() { s(r, "name") } else { format!("{} ({club})", s(r, "name")) }
        })
        .collect()
}

fn pool_size(pool: &Value) -> (f64, f64) {
    let n = arr(pool, "rows").len() as f64;
    let grid_w = NAME_W + POS_W + n * CELL + RESULTS.len() as f64 * SMALL;
    let boxes = arr(pool, "matches").iter().map(|m| f(m, "boxes")).fold(0.0, f64::max);
    let bout_w = 26.0 + 120.0 + boxes * (TOUCH + 1.0) + if has_dispatch(pool) { RES_W } else { 0.0 };
    let h = 18.0 + HEAD_H + n * CELL + 6.0 + arr(pool, "matches").len() as f64 * BOUT_H;
    (grid_w.max(bout_w), h)
}

fn draw_pool(cr: &cairo::Context, pool: &Value, width: f64) {
    let rows = arr(pool, "rows");
    let n = rows.len();
    // En-tête : nom, arbitre, piste
    let (w, _) = text(cr, 0.0, 0.0, &s(pool, "name"), "Sans Bold 9", None, pango::Alignment::Left);
    let mut x = w + 8.0;
    let referees = referee_names(pool);
    cr.set_source_rgb(0.5, 0.5, 0.5);
    let (w, _) = text(cr, x, 1.0, if referees.len() > 1 { "Arbitres" } else { "Arbitre" }, "Sans Bold 7", None, pango::Alignment::Left);
    x += w + 4.0;
    let piste_x = width - 70.0;
    if referees.is_empty() {
        cr.set_source_rgb(0.33, 0.33, 0.33);
        cr.set_line_width(0.8);
        cr.move_to(x, 12.0);
        cr.line_to(piste_x - 6.0, 12.0);
        let _ = cr.stroke();
    } else {
        cr.set_source_rgb(0.0, 0.0, 0.545);
        text(cr, x, 1.0, &referees.join(" · "), "Sans Bold 6.5", Some((piste_x - x - 6.0).max(20.0)), pango::Alignment::Left);
    }
    cr.set_source_rgb(0.5, 0.5, 0.5);
    let (w, _) = text(cr, piste_x, 1.0, "Piste", "Sans Bold 7", None, pango::Alignment::Left);
    cr.set_source_rgb(0.33, 0.33, 0.33);
    cr.move_to(piste_x + w + 4.0, 12.0);
    cr.line_to(width, 12.0);
    let _ = cr.stroke();
    cr.set_source_rgb(0.0, 0.0, 0.0);
    text(cr, piste_x + w + 6.0, 1.0, &s(pool, "piste"), "Sans 7", None, pango::Alignment::Left);

    // Grille
    let top = 18.0;
    let x0 = NAME_W + POS_W;
    for j in 0..n {
        text(cr, x0 + j as f64 * CELL, top, &(j + 1).to_string(), "Sans Bold 7", Some(CELL), pango::Alignment::Center);
    }
    let rx = x0 + n as f64 * CELL;
    for (k, h) in RESULTS.iter().enumerate() {
        text(cr, rx + k as f64 * SMALL, top + 2.0, h, "Sans Bold 5.5", Some(SMALL), pango::Alignment::Center);
    }
    let gy = top + HEAD_H;
    for (i, r) in rows.iter().enumerate() {
        let y = gy + i as f64 * CELL;
        cr.set_source_rgb(0.0, 0.0, 0.0);
        text(cr, 0.0, y + 5.0, &s(r, "name"), "Sans 6.5", Some(NAME_W - 4.0), pango::Alignment::Left);
        text(cr, NAME_W, y + 5.0, &s(r, "position"), "Sans Bold 7.5", Some(POS_W - 2.0), pango::Alignment::Right);
        let grey = arr(r, "grey");
        let cells = arr(r, "cells");
        for j in 0..n {
            let cx = x0 + j as f64 * CELL;
            if grey.get(j).and_then(Value::as_bool).unwrap_or(false) {
                cr.set_source_rgb(0.745, 0.745, 0.745);
                cr.rectangle(cx, y, CELL, CELL);
                let _ = cr.fill();
            }
            cr.set_source_rgb(0.13, 0.13, 0.13);
            cr.set_line_width(1.2);
            cr.rectangle(cx, y, CELL, CELL);
            let _ = cr.stroke();
            // Score de la case (poule commencée)
            if let Some(t) = cells.get(j).and_then(Value::as_str).filter(|t| !t.is_empty()) {
                text(cr, cx, y + 4.0, t, "Sans Bold 7.5", Some(CELL), pango::Alignment::Center);
            }
        }
        cr.set_line_width(0.8);
        let dashboard = arr(r, "dashboard");
        for k in 0..RESULTS.len() {
            cr.set_source_rgb(0.13, 0.13, 0.13);
            cr.rectangle(rx + k as f64 * SMALL, y, SMALL, CELL);
            let _ = cr.stroke();
            if let Some(t) = dashboard.get(k).and_then(Value::as_str).filter(|t| !t.is_empty()) {
                let font = if k == RESULTS.len() - 1 { "Sans Bold 7" } else { "Sans 6" };
                text(cr, rx + k as f64 * SMALL, y + 5.0, t, font, Some(SMALL), pango::Alignment::Center);
            }
        }
    }

    // Matchs : numéro, les deux tireurs et leurs cases de touches
    let mut y = gy + n as f64 * CELL + 6.0;
    let dispatch = has_dispatch(pool);
    let res_w = if dispatch { RES_W } else { 0.0 };
    for m in arr(pool, "matches") {
        let boxes = f(&m, "boxes") as usize;
        cr.set_source_rgb(0.0, 0.0, 0.0);
        text(cr, 0.0, y + 6.0, &s(&m, "name"), "Sans Bold 6.5", Some(24.0), pango::Alignment::Left);
        let officiating = s(&m, "officiating");
        for (k, o) in arr(&m, "opponents").iter().enumerate() {
            let oy = y + k as f64 * 11.0;
            cr.set_source_rgb(0.0, 0.0, 0.0);
            let label = format!("{} {}", s(o, "position"), s(o, "name"));
            let boxes_w = boxes as f64 * (TOUCH + 1.0);
            // Score du match, s'il est saisi, devant les cases de touches.
            let score = s(o, "score");
            let score_w = if score.is_empty() { 0.0 } else { 14.0 };
            text(cr, 26.0, oy + 1.0, &label, "Sans 6", Some((width - 26.0 - boxes_w - res_w - score_w - 4.0).max(20.0)), pango::Alignment::Left);
            if !score.is_empty() {
                cr.set_source_rgb(0.0, 0.0, 0.545);
                text(cr, width - boxes_w - res_w - score_w - 2.0, oy + 0.5, &score, "Sans Bold 6.5", Some(score_w), pango::Alignment::Right);
            }
            cr.set_source_rgb(0.2, 0.2, 0.2);
            cr.set_line_width(0.6);
            for b in 0..boxes {
                let bx = width - boxes_w + b as f64 * (TOUCH + 1.0);
                cr.rectangle(bx, oy + 1.5, TOUCH, TOUCH);
                let _ = cr.stroke();
                text(cr, bx, oy + 2.5, &(b + 1).to_string(), "Sans 3.5", Some(TOUCH), pango::Alignment::Center);
            }
        }
        // Piste et arbitre de ce match (alternance des arbitres).
        if dispatch {
            let boxes_w = boxes as f64 * (TOUCH + 1.0);
            let rx = width - boxes_w - res_w;
            let piste = m.get("piste").and_then(Value::as_u64).map(|p| format!("Piste {p:02}")).unwrap_or_default();
            cr.set_source_rgb(0.4, 0.4, 0.4);
            text(cr, rx, y + 1.0, &piste, "Sans 5", Some(res_w - 4.0), pango::Alignment::Left);
            cr.set_source_rgb(0.0, 0.0, 0.545);
            text(cr, rx, y + 11.0, &s(&m, "referee"), "Sans Bold 5.5", Some(res_w - 4.0), pango::Alignment::Left);
        }
        if !officiating.is_empty() {
            cr.set_source_rgb(0.4, 0.4, 0.4);
            text(cr, 0.0, y + 15.0, &officiating.replace('\n', " · "), "Sans 4.5", Some(26.0), pango::Alignment::Left);
        }
        cr.set_source_rgb(0.73, 0.73, 0.73);
        cr.set_line_width(0.5);
        cr.set_dash(&[1.0, 1.5], 0.0);
        cr.move_to(0.0, y + BOUT_H - 1.0);
        cr.line_to(width, y + BOUT_H - 1.0);
        let _ = cr.stroke();
        cr.set_dash(&[], 0.0);
        y += BOUT_H;
    }
}

/// Tableau vierge : cases et traits calculés par le cœur (unités du web).
fn draw_bracket(cr: &cairo::Context, bracket: &Value) {
    cr.set_source_rgb(0.2, 0.2, 0.2);
    cr.set_line_width(1.1);
    for line in arr(bracket, "lines") {
        let points: Vec<(f64, f64)> = line
            .as_array()
            .map(|p| p.iter().filter_map(|xy| Some((xy.get(0)?.as_f64()?, xy.get(1)?.as_f64()?))).collect())
            .unwrap_or_default();
        for (i, (x, y)) in points.iter().enumerate() {
            if i == 0 {
                cr.move_to(*x, *y);
            } else {
                cr.line_to(*x, *y);
            }
        }
        let _ = cr.stroke();
    }
    for it in arr(bracket, "items") {
        let (x, y, w, h) = (f(&it, "x"), f(&it, "y"), f(&it, "w"), f(&it, "h"));
        match s(&it, "t").as_str() {
            "slot" => {
                cr.set_source_rgb(0.2, 0.2, 0.2);
                cr.set_line_width(1.1);
                cr.move_to(x, y + h);
                cr.line_to(x + w, y + h);
                let _ = cr.stroke();
                cr.set_source_rgb(0.0, 0.0, 0.545);
                text(cr, x + 1.0, y + h - 15.0, &s(&it, "text"), "Sans Bold 9", Some(w - 2.0), pango::Alignment::Left);
            }
            "score" => {
                cr.set_source_rgb(0.33, 0.33, 0.33);
                cr.set_line_width(0.8);
                cr.rectangle(x, y, w, h);
                let _ = cr.stroke();
                let score = s(&it, "text");
                if !score.is_empty() {
                    cr.set_source_rgb(0.0, 0.0, 0.0);
                    text(cr, x, y + 3.0, &score, "Sans Bold 9", Some(w), pango::Alignment::Center);
                }
            }
            "match" => {
                cr.set_source_rgb(0.4, 0.4, 0.4);
                text(cr, x, y, &s(&it, "text"), "Sans 7.5", None, pango::Alignment::Left);
            }
            _ => {
                cr.set_source_rgb(0.0, 0.0, 0.0);
                text(cr, x, y, &s(&it, "text"), "Sans Bold 10.5", None, pango::Alignment::Left);
            }
        }
    }
}

const BAND_H: f64 = 26.0;
const GAP: f64 = 14.0;

impl bp_gtk::print::PageSource for GroupPrint {
    fn begin(&mut self, _width: f64, _height: f64) -> usize {
        arr(&self.data, "groups").len().max(1)
    }

    fn page_name(&self, page: usize) -> String {
        arr(&self.data, "groups").get(page).map(|g| format!("Groupe {}", s(g, "group"))).unwrap_or_default()
    }

    fn draw(&mut self, page: usize, cr: &cairo::Context, width: f64, height: f64) {
        let Some(group) = arr(&self.data, "groups").into_iter().nth(page) else { return };
        let pools = arr(&group, "pools");
        let bracket = group.get("bracket").filter(|b| !b.is_null()).cloned();

        // Mise en page à la largeur de la page, puis réduction pour tenir en hauteur.
        let cols = pools.len().clamp(1, 2);
        let col_w = (width - GAP * (cols - 1) as f64) / cols as f64;
        let mut placed = Vec::new();
        let mut y = BAND_H + 22.0;
        for row in pools.chunks(cols) {
            let mut row_h: f64 = 0.0;
            for (k, pool) in row.iter().enumerate() {
                let (w, h) = pool_size(pool);
                let scale = (col_w / w).min(1.0);
                placed.push((pool.clone(), k as f64 * (col_w + GAP), y, scale, w));
                row_h = row_h.max(h * scale);
            }
            y += row_h + GAP;
        }
        let (bw, bh) = bracket.as_ref().map(|b| (f(b, "width"), f(b, "height"))).unwrap_or((0.0, 0.0));
        let bracket_scale = if bw > 0.0 { (width / bw).min(1.0) } else { 1.0 };
        let total = y + bh * bracket_scale;
        let fit = (height / total).min(1.0);

        cr.save().ok();
        cr.translate((width - width * fit) / 2.0, 0.0);
        cr.scale(fit, fit);

        // Bandeau du groupe
        cr.set_source_rgb(0.933, 0.839, 0.502);
        cr.rectangle(0.0, 0.0, width, BAND_H);
        let _ = cr.fill_preserve();
        cr.set_source_rgb(0.53, 0.53, 0.53);
        cr.set_line_width(0.8);
        let _ = cr.stroke();
        cr.set_source_rgb(0.0, 0.0, 0.0);
        text(cr, 8.0, 3.0, &format!("Groupe {}", s(&group, "group")), "Sans Bold 14", Some(width - 16.0), pango::Alignment::Center);
        let mut all: Vec<String> = Vec::new();
        for pool in &pools {
            for name in referee_names(pool) {
                if !all.contains(&name) {
                    all.push(name);
                }
            }
        }
        if !all.is_empty() {
            cr.set_source_rgb(0.0, 0.0, 0.545);
            text(cr, 0.0, BAND_H + 2.0, &format!("Arbitres du groupe : {}", all.join(" · ")), "Sans Bold 8", Some(width), pango::Alignment::Center);
        }

        for (pool, x, y, scale, w) in placed {
            cr.save().ok();
            cr.translate(x, y);
            cr.scale(scale, scale);
            draw_pool(cr, &pool, w);
            cr.restore().ok();
        }
        if let Some(b) = bracket {
            cr.save().ok();
            cr.translate((width - bw * bracket_scale) / 2.0, y);
            cr.scale(bracket_scale, bracket_scale);
            draw_bracket(cr, &b);
            cr.restore().ok();
        }
        cr.restore().ok();
    }
}
