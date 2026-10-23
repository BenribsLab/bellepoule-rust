//! Lanceur de l'archive Windows : `ManageComp.exe`, à la racine, démarre
//! `bin\managecomp.exe` (l'utilisateur n'a pas à ouvrir `bin`).
//!
//! Le même programme sert pour les autres outils, d'après son propre nom :
//! « ManageComp Organisation.exe » → `bin\managecomp-organisation.exe`,
//! « ManageComp Live.exe » → `bin\managecomp-live.exe`. Les arguments (un
//! fichier `.cotcot` glissé sur l'icône, par exemple) sont transmis.
#![windows_subsystem = "windows"]

use std::path::PathBuf;
use std::process::Command;

fn target(name: &str) -> &'static str {
    let name = name.to_lowercase();
    if name.contains("organisation") {
        "managecomp-organisation.exe"
    } else if name.contains("live") {
        "managecomp-live.exe"
    } else {
        "managecomp.exe"
    }
}

fn main() {
    let me = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("ManageComp.exe"));
    let root = me.parent().map(PathBuf::from).unwrap_or_default();
    let name = me.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let program = root.join("bin").join(target(&name));
    if let Err(e) = Command::new(&program).args(std::env::args_os().skip(1)).spawn() {
        // Pas de console : le message va dans un fichier à côté du lanceur.
        let _ = std::fs::write(root.join("ManageComp-erreur.txt"), format!("{} : {e}\n", program.display()));
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn launcher_name_picks_program() {
        assert_eq!(super::target("ManageComp"), "managecomp.exe");
        assert_eq!(super::target("ManageComp Organisation"), "managecomp-organisation.exe");
        assert_eq!(super::target("ManageComp Live"), "managecomp-live.exe");
    }
}
