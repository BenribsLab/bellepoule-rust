//! Publication des compétitions (port de `supervisor/publication.cpp` et
//! `Contest::Publish`) : à chaque enregistrement, le fichier de la
//! compétition est envoyé par FTP au site choisi, ou au service Escrime Info.

use bp_core::contest::{Contest, LEVEL_XML_IMAGES};
use bp_gtk::glade::Glade;
use gtk::prelude::*;
use gtk4 as gtk;

/// Colonnes de `FavoriteFTP`.
const COL_NAME: u32 = 0;
const COL_PIXBUF: u32 = 1;
const COL_FTP: u32 = 2;
const COL_USER: u32 = 3;
const COL_PASSWD: u32 = 4;
const COL_WWW: u32 = 5;

/// `Publication::Publication` : sites favoris et mise à jour des champs
/// quand un favori est choisi (`OnRemoteHostChanged`).
pub fn setup(glade: &Glade) {
    let model: gtk::ListStore = glade.get("FavoriteFTP");
    let pixbuf = gtk::gdk_pixbuf::Pixbuf::from_file(bp_gtk::resources::resource_path("glade/images/escrime_info.jpg")).ok();
    let iter = model.append();
    model.set(
        &iter,
        &[
            (COL_NAME, &"<b><big>Escrime Info  </big></b>"),
            (COL_FTP, &"escrime-info.com"),
            (COL_USER, &"GREG"),
            (COL_PASSWD, &"cul qui gratte au couché doigt qui sent au levé"),
            (COL_WWW, &"https://www.escrime-info.com"),
        ],
    );
    if let Some(p) = pixbuf {
        model.set(&iter, &[(COL_PIXBUF, &p)]);
    }
    let combo: gtk::ComboBox = glade.get("ftp_comboboxentry");
    let g = glade.clone();
    combo.connect_changed(move |c| {
        let Some(iter) = c.active_iter() else { return };
        let Some(model) = c.model() else { return };
        let get = |col: u32| model.get::<Option<String>>(&iter, col as i32).unwrap_or_default();
        g.get::<gtk::Entry>("user_entry").set_text(&get(COL_USER));
        g.get::<gtk::Entry>("passwd_entry").set_text(&get(COL_PASSWD));
        g.get::<gtk::Entry>("www").set_text(&get(COL_WWW));
    });
}

/// Paramètres saisis dans le dialogue « Publication ».
#[derive(Debug, Clone, Default)]
pub struct Target {
    pub url: String,
    pub user: String,
    pub passwd: String,
    pub www: String,
}

/// Lit le dialogue (`GetUpLoader`) : `None` sans adresse.
pub fn target(glade: &Glade) -> Option<Target> {
    let combo: gtk::ComboBox = glade.get("ftp_comboboxentry");
    let url = combo.child().and_then(|c| c.downcast::<gtk::Entry>().ok()).map(|e| e.text().to_string()).unwrap_or_default();
    if url.trim().is_empty() {
        return None;
    }
    Some(Target {
        url,
        user: glade.get::<gtk::Entry>("user_entry").text().to_string(),
        passwd: glade.get::<gtk::Entry>("passwd_entry").text().to_string(),
        www: glade.get::<gtk::Entry>("www").text().to_string(),
    })
}

/// `Contest::Publish` : envoi en arrière-plan du fichier enregistré.
pub fn publish(target: &Target, contest: &Contest) {
    let Some(file) = contest.filename.clone() else { return };
    let www = (!target.www.is_empty()).then_some(target.www.as_str());
    if target.url.contains("escrime-info.com") {
        let mut uploader = bp_net::greg_uploader::GregUploader::new(&target.url, www);
        uploader.set_team_event(contest.settings.team_event);
        uploader.set_date(contest.day, contest.month, contest.year);
        uploader.set_location(Some(&contest.location), Some(&contest.label));
        uploader.set_level(Some(LEVEL_XML_IMAGES[contest.level.min(6)]));
        uploader.set_division(Some(""));
        uploader.set_running_state(contest.schedule.is_over());
        uploader.set_weapon(Some(contest.weapon.greg_image));
        uploader.set_gender(contest.gender as u32);
        uploader.set_category(Some(&contest.category.xml_image()));
        let _ = uploader.upload_file(&file);
    } else {
        let user = (!target.user.is_empty()).then_some(target.user.as_str());
        let passwd = (!target.passwd.is_empty()).then_some(target.passwd.as_str());
        let uploader = bp_net::file_uploader::FileUploader::new(&target.url, user, passwd, www);
        let _ = uploader.upload_file(&file);
    }
}
