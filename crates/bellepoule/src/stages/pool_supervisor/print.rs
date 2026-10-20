//! Feuilles arbitres, affichage des résultats et classements en cairo.

use bp_core::rounds::pool::pool::PoolSheet;
use bp_gtk::canvas::{Anchor, Canvas, ItemId, Prop};
use gtk4 as gtk;

fn table(canvas: &Canvas, parent: ItemId) -> ItemId {
    let table = canvas.table(parent);
    canvas.set(
        table,
        [
            Prop::ColumnSpacing(10.0),
            Prop::RowSpacing(5.0),
            Prop::HorzGridLineWidth(0.5),
            Prop::VertGridLineWidth(0.5),
        ],
    );
    table
}

pub fn sheets(
    parent: Option<&gtk::Window>,
    title: &str,
    sheets: Vec<PoolSheet>,
    density: (u32, u32),
    watermark: bool,
) -> Result<(), gtk::glib::Error> {
    let canvas = Canvas::new();
    let mut groups = Vec::new();
    for sheet in sheets {
        let group = canvas.group(canvas.root());
        let details = format!(
            "{}   {}   {}",
            sheet.title,
            sheet.piste.as_deref().unwrap_or(""),
            sheet
                .referee
                .as_ref()
                .map(|(n, f)| format!("{} {}", n, f.as_deref().unwrap_or("")))
                .unwrap_or_default()
        );
        let heading = canvas.text(group, &details, 0.0, 0.0, -1.0, Anchor::NW);
        canvas.set(heading, [Prop::Font("Sans Bold 18px".into())]);
        let grid = table(&canvas, group);
        canvas.set_simple_transform(grid, 0.0, 30.0, 1.0, 0.0);
        let n = sheet.rows.len();
        for j in 0..n {
            canvas.put_text_in_table(grid, &(j + 1).to_string(), 0, j as u32 + 3);
        }
        for (j, heading) in sheet.headers.iter().enumerate() {
            canvas.put_text_in_table(grid, heading, 0, (n + 3 + j) as u32);
        }
        for (i, row) in sheet.rows.iter().enumerate() {
            let r = i as u32 + 1;
            canvas.put_text_in_table(grid, &row.position.to_string(), r, 0);
            canvas.put_text_in_table(
                grid,
                &format!("{} {}", row.identity.name, row.identity.first_name),
                r,
                1,
            );
            canvas.put_text_in_table(grid, &row.identity.club, r, 2);
            for (j, cell) in row.cells.iter().enumerate() {
                canvas.put_text_in_table(
                    grid,
                    if cell.grey {
                        "  ×  "
                    } else if sheet.for_referees && !watermark {
                        "       "
                    } else {
                        &cell.text
                    },
                    r,
                    j as u32 + 3,
                );
            }
            if let Some(d) = &row.dashboard {
                for (j, text) in [
                    &d.status.to_string(),
                    &d.victories,
                    &d.victories_ratio,
                    &d.hits_scored,
                    &d.hits_received,
                    &d.indice,
                    &d.rank,
                ]
                .iter()
                .enumerate()
                {
                    canvas.put_text_in_table(grid, text, r, (n + 3 + j) as u32);
                }
            } else {
                canvas.put_text_in_table(grid, "                    ", r, (n + 3) as u32);
            }
        }
        if sheet.for_referees {
            let bottom = canvas.bounds(grid).y2 + 20.0;
            let bouts = table(&canvas, group);
            canvas.set_simple_transform(bouts, 0.0, bottom, 1.0, 0.0);
            for (i, bout) in sheet.matches.iter().enumerate() {
                let row = i as u32;
                canvas.put_text_in_table(
                    bouts,
                    &format!("{}{}", bout.name, if bout.rest_error { " ⏱" } else { "" }),
                    row,
                    0,
                );
                for (j, fencer) in bout.opponents.iter().enumerate() {
                    if let Some(f) = fencer {
                        canvas.put_text_in_table(
                            bouts,
                            &format!(
                                "{}  {} {}",
                                f.position, f.identity.name, f.identity.first_name
                            ),
                            row,
                            j as u32 * 2 + 1,
                        );
                        let boxes = if bout.score_boxes > 0 {
                            (1..=bout.score_boxes)
                                .map(|v| format!("□{v}"))
                                .collect::<Vec<_>>()
                                .join(" ")
                        } else {
                            "________".into()
                        };
                        canvas.put_text_in_table(bouts, &boxes, row, j as u32 * 2 + 2);
                    }
                }
            }
        }
        groups.push(group);
    }
    bp_gtk::print::run_tiled(
        parent,
        title,
        "",
        canvas,
        groups,
        density.0 as usize,
        density.1 as usize,
    )
}

pub fn classification(
    parent: Option<&gtk::Window>,
    title: &str,
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
) -> Result<(), gtk::glib::Error> {
    let canvas = Canvas::new();
    let mut groups = Vec::new();
    for chunk in rows.chunks(25) {
        let grid = table(&canvas, canvas.root());
        for (j, header) in headers.iter().enumerate() {
            let item = canvas.put_text_in_table(grid, header, 0, j as u32);
            canvas.set(item, [Prop::Font("Sans Bold 14px".into())]);
        }
        for (i, row) in chunk.iter().enumerate() {
            for (j, text) in row.iter().enumerate() {
                canvas.put_text_in_table(grid, text, i as u32 + 1, j as u32);
            }
        }
        groups.push(grid);
    }
    bp_gtk::print::run_tiled(parent, title, "", canvas, groups, 1, 1)
}
