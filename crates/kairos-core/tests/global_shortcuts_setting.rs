//! Le réglage `global_shortcuts` : les raccourcis globaux ⌘K, vrai par défaut.

use kairos_core::clock::FakeClock;
use kairos_core::db::Db;
use kairos_core::service::Kairos;

fn kairos() -> (tempfile::TempDir, Kairos<FakeClock>) {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("kairos.db")).unwrap();
    (dir, Kairos::new(db, FakeClock::new(1_792_713_600, 0)))
}

#[test]
fn global_shortcuts_are_on_by_default() {
    let (_d, k) = kairos();
    assert_eq!(k.config_get("global_shortcuts").unwrap(), "true");
}

#[test]
fn it_accepts_true_and_false_and_remembers_them() {
    let (_d, k) = kairos();
    k.config_set("global_shortcuts", "false").unwrap();
    assert_eq!(k.config_get("global_shortcuts").unwrap(), "false");
    k.config_set("global_shortcuts", "true").unwrap();
    assert_eq!(k.config_get("global_shortcuts").unwrap(), "true");
}

#[test]
fn anything_else_is_refused_naming_the_key_and_both_values() {
    let (_d, k) = kairos();
    for bad in ["", "yes", "1", "False"] {
        let err = k.config_set("global_shortcuts", bad).unwrap_err();
        assert_eq!(err.code(), "invalid_setting", "{bad:?}");
        let text = err.to_string();
        assert!(
            text.contains("global_shortcuts") && text.contains("true") && text.contains("false"),
            "{text}"
        );
    }
    assert_eq!(k.config_get("global_shortcuts").unwrap(), "true");
}

#[test]
fn the_unknown_key_message_lists_the_key() {
    let (_d, k) = kairos();
    let err = k.config_get("nope").unwrap_err();
    assert!(err.to_string().contains("global_shortcuts"), "{err}");
}
