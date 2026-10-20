//! Aller-retour des fichiers `.cotcot` : ouvrir, enregistrer, rouvrir.

use std::path::PathBuf;

use bp_core::contest::Contest;
use bp_core::player::PlayerClass;

fn tests_data() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests-data")
}

fn cotcot_files() -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(tests_data())
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().map(|e| e == "cotcot").unwrap_or(false))
        .collect();
    files.sort();
    files
}

/// Résumé comparable d'une compétition.
fn summary(c: &mut Contest) -> String {
    let fencers = c.players.of_class(PlayerClass::Fencer).count();
    let teams = c.players.of_class(PlayerClass::Team).count();
    let stages: Vec<String> = c.schedule.stages().map(|s| s.kind().xml_name().to_string()).collect();
    format!(
        "{}|{}|{}|{}|fencers={fencers}|teams={teams}|referees={}|stages={stages:?}|current={}",
        c.name,
        c.weapon.xml_image,
        c.gender_code(),
        c.xml_date(),
        c.referees.len(),
        c.schedule.current_index()
    )
}

#[test]
fn open_save_reopen() {
    let files = cotcot_files();
    assert!(!files.is_empty(), "aucun .cotcot dans tests-data");
    for file in files {
        let mut a = Contest::new(None, false);
        a.load_file(&file).unwrap_or_else(|e| panic!("{} : {e}", file.display()));
        let first = summary(&mut a);

        let xml = a.to_xml();
        let mut b = Contest::new(None, false);
        b.load_xml_str(&xml).unwrap_or_else(|e| panic!("{} (relecture) : {e}", file.display()));
        assert_eq!(first, summary(&mut b), "{}", file.display());

        // Un ancien fichier est normalisé à la première lecture (renumérotation
        // des tireurs) ; à partir de là l'enregistrement est stable.
        let second = b.to_xml();
        let mut c = Contest::new(None, false);
        c.load_xml_str(&second).unwrap_or_else(|e| panic!("{} (2e relecture) : {e}", file.display()));
        assert_eq!(second, c.to_xml(), "{} : l'enregistrement n'est pas stable", file.display());
    }
}

#[test]
fn example_has_expected_content() {
    let mut c = Contest::new(None, false);
    c.load_file(&tests_data().join("exemple.cotcot")).unwrap();
    assert_eq!(c.weapon.xml_image, "S");
    assert_eq!(c.gender_code(), "M");
    assert_eq!(c.xml_date(), "17.12.2011");
    assert_eq!(c.players.of_class(PlayerClass::Fencer).count(), 53);
    assert_eq!(c.referees.len(), 10);
    let kinds: Vec<&str> = c.schedule.stages().map(|s| s.kind().xml_name()).collect();
    assert_eq!(kinds.first(), Some(&"Pointage"));
    assert_eq!(kinds.last(), Some(&"ClassementGeneral"));
}

#[test]
fn new_contest_has_default_stages() {
    let c = Contest::new_with_default_stages(None);
    let kinds: Vec<&str> = c.schedule.stages().map(|s| s.kind().xml_name()).collect();
    // L'allocateur de poules n'a pas d'élément XML propre (nom vide).
    assert_eq!(kinds, vec!["Pointage", "", "TourDePoules", "PhaseDeTableaux", "ClassementGeneral"]);
}
