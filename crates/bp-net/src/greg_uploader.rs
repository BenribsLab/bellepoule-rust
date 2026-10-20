//! Publication sur GREG / escrime-info (port de `greg_uploader.cpp`).
//!
//! Deux requêtes `POST` multipart successives (agent `BellePoule/5.0`) :
//! 1. `envoyer.php?fichier=<nom>` avec le fichier dans le champ `ficXML` ;
//! 2. `envoyer.php?fichier=./temp/<nom>&valide=1` avec les champs décrivant
//!    la compétition (`type_competition`, `indivouequipe`, `status`,
//!    `dp-normal-1`, `lieu`, `niveau`, `Division`, `arme`, `genre`,
//!    `categorie`).
//!
//! Chaque requête se termine par le champ `submit=send`. L'envoi n'a lieu
//! que si un lieu a été renseigné. Les échanges passent en HTTPS.

use std::collections::BTreeMap;
use std::path::Path;
use std::thread::JoinHandle;

use crate::netutil::{basename, uri_escape};
use crate::uploader::{FormPart, Method, Transfer, UploadError};

/// Adresse du script de réception de GREG.
pub const GREG_URL: &str = "https://www.escrime-info.com/gregxml/Greg/envoyer.php";
/// Agent HTTP annoncé.
pub const USER_AGENT: &str = "BellePoule/5.0";

const GENDER_GREG: [&str; 3] = ["H", "D", "H"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GregUploader {
    url: String,
    www: Option<String>,
    service: String,
    has_location: bool,
    remote_file: Option<String>,
    form_fields: BTreeMap<String, String>,
}

impl GregUploader {
    pub fn new(url: &str, www: Option<&str>) -> Self {
        GregUploader {
            url: url.to_string(),
            www: www.map(str::to_string),
            service: GREG_URL.to_string(),
            has_location: false,
            remote_file: None,
            form_fields: BTreeMap::new(),
        }
    }

    /// Change l'adresse du script (serveur de test).
    pub fn set_service_url(&mut self, url: &str) {
        self.service = url.to_string();
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn www(&self) -> Option<&str> {
        self.www.as_deref()
    }

    pub fn form_fields(&self) -> &BTreeMap<String, String> {
        &self.form_fields
    }

    fn set(&mut self, key: &str, value: Option<String>) {
        match value {
            Some(v) => {
                self.form_fields.insert(key.to_string(), v);
            }
            None => {
                self.form_fields.remove(key);
            }
        }
    }

    pub fn set_team_event(&mut self, is_team_event: bool) {
        if is_team_event {
            self.set("type_competition", Some("CompetitionParEquipes".into()));
            self.set("indivouequipe", Some("equipe".into()));
        } else {
            self.set("type_competition", Some("CompetitionIndividuelle".into()));
            self.set("indivouequipe", Some("indiv".into()));
        }
    }

    pub fn set_running_state(&mut self, is_over: bool) {
        self.set("status", Some(if is_over { "D" } else { "L" }.into()));
    }

    pub fn set_date(&mut self, day: u32, month: u32, year: u32) {
        self.set("dp-normal-1", Some(format!("{day:02}.{month:02}.{year}")));
    }

    /// `lieu` = `lieu[libellé]` ; un lieu non vide autorise l'envoi.
    pub fn set_location(&mut self, location: Option<&str>, label: Option<&str>) {
        let mut lieu = None;
        if let Some(loc) = location.filter(|l| !l.is_empty()) {
            self.has_location = true;
            let mut s = loc.to_string();
            if let Some(label) = label.filter(|l| !l.is_empty()) {
                s.push('[');
                s.push_str(label);
                s.push(']');
            }
            lieu = Some(s);
        }
        self.set("lieu", lieu);
    }

    pub fn set_level(&mut self, level: Option<&str>) {
        self.set("niveau", level.map(str::to_string));
    }

    pub fn set_division(&mut self, division: Option<&str>) {
        self.set("Division", division.map(str::to_string));
    }

    pub fn set_weapon(&mut self, weapon: Option<&str>) {
        self.set("arme", weapon.map(str::to_string));
    }

    /// Genre BellePoule (0 = masculin, 1 = féminin, 2 = mixte) → `H`/`D`/`H`.
    pub fn set_gender(&mut self, gender: u32) {
        self.set("genre", Some(GENDER_GREG.get(gender as usize).copied().unwrap_or("H").into()));
    }

    pub fn set_category(&mut self, category: Option<&str>) {
        self.set("categorie", category.map(str::to_string));
    }

    pub fn has_location(&self) -> bool {
        self.has_location
    }

    /// URL des deux étapes pour un fichier donné.
    pub fn job_urls(&self, remote_file: &str) -> [String; 2] {
        let escaped = uri_escape(remote_file);
        [
            format!("{}?fichier={escaped}", self.service),
            format!("{}?fichier=./temp/{escaped}&valide=1", self.service),
        ]
    }

    /// `UploadFile` : rien n'est envoyé sans lieu (renvoie `None`).
    pub fn upload_file(&mut self, file_path: &Path) -> Option<JoinHandle<Result<(), UploadError>>> {
        if !self.has_location {
            return None;
        }
        if self.remote_file.is_none() {
            self.remote_file = Some(basename(&file_path.to_string_lossy()));
        }
        let me = self.clone();
        let path = file_path.to_path_buf();
        std::thread::Builder::new().name("GregUploader".into()).spawn(move || me.looper(&path)).ok()
    }

    /// `Looper` : les deux requêtes, l'une après l'autre.
    fn looper(&self, file_path: &Path) -> Result<(), UploadError> {
        let remote = self.remote_file.clone().unwrap_or_default();
        let urls = self.job_urls(&remote);
        let content = std::fs::read(file_path).map_err(|e| UploadError(e.to_string()))?;
        let mut result = Ok(());
        for (job, url) in urls.iter().enumerate() {
            let mut t = Transfer::new(Method::Post, url);
            t.user_agent = Some(USER_AGENT.into());
            if job == 0 {
                t.form.push(FormPart::File { name: "ficXML".into(), filename: remote.clone(), content: content.clone() });
            } else {
                for (k, v) in &self.form_fields {
                    t.form.push(FormPart::Field { name: k.clone(), value: v.clone() });
                }
            }
            t.form.push(FormPart::Field { name: "submit".into(), value: "send".into() });
            if let Err(e) = t.perform() {
                log::warn!("GregUploader: {e}");
                result = Err(e);
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields() {
        let mut g = GregUploader::new("escrime-info.com", Some("https://www.escrime-info.com"));
        g.set_team_event(false);
        g.set_date(3, 7, 2026);
        g.set_gender(1);
        assert!(g.upload_file(Path::new("x.cotcot")).is_none());
        g.set_location(Some("Paris"), Some("Coupe"));
        assert!(g.has_location());
        let f = g.form_fields();
        assert_eq!(f["lieu"], "Paris[Coupe]");
        assert_eq!(f["dp-normal-1"], "03.07.2026");
        assert_eq!(f["genre"], "D");
        assert_eq!(f["indivouequipe"], "indiv");
        let urls = g.job_urls("a b.cotcot");
        assert_eq!(urls[1], format!("{GREG_URL}?fichier=./temp/a%20b.cotcot&valide=1"));
    }
}
