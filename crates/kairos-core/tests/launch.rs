//! L'origine d'un lancement (qui, depuis quel dossier), les choix de sous-tâches, `data_version`.

use chrono::NaiveDate;
use kairos_core::clock::FakeClock;
use kairos_core::db::{Db, LATEST_VERSION};
use kairos_core::model::{Source, StartArgs, Status};
use kairos_core::service::Kairos;

fn at(h: u32, m: u32) -> i64 {
    NaiveDate::from_ymd_opt(2026, 10, 23)
        .unwrap()
        .and_hms_opt(h, m, 0)
        .unwrap()
        .and_utc()
        .timestamp()
}

/// Un service sur une base temporaire, à 9 h 00 le vendredi 23 octobre 2026 (heure locale = UTC).
fn kairos_at(now: i64) -> (Kairos<FakeClock>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("kairos.db")).unwrap();
    (Kairos::new(db, FakeClock::new(now, 0)), dir)
}

fn kairos() -> (Kairos<FakeClock>, tempfile::TempDir) {
    kairos_at(at(9, 0))
}

fn launched(parent: &str, source: Source, workdir: Option<&str>) -> StartArgs {
    StartArgs {
        parent_monday_id: Some(parent.into()),
        source,
        workdir: workdir.map(str::to_string),
        ..StartArgs::default()
    }
}

/// (qui, dossier) d'un statut en cours ou en pause.
fn origin(status: Status) -> (Option<Source>, Option<String>) {
    match status {
        Status::Running {
            launched_by,
            workdir,
            ..
        }
        | Status::Paused {
            launched_by,
            workdir,
            ..
        } => (launched_by, workdir),
        Status::Idle => panic!("rien ne tourne"),
    }
}

#[test]
fn a_started_entry_remembers_who_and_where() {
    let (k, _d) = kairos();
    let started = k
        .start(launched(
            "12406850311",
            Source::ClaudeCode,
            Some("/Users/lucas/Beeside/main"),
        ))
        .unwrap();
    assert_eq!(started.entry.source, Some(Source::ClaudeCode));
    assert_eq!(
        origin(k.status().unwrap()),
        (
            Some(Source::ClaudeCode),
            Some("/Users/lucas/Beeside/main".to_string())
        )
    );
    k.clock().advance(600);
    k.switch_to(launched("12406850412", Source::ClaudeDesktop, None))
        .unwrap();
    assert_eq!(
        origin(k.status().unwrap()),
        (Some(Source::ClaudeDesktop), None)
    );
}

#[test]
fn a_resumed_entry_and_a_midnight_split_inherit_the_origin() {
    let (k, _d) = kairos();
    k.start(launched("12406850311", Source::ClaudeCode, Some("/a/b")))
        .unwrap();
    k.clock().advance(600);
    k.pause().unwrap();
    k.clock().advance(600);
    let resumed = k.resume().unwrap();
    assert_eq!(resumed.entry.source, Some(Source::ClaudeCode));
    assert_eq!(resumed.entry.workdir.as_deref(), Some("/a/b"));
    k.stop(None).unwrap();

    let (k, _d) = kairos_at(at(23, 50));
    k.start(launched("12406850311", Source::ClaudeCode, Some("/a/b")))
        .unwrap();
    k.clock().advance(20 * 60);
    assert_eq!(
        origin(k.status().unwrap()),
        (Some(Source::ClaudeCode), Some("/a/b".to_string()))
    );
    let yesterday = k.today(Some("2026-10-23")).unwrap();
    assert_eq!(yesterday.entries.len(), 1);
    let today = k.today(Some("2026-10-24")).unwrap();
    assert_eq!(today.entries.len(), 1);
    let closed_piece: (Option<String>, Option<String>) = k
        .db()
        .conn()
        .query_row(
            "SELECT source, workdir FROM entries WHERE end_at IS NOT NULL",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        closed_piece,
        (Some("claude-code".to_string()), Some("/a/b".to_string()))
    );
}

#[test]
fn a_reused_lot_reports_the_launcher_of_the_current_entry() {
    let (k, _d) = kairos();
    k.start(launched("12406850311", Source::Manual, None))
        .unwrap();
    k.clock().advance(600);
    k.stop(None).unwrap();
    k.start(launched("12406850311", Source::ClaudeCode, None))
        .unwrap();
    assert_eq!(origin(k.status().unwrap()).0, Some(Source::ClaudeCode));
}

#[test]
fn a_workdir_is_cleaned_never_an_error() {
    let (k, _d) = kairos();
    let workdir_after = |workdir: &str| {
        k.start(launched("12406850311", Source::ClaudeCode, Some(workdir)))
            .unwrap();
        let found = origin(k.status().unwrap()).1;
        k.clock().advance(60);
        k.stop(None).unwrap();
        found
    };
    assert_eq!(workdir_after(""), None);
    assert_eq!(workdir_after("   "), None);
    assert_eq!(workdir_after("/a\n/b"), None);
    assert_eq!(workdir_after(" /a/b "), Some("/a/b".to_string()));
    let long = "x".repeat(800);
    assert_eq!(workdir_after(&long), Some("x".repeat(500)));
}

#[test]
fn migration_3_keeps_a_0_2_0_database() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("kairos.db");
    drop(Db::open(&path).unwrap());
    {
        // On ramène la base à l'état de la version 0.2.0 : sans les deux colonnes, au user_version 2.
        let c = rusqlite::Connection::open(&path).unwrap();
        c.execute_batch(&format!(
            "ALTER TABLE entries DROP COLUMN source;
             ALTER TABLE entries DROP COLUMN workdir;
             INSERT INTO lots (id, project_id, name) VALUES (1, 1, 'Ancien'), (2, 1, 'Fermé');
             INSERT INTO entries (lot_id, start_at, end_at) VALUES (2, 100, 200);
             INSERT INTO entries (lot_id, start_at) VALUES (1, {open});
             INSERT INTO days (date, status, closed_at, closed_totals) VALUES ('2026-10-01', 'closed', 5, '{{}}');
             PRAGMA user_version = 2;",
            open = at(8, 0)
        ))
        .unwrap();
    }
    let db = Db::open(&path).unwrap();
    let version: i64 = db
        .conn()
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!((version, LATEST_VERSION), (3, 3));
    let k = Kairos::new(db, FakeClock::new(at(9, 0), 0));
    match k.status().unwrap() {
        Status::Running {
            lot,
            launched_by,
            workdir,
            ..
        } => {
            assert_eq!(lot.name.as_deref(), Some("Ancien"));
            assert_eq!((launched_by, workdir), (None, None));
        }
        other => panic!("le lot en cours aurait dû survivre : {other:?}"),
    }
    let entries: i64 = k
        .db()
        .conn()
        .query_row("SELECT COUNT(*) FROM entries", [], |r| r.get(0))
        .unwrap();
    let days: i64 = k
        .db()
        .conn()
        .query_row("SELECT COUNT(*) FROM days", [], |r| r.get(0))
        .unwrap();
    assert_eq!((entries, days), (2, 1));
}

#[test]
fn subtask_choices_lists_main_base_and_own_subtasks() {
    let (k, _d) = kairos();
    let unknown = k.subtask_choices(Some("12406850311")).unwrap();
    assert_eq!(unknown.main, "Front");
    assert_eq!(unknown.base, ["Back", "Front", "Recettage", "Maquette"]);
    assert!(unknown.own.is_empty());

    k.start(StartArgs {
        subtask: Some("Retour icône".into()),
        ..launched("12406850311", Source::Manual, None)
    })
    .unwrap();
    let known = k.subtask_choices(Some("12406850311")).unwrap();
    assert_eq!(known.own, ["Retour icône"]);
    assert!(!known.base.contains(&"Retour icône".to_string()));
    assert!(k
        .subtask_choices(Some("12406850412"))
        .unwrap()
        .own
        .is_empty());

    k.config_set("main_subtask", "Back").unwrap();
    assert_eq!(k.subtask_choices(None).unwrap().main, "Back");
    assert!(k.subtask_choices(None).unwrap().own.is_empty());
}

#[test]
fn data_version_changes_after_another_connection_writes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("kairos.db");
    let a = Db::open(&path).unwrap();
    let b = Db::open(&path).unwrap();
    let first = a.data_version().unwrap();
    a.conn()
        .query_row("SELECT 1", [], |r| r.get::<_, i64>(0))
        .unwrap();
    assert_eq!(a.data_version().unwrap(), first);
    b.conn()
        .execute(
            "UPDATE settings SET value = 'Back' WHERE key = 'main_subtask'",
            [],
        )
        .unwrap();
    assert_ne!(a.data_version().unwrap(), first);
}
