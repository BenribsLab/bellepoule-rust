//! Ouverture des différents types de fichiers par le tournoi.

use std::path::PathBuf;

use bp_core::player::PlayerClass;
use bp_core::tournament::user_config::UserConfig;
use bp_core::tournament::Tournament;

fn data(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests-data").join(name)
}

fn tournament() -> Tournament {
    Tournament::new(UserConfig::new())
}

#[test]
fn opens_cotcot() {
    let mut t = tournament();
    let opened = t.open(&data("exemple.cotcot")).unwrap();
    assert_eq!(opened.contests.len(), 1);
    assert!(!opened.needs_file_name);
    let c = t.contest(opened.contests[0]).unwrap();
    assert_eq!(c.players.of_class(PlayerClass::Fencer).count(), 53);
    assert_eq!(t.contest_by_filename(c.filename.as_ref().unwrap()), Some(opened.contests[0]));
}

#[test]
fn opens_fff_as_new_contest() {
    let mut t = tournament();
    let opened = t.open(&data("Fencers-Tireurs(FFF)/CLS_SHM.FFF")).unwrap();
    assert!(opened.ask_for_settings);
    let c = t.contest(opened.contests[0]).unwrap();
    let n = c.players.of_class(PlayerClass::Fencer).count();
    assert!(n > 0, "aucun tireur importé");
    let kinds: Vec<&str> = c.schedule.stages().map(|s| s.kind().xml_name()).collect();
    assert_eq!(kinds.first(), Some(&"Pointage"));
}

#[test]
fn opens_zip() {
    let mut t = tournament();
    let opened = t.open(&data("Circuit_National_élite_SHM17.zip")).unwrap();
    assert!(opened.needs_file_name);
    let c = t.contest(opened.contests[0]).unwrap();
    assert!(c.players.of_class(PlayerClass::Fencer).count() > 0);
}

#[test]
fn opens_fie_xml() {
    let mut t = tournament();
    for name in ["FIE(XML)/Individuelle.XML", "FIE(XML)/ParEquipe.XML"] {
        let opened = t.open(&data(name)).unwrap_or_else(|e| panic!("{name} : {e}"));
        let c = t.contest(opened.contests[0]).unwrap();
        assert!(c.players.len() > 0, "{name} : aucun joueur");
        assert!(opened.needs_file_name);
    }
}
