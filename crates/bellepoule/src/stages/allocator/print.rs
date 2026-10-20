//! Impression de la composition (deux colonnes, sans couper une poule)
//! ou de la liste affichée. Les mêmes cellules alimentent GTK et cairo.

use bp_core::i18n::tr;
use bp_gtk::canvas::{Anchor, Canvas, ItemId, Prop};
use gtk4 as gtk;

use super::{ListCell, Mark, Snapshot};

fn cell(
    canvas: &Canvas,
    table: ItemId,
    value: &ListCell,
    row: u32,
    col: u32,
    absent: bool,
    referee: bool,
) {
    if let ListCell::Icon(pixbuf) = value {
        canvas.put_pixbuf_in_table(table, pixbuf.as_ref(), row, col);
        return;
    }
    let text = match value {
        ListCell::Text(s) => s.clone(),
        ListCell::Toggle(b) => {
            if *b {
                "✓".into()
            } else {
                String::new()
            }
        }
        ListCell::Progress(n) => n.to_string(),
        _ => unreachable!(),
    };
    let escaped = gtk::glib::markup_escape_text(&text);
    let markup = if absent {
        format!("<s>{escaped}</s>")
    } else if referee {
        format!("<b><i>{escaped}</i></b>")
    } else {
        escaped.to_string()
    };
    let item = canvas.put_text_in_table(table, &markup, row, col);
    canvas.set(item, [Prop::UseMarkup(true)]);
}

pub(super) fn run(
    parent: Option<&gtk::Window>,
    title: &str,
    snapshot: Snapshot,
    list: bool,
) -> Result<(), gtk::glib::Error> {
    let canvas = Canvas::new();
    let mut groups = Vec::new();
    if list {
        // Pagination par lignes, les en-têtes sont répétés sur chaque feuille.
        for rows in snapshot.list_rows.chunks(25) {
            let table = canvas.table(canvas.root());
            canvas.set(table, [Prop::ColumnSpacing(10.0), Prop::RowSpacing(4.0)]);
            for (col, spec) in snapshot.list_specs.iter().enumerate() {
                let item = canvas.put_text_in_table(table, &spec.title, 0, col as u32);
                canvas.set(item, [Prop::Font("Sans Bold 14px".into())]);
            }
            for (row, data) in rows.iter().enumerate() {
                for (col, value) in data.cells.iter().enumerate() {
                    cell(
                        &canvas,
                        table,
                        value,
                        row as u32 + 1,
                        col as u32,
                        data.dimmed,
                        false,
                    );
                }
            }
            groups.push(table);
        }
    } else {
        for pool in &snapshot.pools {
            let group = canvas.group(canvas.root());
            let details = [pool.piste.as_deref(), pool.time.as_deref()]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join("   ");
            let title = canvas.text(
                group,
                &format!("{}   {details}", pool.name),
                0.0,
                0.0,
                -1.0,
                Anchor::NW,
            );
            canvas.set(title, [Prop::Font("Sans Bold 18px".into())]);
            let table = canvas.table(group);
            canvas.set(table, [Prop::ColumnSpacing(10.0), Prop::RowSpacing(3.0)]);
            canvas.set_simple_transform(table, 0.0, 30.0, 1.0, 0.0);
            for (row, referee) in pool.referees.iter().enumerate() {
                canvas.put_icon_in_table(
                    table,
                    &bp_gtk::resources::image_path("referee.png"),
                    row as u32,
                    0,
                );
                for (col, value) in referee.iter().enumerate() {
                    cell(
                        &canvas,
                        table,
                        value,
                        row as u32,
                        col as u32 + 1,
                        false,
                        true,
                    );
                }
            }
            for (row, fencer) in pool.fencers.iter().enumerate() {
                let row = (row + pool.referees.len()) as u32;
                let status = match fencer.mark {
                    Mark::Error => "!",
                    Mark::Moved => "↪",
                    Mark::None => "",
                };
                canvas.put_text_in_table(table, status, row, 0);
                for (col, value) in fencer.cells.iter().enumerate() {
                    cell(
                        &canvas,
                        table,
                        value,
                        row,
                        col as u32 + 1,
                        fencer.absent,
                        false,
                    );
                }
            }
            groups.push(group);
        }
    }
    let subtitle = snapshot
        .selected
        .and_then(|i| snapshot.configs.get(i))
        .map(|(size, count)| {
            format!(
                "{}: {}    {}: {}",
                tr("Number of pools"),
                count,
                tr("Fencers / pools"),
                size
            )
        })
        .unwrap_or_default();
    bp_gtk::print::run_tiled(
        parent,
        title,
        &subtitle,
        canvas,
        groups,
        0,
        if list { 1 } else { 2 },
    )
}
