//! Documents de listes : mêmes cellules pour l'impression et l'export PDF.

use bp_core::player::{AttrId, Owner, PlayerRef, PlayerStore};
use bp_core::stage::RankEntry;
use bp_gtk::canvas::{Anchor, Canvas, ItemId, Prop};
use gtk4 as gtk;
use std::path::Path;

use super::{
    players_list::{Cell, ColumnSpec, RowData},
    render,
};

pub struct ListDocument {
    pub title: String,
    pub subtitle: String,
    pub specs: Vec<ColumnSpec>,
    pub rows: Vec<RowData>,
    pub classification_borders: bool,
}

pub fn rows_for(
    players: &PlayerStore,
    refs: &[PlayerRef],
    specs: &[ColumnSpec],
    owner: Owner,
) -> Vec<RowData> {
    refs.iter()
        .filter_map(|r| {
            let player = players.get(*r)?;
            Some(RowData {
                player: *r,
                cells: specs
                    .iter()
                    .map(|s| render::cell_for(player, s, owner))
                    .collect(),
                sort: render::sort_values(player, specs, owner),
                children: Vec::new(),
                dimmed: false,
            })
        })
        .collect()
}

/// Les phases calculent parfois les rangs à la volée ; les écrire sur une
/// copie permet d'afficher le classement sans changer les données sauvegardées.
pub fn ranked_rows(
    players: &PlayerStore,
    entries: &[RankEntry],
    specs: &[ColumnSpec],
    owner: Owner,
) -> Vec<RowData> {
    entries
        .iter()
        .filter_map(|entry| {
            let mut player = players.get(entry.player)?.clone();
            player.set_uint(&AttrId::local("rank", owner), entry.rank);
            player.set_str(&AttrId::local("status", owner), &entry.status.to_string());
            Some(RowData {
                player: entry.player,
                cells: specs
                    .iter()
                    .map(|s| render::cell_for(&player, s, owner))
                    .collect(),
                sort: render::sort_values(&player, specs, owner),
                children: Vec::new(),
                dimmed: entry.status != 'Q',
            })
        })
        .collect()
}

impl ListDocument {
    fn scene(&self) -> (Canvas, Vec<ItemId>) {
        let canvas = Canvas::new();
        let mut groups = Vec::new();
        // Les en-têtes sont répétés ; une feuille ne dépasse jamais 25 lignes.
        for (page, rows) in self.rows.chunks(25).enumerate() {
            let group = canvas.group(canvas.root());
            let table = canvas.table(group);
            canvas.set(table, [Prop::ColumnSpacing(12.0), Prop::RowSpacing(5.0)]);
            for (col, spec) in self.specs.iter().enumerate() {
                let item = canvas.put_text_in_table(table, &spec.title, 0, col as u32);
                canvas.set(item, [Prop::Font("Sans Bold 14px".into())]);
            }
            let mut separators = Vec::new();
            for (row, data) in rows.iter().enumerate() {
                for (col, cell) in data.cells.iter().enumerate() {
                    let item = match cell {
                        Cell::Icon(icon) => canvas.put_pixbuf_in_table(
                            table,
                            icon.as_ref(),
                            row as u32 + 1,
                            col as u32,
                        ),
                        _ => {
                            let text = match cell {
                                Cell::Text(s) => s.clone(),
                                Cell::Toggle(v) => {
                                    if *v {
                                        "✓".into()
                                    } else {
                                        "□".into()
                                    }
                                }
                                Cell::Progress(v) => v.to_string(),
                                _ => unreachable!(),
                            };
                            canvas.put_text_in_table(table, &text, row as u32 + 1, col as u32)
                        }
                    };
                    if data.dimmed {
                        canvas.set(item, [Prop::FillColor("#666666".into())]);
                    }
                    if col == 0
                        && self.classification_borders
                        && bp_core::contest::html::is_power_of_two_border(page * 25 + row)
                    {
                        separators.push(item);
                    }
                }
            }
            let width = canvas.bounds(table).width();
            for item in separators {
                let y = canvas.bounds(item).y1 - 2.0;
                let line = canvas.polyline(group, false, &[(0.0, y), (width, y)]);
                canvas.set(
                    line,
                    [Prop::StrokeColor("#808080".into()), Prop::LineWidth(0.5)],
                );
            }
            groups.push(group);
        }
        if groups.is_empty() {
            groups.push(canvas.group(canvas.root()));
        }
        (canvas, groups)
    }

    pub fn print(&self, parent: Option<&gtk::Window>) -> Result<(), glib::Error> {
        let (canvas, groups) = self.scene();
        bp_gtk::print::run_tiled(parent, &self.title, &self.subtitle, canvas, groups, 1, 1)
    }

    /// Pages de la liste, pour l'assembler avec d'autres documents.
    pub fn source(&self) -> Box<dyn bp_gtk::print::PageSource> {
        let (canvas, groups) = self.scene();
        Box::new(bp_gtk::print::Tiled::new(&self.title, &self.subtitle, canvas, groups, 1, 1))
    }

    pub fn export_pdf(&self, path: &Path) -> Result<(), String> {
        let surface = cairo::PdfSurface::new(595.0, 842.0, path).map_err(|e| e.to_string())?;
        let cr = cairo::Context::new(&surface).map_err(|e| e.to_string())?;
        let (canvas, groups) = self.scene();
        let width = groups
            .iter()
            .map(|g| canvas.bounds(*g).width())
            .fold(1.0_f64, f64::max);
        let height = groups
            .iter()
            .map(|g| canvas.bounds(*g).height())
            .fold(1.0_f64, f64::max);
        let factor = (547.0 / width).min(720.0 / height).min(1.0);
        for (page, group) in groups.iter().enumerate() {
            let heading = Canvas::new();
            let title = heading.text(heading.root(), &self.title, 24.0, 24.0, 547.0, Anchor::NW);
            heading.set(
                title,
                [
                    Prop::Font("Sans Bold 16px".into()),
                    Prop::Ellipsize(gtk::pango::EllipsizeMode::End),
                ],
            );
            let subtitle = heading.text(
                heading.root(),
                &self.subtitle,
                24.0,
                49.0,
                547.0,
                Anchor::NW,
            );
            heading.set(
                subtitle,
                [
                    Prop::Font("Sans 12px".into()),
                    Prop::Ellipsize(gtk::pango::EllipsizeMode::End),
                ],
            );
            let footer = heading.text(
                heading.root(),
                &format!("Page {}/{}", page + 1, groups.len()),
                24.0,
                808.0,
                -1.0,
                Anchor::NW,
            );
            heading.set(footer, [Prop::Font("Sans 10px".into())]);
            heading.render(&cr, None, 1.0);
            let bounds = canvas.bounds(*group);
            cr.save().map_err(|e| e.to_string())?;
            cr.translate(24.0, 76.0);
            cr.scale(factor, factor);
            cr.translate(-bounds.x1, -bounds.y1);
            canvas.render_item(*group, &cr);
            cr.restore().map_err(|e| e.to_string())?;
            cr.show_page().map_err(|e| e.to_string())?;
        }
        surface.finish();
        surface.status().map_err(|e| e.to_string())
    }
}

/// Les colonnes de cases sont explicitement autorisées par ces écrans.
pub fn make_editable(specs: &mut [ColumnSpec], code: &str, unlocked: bool) {
    for spec in specs.iter_mut().filter(|s| s.code == code) {
        spec.editable = unlocked;
    }
}
