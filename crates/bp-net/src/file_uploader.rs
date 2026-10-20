//! Publication d'un fichier de compétition (port de `file_uploader.cpp`).
//!
//! Le fichier est lu puis envoyé dans un thread vers `url/<nom échappé>` :
//! `PUT` HTTP(S) avec authentification basique, ou `STOR` FTP.

use std::path::Path;
use std::thread::JoinHandle;

use crate::netutil::{basename, uri_escape};
use crate::uploader::{Method, Transfer, UploadError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileUploader {
    url: String,
    user: Option<String>,
    passwd: Option<String>,
    www: Option<String>,
}

impl FileUploader {
    /// `FileUploader (url, user, passwd, www)` ; `www` est l'adresse publique
    /// où le fichier sera consultable (lien des annonces).
    pub fn new(url: &str, user: Option<&str>, passwd: Option<&str>, www: Option<&str>) -> Self {
        FileUploader {
            url: url.to_string(),
            user: user.map(str::to_string),
            passwd: passwd.map(str::to_string),
            www: www.map(str::to_string),
        }
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    /// `GetWWW`.
    pub fn www(&self) -> Option<&str> {
        self.www.as_deref()
    }

    /// URL complète de dépôt d'un fichier.
    pub fn full_url(&self, file_path: &str) -> String {
        format!("{}/{}", self.url, uri_escape(&basename(file_path)))
    }

    /// `UploadFile` : envoi asynchrone ; le résultat est rendu par le thread.
    pub fn upload_file(&self, file_path: &Path) -> JoinHandle<Result<(), UploadError>> {
        let me = self.clone();
        let path = file_path.to_path_buf();
        std::thread::Builder::new()
            .name("FileUploader".into())
            .spawn(move || me.upload_file_sync(&path))
            .expect("FileUploader thread")
    }

    /// Envoi synchrone (`Looper`).
    pub fn upload_file_sync(&self, file_path: &Path) -> Result<(), UploadError> {
        let data = std::fs::read(file_path).map_err(|e| {
            log::warn!("Unable to read file: {e}");
            UploadError(format!("Unable to read file: {e}"))
        })?;
        let mut transfer = Transfer::new(Method::Put, &self.full_url(&file_path.to_string_lossy()));
        transfer.body = data;
        if let (Some(u), Some(p)) = (&self.user, &self.passwd) {
            if !u.is_empty() && !p.is_empty() {
                transfer.credentials = Some((u.clone(), p.clone()));
            }
        }
        transfer.perform().map(|_| ()).inspect_err(|e| log::warn!("FileUploader: {e}"))
    }
}
