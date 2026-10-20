//! Icônes : images de `resources/glade/images`, puis thème de l'interface.

use gtk4 as gtk;
use gtk::gdk_pixbuf::Pixbuf;
use gtk::prelude::*;

/// Icône `name` à la taille `size` : un fichier `glade/images/<name>.png`
/// s'il existe, sinon l'icône du thème. `None` si rien n'est trouvé ou si
/// aucun écran n'est disponible.
pub fn icon_pixbuf(name: &str, size: i32) -> Option<Pixbuf> {
    let file = crate::resources::image_path(&format!("{name}.png"));
    if file.is_file() {
        if let Ok(p) = Pixbuf::from_file_at_size(&file, size, size) {
            return Some(p);
        }
    }
    let display = gtk::gdk::Display::default()?;
    let theme = gtk::IconTheme::for_display(&display);
    if !theme.has_icon(name) {
        return None;
    }
    let paintable = theme.lookup_icon(name, &[], size, 1, gtk::TextDirection::None, gtk::IconLookupFlags::empty());
    let file = paintable.file()?;
    let path = file.path()?;
    Pixbuf::from_file_at_size(path, size, size).ok()
}
