//! Le réglage d'autonomie de Claude et la lecture d'une entrée.

use chrono::NaiveDate;
use kairos_core::clock::FakeClock;
use kairos_core::db::Db;
use kairos_core::error::Error;
use kairos_core::model::AddEntry;
use kairos_core::service::Kairos;

fn at(h: u32, m: u32) -> i64 {
    NaiveDate::from_ymd_opt(2026, 10, 23)
        .unwrap()
        .and_hms_opt(h, m, 0)
        .unwrap()
        .and_utc()
        .timestamp()
}

fn kairos_at(path: &std::path::Path) -> Kairos<FakeClock> {
    Kairos::new(Db::open(path).unwrap(), FakeClock::new(at(17, 0), 0))
}

fn temp() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("kairos.db");
    (dir, path)
}

fn code<T: std::fmt::Debug>(r: Result<T, Error>) -> &'static str {
    r.unwrap_err().code()
}

#[test]
fn autonomy_defaults_to_ask() {
    let (_d, path) = temp();
    assert_eq!(
        kairos_at(&path).config_get("claude_autonomy").unwrap(),
        "ask"
    );
}

#[test]
fn autonomy_accepts_ask_and_auto_only() {
    let (_d, path) = temp();
    let k = kairos_at(&path);
    k.config_set("claude_autonomy", "auto").unwrap();
    k.config_set("claude_autonomy", "ask").unwrap();
    for bad in ["AUTO", "", "yes", "auto ", "Ask", "true"] {
        let err = k.config_set("claude_autonomy", bad).unwrap_err();
        assert_eq!(err.code(), "invalid_setting", "{bad:?}");
        assert!(
            err.to_string().contains("ask") && err.to_string().contains("auto"),
            "le message cite les deux valeurs : {err}"
        );
    }
    assert_eq!(k.config_get("claude_autonomy").unwrap(), "ask");
}

#[test]
fn autonomy_survives_reopening_the_database() {
    let (_d, path) = temp();
    kairos_at(&path)
        .config_set("claude_autonomy", "auto")
        .unwrap();
    assert_eq!(
        kairos_at(&path).config_get("claude_autonomy").unwrap(),
        "auto"
    );
}

#[test]
fn an_existing_version_1_database_gets_the_default_on_upgrade() {
    let (_d, path) = temp();
    drop(Db::open(&path).unwrap());
    {
        let c = rusqlite::Connection::open(&path).unwrap();
        c.execute("DELETE FROM settings WHERE key = 'claude_autonomy'", [])
            .unwrap();
        // La base de la version 1 n'a pas non plus les colonnes de la migration 3.
        c.execute_batch(
            "ALTER TABLE entries DROP COLUMN source; ALTER TABLE entries DROP COLUMN workdir;",
        )
        .unwrap();
        c.pragma_update(None, "user_version", 1).unwrap();
    }
    assert_eq!(
        kairos_at(&path).config_get("claude_autonomy").unwrap(),
        "ask"
    );
}

#[test]
fn entry_view_returns_the_entry_with_its_lot() {
    let (_d, path) = temp();
    let k = kairos_at(&path);
    let e = k
        .add_entry(AddEntry {
            parent_monday_id: Some("12406850311".into()),
            subtask: Some("Front".into()),
            from: at(9, 0),
            to: at(10, 0),
            ..AddEntry::default()
        })
        .unwrap();
    let view = k.entry_view(e.id).unwrap();
    assert_eq!(
        (view.id, view.start, view.end, view.removed),
        (e.id, at(9, 0), Some(at(10, 0)), false)
    );
    assert_eq!(view.lot.describe(), "Front sur 12406850311");
}

#[test]
fn entry_view_unknown_id_is_entry_not_found() {
    let (_d, path) = temp();
    assert_eq!(code(kairos_at(&path).entry_view(42)), "entry_not_found");
}

#[test]
fn entry_view_does_not_change_anything() {
    let (_d, path) = temp();
    let k = kairos_at(&path);
    let e = k
        .add_entry(AddEntry {
            name: Some("Réunion".into()),
            from: at(9, 0),
            to: at(10, 0),
            ..AddEntry::default()
        })
        .unwrap();
    let dump = |k: &Kairos<FakeClock>| -> String {
        let c = k.db().conn();
        let mut parts = Vec::new();
        for table in ["entries", "lots", "days", "state", "settings"] {
            let n: i64 = c
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
                .unwrap();
            parts.push(format!("{table}={n}"));
        }
        parts.join(",")
    };
    let before = dump(&k);
    k.entry_view(e.id).unwrap();
    assert_eq!(dump(&k), before);
}
