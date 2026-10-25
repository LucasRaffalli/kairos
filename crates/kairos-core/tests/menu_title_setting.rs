//! Le réglage `menu_title_seconds` : les secondes dans le titre de l'icône, vrai par défaut.

use kairos_core::clock::FakeClock;
use kairos_core::db::Db;
use kairos_core::service::Kairos;

fn kairos() -> (tempfile::TempDir, Kairos<FakeClock>) {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("kairos.db")).unwrap();
    (dir, Kairos::new(db, FakeClock::new(1_792_713_600, 0)))
}

#[test]
fn seconds_are_shown_by_default() {
    let (_d, k) = kairos();
    assert_eq!(k.config_get("menu_title_seconds").unwrap(), "true");
}

#[test]
fn it_accepts_true_and_false_and_remembers_them() {
    let (_d, k) = kairos();
    k.config_set("menu_title_seconds", "false").unwrap();
    assert_eq!(k.config_get("menu_title_seconds").unwrap(), "false");
    k.config_set("menu_title_seconds", "true").unwrap();
    assert_eq!(k.config_get("menu_title_seconds").unwrap(), "true");
}

#[test]
fn anything_else_is_refused_with_a_clear_message() {
    let (_d, k) = kairos();
    for bad in ["", "yes", "1", "False", "true "] {
        let err = k.config_set("menu_title_seconds", bad).unwrap_err();
        assert_eq!(err.code(), "invalid_setting", "{bad:?}");
        assert!(
            err.to_string().contains("true") && err.to_string().contains("false"),
            "le message cite les deux valeurs : {err}"
        );
    }
    assert_eq!(k.config_get("menu_title_seconds").unwrap(), "true");
}

#[test]
fn the_unknown_key_message_lists_the_new_key() {
    let (_d, k) = kairos();
    let err = k.config_get("nope").unwrap_err();
    assert!(err.to_string().contains("menu_title_seconds"), "{err}");
}
