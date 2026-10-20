//! Localisation du dossier `resources/` (équivalent de `Global::_share_dir`).
//!
//! Ordre de recherche :
//! 1. la variable d'environnement `BELLEPOULE_RESOURCES` ;
//! 2. un dossier `resources` à côté de l'exécutable, puis dans ses parents
//!    (cas de `target/debug/bellepoule` lancé depuis le dépôt) ;
//! 3. `../share/bellepoule/resources` relatif à l'exécutable (installation) ;
//! 4. le dossier courant et ses parents.
//!
//! Un dossier n'est retenu que s'il contient `ui/` ou `glade/`.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

static OVERRIDE: OnceLock<PathBuf> = OnceLock::new();

/// Impose le dossier de ressources (tests, outils). Sans effet s'il a déjà été fixé.
pub fn set_resources_dir(dir: impl Into<PathBuf>) {
    let _ = OVERRIDE.set(dir.into());
}

fn is_resources_dir(p: &Path) -> bool {
    p.join("ui").is_dir() || p.join("glade").is_dir()
}

fn search_upwards(start: &Path) -> Option<PathBuf> {
    let mut cur = Some(start);
    while let Some(dir) = cur {
        let candidate = dir.join("resources");
        if is_resources_dir(&candidate) {
            return Some(candidate);
        }
        cur = dir.parent();
    }
    None
}

fn find() -> PathBuf {
    if let Ok(env) = std::env::var("BELLEPOULE_RESOURCES") {
        let p = PathBuf::from(env);
        if is_resources_dir(&p) {
            return p;
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            if let Some(p) = search_upwards(dir) {
                return p;
            }
            let share = dir.join("..").join("share").join("bellepoule").join("resources");
            if is_resources_dir(&share) {
                return share;
            }
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        if let Some(p) = search_upwards(&cwd) {
            return p;
        }
    }
    // Dossier du dépôt connu à la compilation (dernier recours).
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("resources")
}

/// Dossier `resources/`.
pub fn resources_dir() -> &'static Path {
    static FOUND: OnceLock<PathBuf> = OnceLock::new();
    if let Some(p) = OVERRIDE.get() {
        return p.as_path();
    }
    FOUND.get_or_init(find).as_path()
}

/// Racine de partage (`Global::_share_dir`) : parent de `resources/`.
pub fn share_dir() -> PathBuf {
    resources_dir().parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."))
}

/// Chemin d'un fichier relatif à `resources/` (ex. `glade/images/male.png`).
pub fn resource_path(rel: &str) -> PathBuf {
    resources_dir().join(rel)
}

/// Chemin d'un fichier `.ui` (ex. `tournament.ui`).
pub fn ui_path(name: &str) -> PathBuf {
    resources_dir().join("ui").join(name)
}

/// Chemin d'une image de `resources/glade/images/`.
pub fn image_path(name: &str) -> PathBuf {
    resources_dir().join("glade").join("images").join(name)
}

/// Dossier des traductions (`resources/translations`).
pub fn translations_dir() -> PathBuf {
    resources_dir().join("translations")
}

#[cfg(test)]
mod tests {
    #[test]
    fn resources_are_found() {
        let dir = super::resources_dir();
        assert!(dir.join("ui").join("tournament.ui").is_file(), "{}", dir.display());
        assert!(super::image_path("male.png").is_file());
    }
}
