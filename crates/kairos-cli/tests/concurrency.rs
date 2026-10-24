//! Plusieurs processus sur la même base : l'app, la ligne de commande et le MCP écriront en même temps.

use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;

/// 2026-10-23 09:00 UTC.
const T9: i64 = 1_792_746_000;

fn spawn(db: &std::path::Path, args: &[&str]) -> Child {
    Command::new(env!("CARGO_BIN_EXE_kairos"))
        .args(args)
        .env("KAIROS_DB", db)
        .env("KAIROS_NOW", T9.to_string())
        .env("KAIROS_TZ_OFFSET", "0")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("le binaire kairos se lance")
}

fn run(db: &std::path::Path, args: &[&str]) -> std::process::Output {
    spawn(db, args).wait_with_output().unwrap()
}

fn code_of(out: &std::process::Output) -> Option<String> {
    let v: Value = serde_json::from_slice(&out.stdout).ok()?;
    v["error"]["code"].as_str().map(str::to_string)
}

fn open_entries(db: &std::path::Path) -> i64 {
    let c = rusqlite::Connection::open(db).unwrap();
    c.query_row(
        "SELECT COUNT(*) FROM entries WHERE end_at IS NULL",
        [],
        |r| r.get(0),
    )
    .unwrap()
}

#[test]
fn two_processes_starting_at_once_on_an_existing_db_only_one_wins() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("kairos.db");
    assert!(run(&db, &["status"]).status.success(), "crée la base");
    for round in 0..6 {
        run(&db, &["stop"]);
        let a = spawn(&db, &["--json", "start", "--parent", "111"]);
        let b = spawn(&db, &["--json", "start", "--parent", "222"]);
        let (a, b) = (a.wait_with_output().unwrap(), b.wait_with_output().unwrap());
        let wins = [&a, &b].iter().filter(|o| o.status.success()).count();
        assert_eq!(wins, 1, "tour {round} : exactement un gagnant");
        let loser = if a.status.success() { &b } else { &a };
        assert_eq!(
            code_of(loser).as_deref(),
            Some("already_running"),
            "tour {round}"
        );
        assert_eq!(open_entries(&db), 1, "jamais deux entrées ouvertes");
    }
}

#[test]
fn two_processes_creating_a_fresh_db_at_once_both_succeed_in_migrating() {
    for round in 0..8 {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("kairos.db");
        let a = spawn(&db, &["--json", "status"]);
        let b = spawn(&db, &["--json", "status"]);
        let (a, b) = (a.wait_with_output().unwrap(), b.wait_with_output().unwrap());
        assert!(
            a.status.success() && b.status.success(),
            "tour {round} : {} / {}",
            String::from_utf8_lossy(&a.stdout),
            String::from_utf8_lossy(&b.stdout)
        );
        let c = rusqlite::Connection::open(&db).unwrap();
        let projects: i64 = c
            .query_row("SELECT COUNT(*) FROM projects", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            projects, 1,
            "tour {round} : la migration n'a eu lieu qu'une fois"
        );
    }
}

#[test]
fn a_locked_database_reports_db_unavailable_after_the_timeout() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("kairos.db");
    assert!(run(&db, &["status"]).status.success());
    let holder = rusqlite::Connection::open(&db).unwrap();
    holder.execute_batch("BEGIN IMMEDIATE").unwrap();
    let started = Instant::now();
    let out = run(&db, &["--json", "status"]);
    let waited = started.elapsed();
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(
        code_of(&out).as_deref(),
        Some("db_unavailable"),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(
        waited >= Duration::from_millis(4_500),
        "le processus a attendu {waited:?}"
    );
    assert!(
        waited < Duration::from_secs(15),
        "mais pas indéfiniment : {waited:?}"
    );
    holder.execute_batch("ROLLBACK").unwrap();
    assert!(
        run(&db, &["status"]).status.success(),
        "la base est de nouveau disponible"
    );
}

#[test]
fn a_newer_database_is_refused_by_the_binary_and_left_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("kairos.db");
    assert!(run(&db, &["status"]).status.success());
    rusqlite::Connection::open(&db)
        .unwrap()
        .pragma_update(None, "user_version", 99)
        .unwrap();
    let before = std::fs::read(&db).unwrap();
    let out = run(&db, &["--json", "start", "--parent", "1"]);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(code_of(&out).as_deref(), Some("db_newer"));
    assert_eq!(std::fs::read(&db).unwrap(), before);
}

#[test]
fn unicode_and_emoji_names_roundtrip_through_the_report() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("kairos.db");
    let now = (T9 + 8 * 3600).to_string();
    let add = Command::new(env!("CARGO_BIN_EXE_kairos"))
        .args([
            "add",
            "--name",
            "Café ☕ « 日本語 »",
            "--from",
            "09:00",
            "--to",
            "10:00",
        ])
        .env("KAIROS_DB", &db)
        .env("KAIROS_NOW", &now)
        .env("KAIROS_TZ_OFFSET", "0")
        .output()
        .unwrap();
    assert!(
        add.status.success(),
        "{}",
        String::from_utf8_lossy(&add.stderr)
    );
    let report = Command::new(env!("CARGO_BIN_EXE_kairos"))
        .args(["--json", "report"])
        .env("KAIROS_DB", &db)
        .env("KAIROS_NOW", &now)
        .env("KAIROS_TZ_OFFSET", "0")
        .output()
        .unwrap();
    let v: Value = serde_json::from_slice(&report.stdout).unwrap();
    assert_eq!(v["no_id"][0]["name"], "Café ☕ « 日本語 »");
}
