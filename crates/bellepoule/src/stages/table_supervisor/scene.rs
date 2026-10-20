//! Dessin partagé entre le tableau à l'écran et ses impressions.

use bp_core::attribute::{self, Look};
use bp_core::player::{Owner, PlayerRef, PlayerStore};
use bp_core::rounds::table::{
    layout::{LayoutMetrics, TableSetDrawing, TableStatus},
    table_set::TableSet,
};
use bp_gtk::canvas::{Anchor, Canvas, Prop};
use gtk4 as gtk;

use super::Snapshot;
use crate::widgets::render;

pub fn identity(
    players: &PlayerStore,
    player: PlayerRef,
    columns: &[String],
    owner: Owner,
    compact: bool,
) -> String {
    let Some(p) = players.get(player) else {
        return String::new();
    };
    let compact_columns = ["name".to_string(), "first_name".to_string()];
    let columns = if compact {
        &compact_columns[..]
    } else {
        columns
    };
    columns
        .iter()
        .filter_map(|code| {
            let desc = attribute::registry().by_code(code)?;
            let value = render::value_of(p, code, owner)?;
            Some(render::user_image(desc, value, Look::LongText))
        })
        .collect::<Vec<_>>()
        .join("  ")
}

pub fn draw(
    canvas: &Canvas,
    set: &TableSet,
    snapshot: &Snapshot,
    metrics: &LayoutMetrics,
    interactive: bool,
) -> TableSetDrawing {
    let players = &snapshot.players;
    let referees = &snapshot.referees;
    let columns = &snapshot.base.filter;
    let owner = snapshot.base.owner();
    let drawing = set.drawing_with_referees(players, referees, metrics);
    let root = canvas.root();
    for connector in &drawing.connectors {
        let line = canvas.polyline(root, false, &connector.points);
        canvas.set(
            line,
            [
                Prop::StrokeColor("#404040".into()),
                Prop::LineWidth(connector.line_width),
            ],
        );
    }
    for header in &drawing.headers {
        let status = match header.status {
            Some(TableStatus::Over) => "✓ ",
            Some(TableStatus::Error) => "! ",
            _ => "",
        };
        let item = canvas.text(
            root,
            &format!("{status}{}", header.image),
            header.bounds.x1,
            0.0,
            metrics.cell_width - 25.0,
            Anchor::NW,
        );
        canvas.set(
            item,
            [
                Prop::Font("Sans Bold 15px".into()),
                Prop::Ellipsize(gtk::pango::EllipsizeMode::End),
            ],
        );
        if header.print_icon && interactive {
            let icon = canvas.text(root, "⎙", header.bounds.x2 - 24.0, 0.0, 24.0, Anchor::NW);
            canvas.set(
                icon,
                [
                    Prop::Font("Sans 20px".into()),
                    Prop::Tooltip(bp_core::i18n::tr("Print score sheets")),
                ],
            );
        }
    }
    for cell in &drawing.cells {
        if cell.print_icon && interactive {
            let (x, y) = print_origin(cell);
            let icon = canvas.text(root, "⎙", x, y, 22.0, Anchor::NW);
            canvas.set(icon, [Prop::Font("Sans 16px".into())]);
        }
        if let Some(player) = cell.winner {
            let name = identity(players, player, columns, owner, cell.compact);
            let markup = identity_markup(players, player, columns, owner, cell.compact);
            let width =
                cell.bounds.x2 - cell.bounds.x1 - if cell.score.is_some() { 52.0 } else { 4.0 };
            let item = canvas.text(
                root,
                &markup,
                cell.bounds.x1 + 3.0,
                cell.bounds.y1 + 2.0,
                width,
                Anchor::NW,
            );
            canvas.set(
                item,
                [
                    Prop::Font("Sans 14px".into()),
                    Prop::UseMarkup(true),
                    Prop::Ellipsize(gtk::pango::EllipsizeMode::End),
                    Prop::Tooltip(name),
                ],
            );
        }
        if let Some(label) = &cell.match_label {
            let name = canvas.text(
                root,
                &label.name,
                label.bounds.x1 + 4.0,
                label.bounds.y1,
                100.0,
                Anchor::NW,
            );
            canvas.set(
                name,
                [
                    Prop::Font("Sans Italic 10px".into()),
                    Prop::FillColor("#606060".into()),
                ],
            );
            if let Some(roadmap) = &label.roadmap {
                let text = format!(
                    "{}  {}",
                    roadmap.text,
                    roadmap
                        .referees
                        .iter()
                        .map(|(_, name)| name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                let item = canvas.text(
                    root,
                    &text,
                    label.bounds.x1 + 4.0,
                    label.bounds.y1 + 15.0,
                    metrics.cell_width - 10.0,
                    Anchor::NW,
                );
                canvas.set(
                    item,
                    [
                        Prop::Font("Sans Italic 10px".into()),
                        Prop::FillColor("darkgreen".into()),
                        Prop::Ellipsize(gtk::pango::EllipsizeMode::End),
                        Prop::Tooltip(text),
                    ],
                );
            }
        }
        if let Some(score) = &cell.score {
            if score.visible {
                let b = score.bounds;
                let rect = canvas.rect(root, b.x1, b.y1, b.x2 - b.x1, b.y2 - b.y1);
                let error = set.bout(score.bout_node).is_some_and(|m| m.has_error());
                canvas.set(
                    rect,
                    [
                        Prop::FillColor(if error { "#ffe0e0" } else { "white" }.into()),
                        Prop::StrokeColor(if error { "red" } else { "#808080" }.into()),
                        Prop::LineWidth(0.7),
                    ],
                );
                if !interactive {
                    let text = score
                        .drop_reason
                        .map(|c| c.to_string())
                        .unwrap_or_else(|| score.text.clone());
                    let item =
                        canvas.text(root, &text, b.x1 + 8.0, b.y1, b.x2 - b.x1 - 8.0, Anchor::NW);
                    canvas.set(item, [Prop::Font("Sans 14px".into())]);
                }
            }
        }
    }
    // Les tableaux non encore remplis conservent leur étendue de défilement.
    let extent = canvas.rect(root, 0.0, 0.0, drawing.width, drawing.height);
    canvas.set(
        extent,
        [Prop::NoFill, Prop::NoStroke, Prop::PointerEvents(false)],
    );
    drawing
}

fn identity_markup(
    players: &PlayerStore,
    player: PlayerRef,
    columns: &[String],
    owner: Owner,
    compact: bool,
) -> String {
    let Some(p) = players.get(player) else {
        return String::new();
    };
    let codes: Vec<&str> = if compact {
        vec!["name", "first_name"]
    } else {
        columns.iter().map(String::as_str).collect()
    };
    codes
        .into_iter()
        .filter_map(|code| {
            let desc = attribute::registry().by_code(code)?;
            let value = render::value_of(p, code, owner)?;
            let text =
                gtk::glib::markup_escape_text(&render::user_image(desc, value, Look::LongText))
                    .to_string();
            Some(match code {
                "name" => format!("<b><span foreground='darkblue'>{text}</span></b>"),
                "first_name" => format!("<span foreground='darkblue'>{text}</span>"),
                "club" | "league" | "region" | "country" => {
                    format!("<i><span foreground='dimgrey'>{text}</span></i>")
                }
                _ => text,
            })
        })
        .collect::<Vec<_>>()
        .join("  ")
}

pub fn print_origin(cell: &bp_core::rounds::table::NodeCell) -> (f64, f64) {
    cell.match_label
        .as_ref()
        .map(|label| (label.bounds.x2 - 24.0, label.bounds.y1))
        .unwrap_or((cell.bounds.x1 + 2.0, cell.bounds.y1))
}
