//! Documents de listes : mêmes cellules pour l'impression et l'export PDF.

use bp_core::player::{AttrId, Owner, PlayerRef, PlayerStore};
use bp_core::stage::RankEntry;
use bp_gtk::canvas::{Canvas, ItemId, Prop};
use gtk4 as gtk;
use std::path::Path;

use super::{
    players_list::{Cell, ColumnSpec, RowData},
    render,
};

#[derive(Clone)]
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
        self.scene_chunked(25)
    }

    /// Une table par page de `per_page` lignes, en-têtes répétés.
    fn scene_chunked(&self, per_page: usize) -> (Canvas, Vec<ItemId>) {
        let per_page = per_page.max(1);
        let canvas = Canvas::new();
        let mut groups = Vec::new();
        // Les sous-lignes (sections d'un classement) sont imprimées à la suite.
        fn flatten(rows: &[RowData], out: &mut Vec<RowData>) {
            for r in rows {
                out.push(RowData { children: Vec::new(), ..r.clone() });
                flatten(&r.children, out);
            }
        }
        let mut all = Vec::new();
        flatten(&self.rows, &mut all);
        // Les en-têtes sont répétés à chaque page.
        for (page, rows) in all.chunks(per_page).enumerate() {
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
                    // Titre de section (classement d'un groupe…).
                    if data.player == 0 && col == 0 {
                        canvas.set(item, [Prop::Font("Sans Bold 15px".into())]);
                    }
                    if col == 0
                        && self.classification_borders
                        && bp_core::contest::html::is_power_of_two_border(page * per_page + row)
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
        let document = self.clone();
        bp_gtk::print::run_document(parent, &self.title, move || document.source())?;
        Ok(())
    }

    /// Pages de la liste, pour l'assembler avec d'autres documents : autant
    /// de lignes par page que la hauteur le permet.
    pub fn source(&self) -> Box<dyn bp_gtk::print::PageSource> {
        Box::new(ListPages { document: self.clone(), canvas: Canvas::new(), groups: Vec::new(), scale: 1.0 })
    }

    /// Tableau HTML de la liste (titres de section sur une ligne entière).
    pub fn to_html(&self) -> String {
        let esc = |t: &str| gtk::glib::markup_escape_text(t).to_string();
        let mut html = String::from("<table class=\"list\">\n<tr>");
        for spec in &self.specs {
            html.push_str(&format!("<th>{}</th>", esc(&spec.title)));
        }
        html.push_str("</tr>\n");
        fn rows(out: &mut String, list: &[RowData], columns: usize, esc: &dyn Fn(&str) -> String) {
            for r in list {
                let texts: Vec<String> = r
                    .cells
                    .iter()
                    .map(|c| match c {
                        Cell::Text(s) => s.clone(),
                        Cell::Toggle(v) => if *v { "✓".into() } else { String::new() },
                        Cell::Progress(v) => v.to_string(),
                        _ => String::new(),
                    })
                    .collect();
                if r.player == 0 {
                    out.push_str(&format!(
                        "<tr class=\"section\"><th colspan=\"{columns}\" style=\"text-align:left;padding-top:1em\">{}</th></tr>\n",
                        esc(texts.first().map(String::as_str).unwrap_or(""))
                    ));
                } else {
                    out.push_str("<tr>");
                    for t in &texts {
                        out.push_str(&format!("<td>{}</td>", esc(t)));
                    }
                    out.push_str("</tr>\n");
                }
                rows(out, &r.children, columns, esc);
            }
        }
        rows(&mut html, &self.rows, self.specs.len(), &esc);
        html.push_str("</table>\n");
        html
    }

    /// PDF avec la même mise en page que l'impression.
    pub fn export_pdf(&self, path: &Path) -> Result<(), String> {
        bp_gtk::print::export_pdf(self.source().as_mut(), path)
    }
}

/// Les colonnes de cases sont explicitement autorisées par ces écrans.
pub fn make_editable(specs: &mut [ColumnSpec], code: &str, unlocked: bool) {
    for spec in specs.iter_mut().filter(|s| s.code == code) {
        spec.editable = unlocked;
    }
}

/// Liste paginée selon la place disponible sur la feuille.
struct ListPages {
    document: ListDocument,
    canvas: Canvas,
    groups: Vec<ItemId>,
    scale: f64,
}

impl bp_gtk::print::PageSource for ListPages {
    fn begin(&mut self, width: f64, height: f64) -> usize {
        let count: usize = {
            fn count(rows: &[RowData]) -> usize {
                rows.iter().map(|r| 1 + count(&r.children)).sum()
            }
            count(&self.document.rows)
        };
        // Hauteur d'une ligne et de l'en-tête, mesurées sur une seule table.
        let (canvas, groups) = self.document.scene_chunked(count.max(1));
        let all = groups.first().map(|g| canvas.bounds(*g)).unwrap_or_default();
        let one = {
            let (c, g) = self.document.scene_chunked(1);
            g.first().map(|g| c.bounds(*g)).unwrap_or_default()
        };
        let row_h = if count > 1 { ((all.height() - one.height()) / (count - 1) as f64).max(1.0) } else { one.height().max(1.0) };
        let header_h = (one.height() - row_h).max(0.0);
        self.scale = (width / all.width().max(1.0)).min(1.0);
        // Un peu trop long pour une page : on réduit légèrement plutôt que de
        // reporter quelques lignes sur la page suivante.
        if all.height() * self.scale > height && all.height() * self.scale * 0.8 <= height {
            self.scale = height / all.height();
            self.canvas = canvas;
            self.groups = groups;
            return 1;
        }
        let per_page = (((height / self.scale) - header_h) / row_h).floor().max(5.0) as usize;
        let (canvas, groups) = self.document.scene_chunked(per_page);
        self.canvas = canvas;
        self.groups = groups;
        self.groups.len().max(1)
    }

    fn draw(&mut self, page: usize, cr: &cairo::Context, _width: f64, _height: f64) {
        let Some(group) = self.groups.get(page) else { return };
        let bounds = self.canvas.bounds(*group);
        let _ = cr.save();
        cr.scale(self.scale, self.scale);
        cr.translate(-bounds.x1, -bounds.y1);
        self.canvas.render_item(*group, cr);
        let _ = cr.restore();
    }

    fn page_name(&self, _page: usize) -> String {
        if self.document.subtitle.is_empty() {
            self.document.title.clone()
        } else {
            self.document.subtitle.clone()
        }
    }
}

