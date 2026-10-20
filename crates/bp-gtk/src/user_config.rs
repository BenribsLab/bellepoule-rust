//! Configuration utilisateur (port de `common/util/user_config.cpp`).
//!
//! Fichier `config.ini` (format GKeyFile) dans `<config utilisateur>/<app>/`.
//! Une instance globale par fil (`Global::_user_config`) est accessible par
//! [`with_config`] ; elle est enregistrée à chaque modification par les
//! accesseurs `set_*` et par [`save`].

use std::cell::RefCell;
use std::path::PathBuf;

use gtk4::glib;

pub struct UserConfig {
    pub key_file: glib::KeyFile,
    file_path: PathBuf,
    read_only: bool,
}

impl UserConfig {
    /// Ouvre (ou prépare) la configuration de l'application `app_name`.
    pub fn new(app_name: &str, read_only: bool) -> UserConfig {
        let dir = glib::user_config_dir().join(app_name);
        UserConfig::with_path(dir.join("config.ini"), read_only)
    }

    /// Configuration stockée dans un fichier précis.
    pub fn with_path(file_path: PathBuf, read_only: bool) -> UserConfig {
        let key_file = glib::KeyFile::new();
        if key_file.load_from_file(&file_path, glib::KeyFileFlags::KEEP_COMMENTS).is_err() {
            if let Some(dir) = file_path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
        }
        UserConfig { key_file, file_path, read_only }
    }

    pub fn is_empty(&self) -> bool {
        self.key_file.groups().is_empty()
    }

    pub fn save(&self) {
        if self.read_only {
            return;
        }
        if let Err(e) = self.key_file.save_to_file(&self.file_path) {
            log::warn!("g_file_set_contents: {e}");
        }
    }

    pub fn path(&self) -> &std::path::Path {
        &self.file_path
    }

    pub fn get_string(&self, group: &str, key: &str) -> Option<String> {
        self.key_file.string(group, key).ok().map(|s| s.to_string())
    }

    pub fn set_string(&self, group: &str, key: &str, value: &str) {
        self.key_file.set_string(group, key, value);
    }

    pub fn get_integer(&self, group: &str, key: &str) -> Option<i32> {
        self.key_file.integer(group, key).ok()
    }

    pub fn set_integer(&self, group: &str, key: &str, value: i32) {
        self.key_file.set_integer(group, key, value);
    }

    pub fn get_boolean(&self, group: &str, key: &str) -> Option<bool> {
        self.key_file.boolean(group, key).ok()
    }

    pub fn set_boolean(&self, group: &str, key: &str, value: bool) {
        self.key_file.set_boolean(group, key, value);
    }
}

impl Drop for UserConfig {
    fn drop(&mut self) {
        self.save();
    }
}

thread_local! {
    static CONFIG: RefCell<Option<UserConfig>> = const { RefCell::new(None) };
}

/// Installe la configuration globale (au démarrage de l'application).
pub fn install(config: UserConfig) {
    CONFIG.with(|c| *c.borrow_mut() = Some(config));
}

/// Accès à la configuration globale ; une configuration en mémoire, non
/// enregistrée, est créée si aucune n'a été installée (tests).
pub fn with_config<R>(f: impl FnOnce(&UserConfig) -> R) -> R {
    CONFIG.with(|c| {
        let mut c = c.borrow_mut();
        if c.is_none() {
            let path = std::env::temp_dir().join("bellepoule-rust-test").join("config.ini");
            *c = Some(UserConfig::with_path(path, true));
        }
        f(c.as_ref().unwrap())
    })
}

pub fn get_string(group: &str, key: &str) -> Option<String> {
    with_config(|c| c.get_string(group, key))
}

pub fn set_string(group: &str, key: &str, value: &str) {
    with_config(|c| {
        c.set_string(group, key, value);
        c.save();
    })
}

pub fn get_integer(group: &str, key: &str) -> Option<i32> {
    with_config(|c| c.get_integer(group, key))
}

pub fn set_integer(group: &str, key: &str, value: i32) {
    with_config(|c| {
        c.set_integer(group, key, value);
        c.save();
    })
}

pub fn get_boolean(group: &str, key: &str) -> Option<bool> {
    with_config(|c| c.get_boolean(group, key))
}

pub fn set_boolean(group: &str, key: &str, value: bool) {
    with_config(|c| {
        c.set_boolean(group, key, value);
        c.save();
    })
}

pub fn is_empty() -> bool {
    with_config(|c| c.is_empty())
}

pub fn save() {
    with_config(|c| c.save())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let path = std::env::temp_dir().join("bp-gtk-config-test").join("config.ini");
        let _ = std::fs::remove_file(&path);
        {
            let c = UserConfig::with_path(path.clone(), false);
            assert!(c.is_empty());
            c.set_string("Tournament", "interface_language", "fr");
            c.set_integer("Competiton", "color_to_use", 3);
        }
        let c = UserConfig::with_path(path, true);
        assert_eq!(c.get_string("Tournament", "interface_language").as_deref(), Some("fr"));
        assert_eq!(c.get_integer("Competiton", "color_to_use"), Some(3));
    }
}
