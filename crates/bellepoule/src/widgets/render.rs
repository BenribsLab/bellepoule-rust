//! Représentation des attributs dans les listes (port de
//! `Attribute::TreeStoreSet`, `GetUserImage`, `AttributeDesc::GetGType`).

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use bp_core::attribute::{self, AttrType, AttributeDesc, Look, Scope, Value};
use bp_core::filter::Layout;
use bp_core::formats::discrete;
use bp_core::i18n::tr;
use bp_core::player::{AttrId, Owner, Player};
use gtk4 as gtk;
use gtk::gdk_pixbuf::Pixbuf;

use super::players_list::{Cell, CellKind, ColumnSpec};

/// Type de cellule d'une colonne (`PlayersList::SetColumn`).
pub fn cell_kind(desc: &AttributeDesc, look: Look) -> CellKind {
    if desc.ty == AttrType::UInt && look == Look::Graphical {
        CellKind::Progress
    } else if desc.ty == AttrType::Str && look == Look::Graphical && desc.has_discrete_value() {
        CellKind::Icon
    } else if desc.ty == AttrType::Bool && look == Look::Graphical {
        CellKind::Toggle
    } else {
        CellKind::Text
    }
}

/// Colonnes d'après la disposition d'un filtre. `modifiable` : les cases à
/// cocher publiques peuvent être modifiées par l'utilisateur.
pub fn column_specs(layout: &[Layout], modifiable: bool) -> Vec<ColumnSpec> {
    layout
        .iter()
        .filter_map(|l| {
            let desc = attribute::registry().by_code(&l.code_name)?;
            let kind = cell_kind(desc, l.look);
            Some(ColumnSpec {
                code: l.code_name.clone(),
                title: tr(desc.user_name),
                look: l.look,
                kind,
                editable: kind == CellKind::Toggle && modifiable && desc.rights == attribute::Rights::Public,
            })
        })
        .collect()
}

/// Valeur d'un attribut du joueur (`owner` pour les attributs locaux).
pub fn value_of<'a>(player: &'a Player, code: &str, owner: Owner) -> Option<&'a Value> {
    let local = attribute::registry().by_code(code).map(|d| d.scope == Scope::Local).unwrap_or(false);
    if local {
        player.get(&AttrId::local(code, owner))
    } else {
        player.get(&AttrId::global(code))
    }
}

/// `GetUserImage (look)`.
pub fn user_image(desc: &AttributeDesc, value: &Value, look: Look) -> String {
    match value {
        Value::Str(s) => {
            let look = if look == Look::Graphical { Look::ShortText } else { look };
            if desc.has_discrete_value() {
                discrete::attribute_user_image(desc, value, look)
            } else if look == Look::ShortText {
                let image: String = s.chars().take(desc.short_length as usize).collect();
                if desc.short_length <= 3 || image.len() >= s.len() {
                    image
                } else {
                    format!("{image}…")
                }
            } else {
                s.clone()
            }
        }
        Value::UInt(v) => v.to_string(),
        Value::Int(v) => v.to_string(),
        Value::Bool(b) => (*b as u32).to_string(),
    }
}

thread_local! {
    static ICONS: RefCell<HashMap<PathBuf, Option<Pixbuf>>> = RefCell::new(HashMap::new());
}

/// Icône chargée (et mémorisée) depuis un fichier.
pub fn icon_from_file(path: &Path) -> Option<Pixbuf> {
    ICONS.with(|c| {
        c.borrow_mut()
            .entry(path.to_path_buf())
            .or_insert_with(|| Pixbuf::from_file_at_size(path, 48, 20).ok())
            .clone()
    })
}

/// Icône associée à une valeur discrète (`GetDiscretePixbuf`).
pub fn discrete_icon(desc: &AttributeDesc, value: &Value) -> Option<Pixbuf> {
    let xml = value.xml_image();
    if let Some(cat) = discrete::catalog() {
        if let Some(path) = cat.icon(desc.code_name, &xml) {
            return icon_from_file(&path);
        }
    }
    let d = desc.discrete_from_xml(&xml)?;
    let rel = d.icon.as_ref()?;
    icon_from_file(&bp_gtk::resources::resource_path(rel))
}

/// Contenu de la cellule d'un joueur pour une colonne.
pub fn cell_for(player: &Player, spec: &ColumnSpec, owner: Owner) -> Cell {
    let desc = attribute::registry().by_code(&spec.code);
    let value = value_of(player, &spec.code, owner);
    match (spec.kind, desc, value) {
        (CellKind::Toggle, _, v) => Cell::Toggle(v.map(Value::as_bool).unwrap_or(false)),
        (CellKind::Progress, _, v) => Cell::Progress(v.map(Value::as_int).unwrap_or(0)),
        (CellKind::Icon, Some(d), Some(v)) => Cell::Icon(discrete_icon(d, v)),
        (CellKind::Icon, _, None) => Cell::Icon(None),
        (_, Some(d), Some(v)) => Cell::Text(user_image(d, v, spec.look)),
        _ => Cell::Text(String::new()),
    }
}

/// Valeurs de tri d'une ligne.
pub fn sort_values(player: &Player, specs: &[ColumnSpec], owner: Owner) -> Vec<Option<Value>> {
    specs.iter().map(|s| value_of(player, &s.code, owner).cloned()).collect()
}

#[allow(dead_code)]
fn _unused(_: &gtk::Widget) {}
