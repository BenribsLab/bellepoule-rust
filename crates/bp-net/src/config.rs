//! Conservation de la phrase secrète de l'anneau.
//!
//! Le C++ range la phrase dans `config.ini` de l'application (groupe `[Ring]`,
//! clé `passphrase`) et lit, pour les identifiants de secours, celle de
//! l'application partenaire installée sur le même poste (`BellePoule` pour
//! le Marshaller, `BellePoule2D` pour BellePoule). L'application peut fournir
//! sa propre implémentation de [`PassphraseStore`] (pour partager sa
//! configuration en mémoire) ou utiliser [`UserConfigStore`].

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::message::{escape, unescape};
use crate::netutil::user_config_dir;
use crate::ring::Role;

/// Accès à la phrase secrète.
pub trait PassphraseStore: Send + Sync {
    /// Phrase mémorisée par cette application.
    fn load(&self) -> Option<String>;
    /// Mémorise la phrase de cette application.
    fn save(&self, passphrase: &str);
    /// Phrase de l'application partenaire sur ce poste (identifiants de secours).
    fn load_partner(&self) -> Option<String>;
}

/// Nom public de l'application selon son rôle dans l'anneau.
pub fn app_name(role: Role) -> &'static str {
    match role {
        Role::ResourceUser => "BellePoule",
        Role::ResourceManager => "BellePoule2D",
    }
}

/// Nom de l'application partenaire (`RetreiveBackupCredentials`).
pub fn partner_app_name(role: Role) -> &'static str {
    match role {
        Role::ResourceUser => "BellePoule2D",
        Role::ResourceManager => "BellePoule",
    }
}

/// Fichier `config.ini` minimal au format GKeyFile, qui préserve les lignes
/// qu'il ne modifie pas (commentaires, autres groupes).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KeyFile {
    lines: Vec<String>,
}

impl KeyFile {
    pub fn parse(text: &str) -> Self {
        KeyFile { lines: text.lines().map(|l| l.trim_end_matches('\r').to_string()).collect() }
    }

    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path).map(|t| Self::parse(&t)).unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, self.to_data())
    }

    pub fn to_data(&self) -> String {
        let mut s = self.lines.join("\n");
        s.push('\n');
        s
    }

    /// Indice de la ligne `[group]`, et fin du groupe (exclue).
    fn group_range(&self, group: &str) -> Option<(usize, usize)> {
        let header = format!("[{group}]");
        let start = self.lines.iter().position(|l| l.trim() == header)?;
        let end = self.lines[start + 1..]
            .iter()
            .position(|l| l.trim_start().starts_with('['))
            .map(|i| start + 1 + i)
            .unwrap_or(self.lines.len());
        Some((start, end))
    }

    pub fn get(&self, group: &str, key: &str) -> Option<String> {
        let (start, end) = self.group_range(group)?;
        self.lines[start + 1..end].iter().find_map(|l| {
            let eq = l.find('=')?;
            (l[..eq].trim() == key).then(|| unescape(l[eq + 1..].trim_start()))
        })
    }

    pub fn set(&mut self, group: &str, key: &str, value: &str) {
        let line = format!("{key}={}", escape(value));
        match self.group_range(group) {
            Some((start, end)) => {
                for i in start + 1..end {
                    if let Some(eq) = self.lines[i].find('=') {
                        if self.lines[i][..eq].trim() == key {
                            self.lines[i] = line;
                            return;
                        }
                    }
                }
                // insérer après la dernière ligne non vide du groupe
                let mut at = end;
                while at > start + 1 && self.lines[at - 1].trim().is_empty() {
                    at -= 1;
                }
                self.lines.insert(at, line);
            }
            None => {
                if self.lines.last().is_some_and(|l| !l.trim().is_empty()) {
                    self.lines.push(String::new());
                }
                self.lines.push(format!("[{group}]"));
                self.lines.push(line);
            }
        }
    }
}

/// Phrase secrète lue et écrite dans `<config>/<application>/config.ini`.
#[derive(Debug)]
pub struct UserConfigStore {
    own: PathBuf,
    partner: PathBuf,
    lock: Mutex<()>,
}

impl UserConfigStore {
    pub fn new(app_name: &str, partner_app_name: &str) -> Self {
        let dir = user_config_dir();
        Self::with_paths(dir.join(app_name).join("config.ini"), dir.join(partner_app_name).join("config.ini"))
    }

    /// Magasin standard pour un rôle donné.
    pub fn for_role(role: Role) -> Self {
        Self::new(app_name(role), partner_app_name(role))
    }

    pub fn with_paths(own: PathBuf, partner: PathBuf) -> Self {
        UserConfigStore { own, partner, lock: Mutex::new(()) }
    }
}

impl PassphraseStore for UserConfigStore {
    fn load(&self) -> Option<String> {
        let _g = self.lock.lock().unwrap();
        KeyFile::load(&self.own).get("Ring", "passphrase")
    }

    fn save(&self, passphrase: &str) {
        let _g = self.lock.lock().unwrap();
        let mut kf = KeyFile::load(&self.own);
        kf.set("Ring", "passphrase", passphrase);
        if let Err(e) = kf.save(&self.own) {
            log::error!("g_file_set_contents: {}: {e}", self.own.display());
        }
    }

    fn load_partner(&self) -> Option<String> {
        KeyFile::load(&self.partner).get("Ring", "passphrase")
    }
}

/// Magasin en mémoire (tests, applications sans fichier de configuration).
#[derive(Debug, Default)]
pub struct MemoryStore {
    own: Mutex<Option<String>>,
    partner: Mutex<Option<String>>,
}

impl MemoryStore {
    pub fn new(own: Option<&str>, partner: Option<&str>) -> Self {
        MemoryStore { own: Mutex::new(own.map(str::to_string)), partner: Mutex::new(partner.map(str::to_string)) }
    }

    pub fn set_partner(&self, partner: Option<&str>) {
        *self.partner.lock().unwrap() = partner.map(str::to_string);
    }
}

impl PassphraseStore for MemoryStore {
    fn load(&self) -> Option<String> {
        self.own.lock().unwrap().clone()
    }

    fn save(&self, passphrase: &str) {
        *self.own.lock().unwrap() = Some(passphrase.to_string());
    }

    fn load_partner(&self) -> Option<String> {
        self.partner.lock().unwrap().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyfile_edit() {
        let mut kf = KeyFile::parse("# commentaire\n[Tournament]\nbackup_location=C:\\\\x\n\n[Other]\na=1\n");
        assert_eq!(kf.get("Tournament", "backup_location").as_deref(), Some("C:\\x"));
        kf.set("Tournament", "interface_language", "fr");
        kf.set("Ring", "passphrase", "abc");
        kf.set("Other", "a", "2");
        let data = kf.to_data();
        assert!(data.starts_with("# commentaire\n[Tournament]\nbackup_location=C:\\\\x\ninterface_language=fr\n\n[Other]\na=2\n"));
        let kf = KeyFile::parse(&data);
        assert_eq!(kf.get("Ring", "passphrase").as_deref(), Some("abc"));
    }

    #[test]
    fn file_store() {
        let dir = std::env::temp_dir().join(format!("bp-net-cfg-{}", rand::random::<u32>()));
        let store = UserConfigStore::with_paths(dir.join("A/config.ini"), dir.join("B/config.ini"));
        assert_eq!(store.load(), None);
        store.save("secret");
        assert_eq!(store.load().as_deref(), Some("secret"));
        assert_eq!(store.load_partner(), None);
        let _ = std::fs::remove_dir_all(dir);
    }
}
