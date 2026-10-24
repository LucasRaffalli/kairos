//! Le binaire `kairos`, de bout en bout : chaque test a sa base temporaire et son heure figée.

use std::process::{Command, Output};

use serde_json::Value;

/// 2026-10-23 09:00 UTC (heure locale = UTC dans ces tests).
const T9: i64 = 1_792_746_000;

struct Env {
    dir: tempfile::TempDir,
}

impl Env {
    fn new() -> Env {
        Env {
            dir: tempfile::tempdir().unwrap(),
        }
    }

    fn run_at(&self, now: i64, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_kairos"))
            .args(args)
            .env("KAIROS_DB", self.dir.path().join("kairos.db"))
            .env("KAIROS_NOW", now.to_string())
            .env("KAIROS_TZ_OFFSET", "0")
            .output()
            .expect("le binaire kairos se lance")
    }

    fn run(&self, args: &[&str]) -> Output {
        self.run_at(T9, args)
    }
}

fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).to_string()
}

fn err(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).to_string()
}

fn json(o: &Output) -> Value {
    serde_json::from_slice(&o.stdout).unwrap_or_else(|e| panic!("JSON invalide ({e}) : {}", out(o)))
}

fn ok(o: Output) -> Output {
    assert!(
        o.status.success(),
        "attendu un succès : {} / {}",
        out(&o),
        err(&o)
    );
    o
}

#[test]
fn start_then_status_shows_running() {
    let e = Env::new();
    let started = ok(e.run(&["start", "--parent", "12406850311", "--subtask", "Front"]));
    assert!(
        out(&started).contains("Front sur 12406850311"),
        "{}",
        out(&started)
    );
    let status = ok(e.run_at(T9 + 1800, &["status"]));
    let text = out(&status);
    assert!(
        text.contains("En cours") && text.contains("depuis 9 h 00") && text.contains("0 h 30"),
        "{text}"
    );
}

#[test]
fn status_when_idle() {
    let e = Env::new();
    assert!(out(&ok(e.run(&["status"]))).contains("Rien en cours"));
}

#[test]
fn start_json_has_stable_keys() {
    let e = Env::new();
    let v = json(&ok(e.run(&["--json", "start", "--parent", "12406850311"])));
    assert_eq!(v["lot"]["parent_monday_id"], "12406850311");
    assert_eq!(v["lot"]["subtask"], "Front");
    assert_eq!(v["entry"]["start"], T9);
    assert!(v["entry"]["end"].is_null());
    let status = json(&ok(e.run_at(T9 + 60, &["status", "--json"])));
    assert_eq!(
        (status["state"].as_str(), status["duration"].as_i64()),
        (Some("running"), Some(60))
    );
}

#[test]
fn a_second_start_exits_2_with_a_readable_message() {
    let e = Env::new();
    ok(e.run(&["start", "--parent", "1"]));
    let second = e.run(&["start", "--parent", "2"]);
    assert_eq!(second.status.code(), Some(2));
    assert!(err(&second).contains("déjà en cours"), "{}", err(&second));
    assert!(out(&second).is_empty());
}

#[test]
fn errors_in_json_are_a_code_and_a_message_on_stdout() {
    let e = Env::new();
    ok(e.run(&["start", "--parent", "1"]));
    let second = e.run(&["--json", "start", "--parent", "2"]);
    assert_eq!(second.status.code(), Some(2));
    let v = json(&second);
    assert_eq!(v["error"]["code"], "already_running");
    assert!(v["error"]["message"]
        .as_str()
        .unwrap()
        .contains("déjà en cours"));
}

#[test]
fn pause_resume_switch_stop_roundtrip() {
    let e = Env::new();
    ok(e.run(&["start", "--parent", "111"]));
    assert!(out(&ok(e.run_at(T9 + 1800, &["pause"]))).contains("En pause"));
    assert!(out(&ok(e.run_at(T9 + 1800, &["status"]))).contains("En pause"));
    ok(e.run_at(T9 + 2400, &["resume"]));
    let switched = ok(e.run_at(
        T9 + 3000,
        &["switch", "--parent", "222", "--subtask", "Back"],
    ));
    let text = out(&switched);
    assert!(
        text.contains("Front sur 111") && text.contains("Back sur 222"),
        "{text}"
    );
    let stopped = ok(e.run_at(T9 + 3600, &["stop"]));
    assert!(out(&stopped).contains("Lot arrêté") && out(&stopped).contains("Back sur 222"));
    assert!(out(&ok(e.run_at(T9 + 3600, &["status"]))).contains("Rien en cours"));
}

#[test]
fn switch_when_idle_starts() {
    let e = Env::new();
    ok(e.run(&["switch", "--parent", "111"]));
    assert!(out(&ok(e.run(&["status"]))).contains("En cours"));
}

#[test]
fn stopping_nothing_exits_2() {
    let e = Env::new();
    assert_eq!(e.run(&["stop"]).status.code(), Some(2));
    assert!(err(&e.run(&["stop"])).contains("Aucun lot en cours"));
}

#[test]
fn add_edit_delete_entry_roundtrip() {
    let e = Env::new();
    let now = T9 + 8 * 3600; // 17 h 00
    let added = ok(e.run_at(
        now,
        &[
            "add",
            "--parent",
            "111",
            "--subtask",
            "Front",
            "--from",
            "09:00",
            "--to",
            "10:00",
        ],
    ));
    assert!(out(&added).contains("#1"), "{}", out(&added));
    ok(e.run_at(now, &["edit", "1", "--to", "10:30"]));
    let today = out(&ok(e.run_at(now, &["today"])));
    assert!(
        today.contains("#1") && today.contains("9 h 00 → 10 h 30") && today.contains("1 h 30"),
        "{today}"
    );
    ok(e.run_at(now, &["delete", "1"]));
    assert!(!out(&ok(e.run_at(now, &["today"]))).contains("#1"));
    assert_eq!(e.run_at(now, &["delete", "1"]).status.code(), Some(2));
}

#[test]
fn add_refuses_an_overlap_with_a_clear_message() {
    let e = Env::new();
    let now = T9 + 8 * 3600;
    ok(e.run_at(
        now,
        &["add", "--parent", "111", "--from", "09:00", "--to", "10:00"],
    ));
    let clash = e.run_at(
        now,
        &["add", "--parent", "222", "--from", "09:30", "--to", "10:30"],
    );
    assert_eq!(clash.status.code(), Some(2));
    assert!(
        err(&clash).contains("Chevauche 9 h 00 – 10 h 00 sur 111"),
        "{}",
        err(&clash)
    );
}

fn day_of_entries(e: &Env, now: i64) {
    let entries: [&[&str]; 3] = [
        &[
            "add",
            "--parent",
            "12406850311",
            "--subtask",
            "Front",
            "--from",
            "09:00",
            "--to",
            "11:10",
        ],
        &[
            "add",
            "--parent",
            "12406850311",
            "--subtask",
            "Back",
            "--from",
            "11:10",
            "--to",
            "12:54",
        ],
        &[
            "add", "--name", "Réunion", "--from", "13:00", "--to", "13:30",
        ],
    ];
    for args in entries {
        ok(e.run_at(now, args));
    }
}

#[test]
fn report_prints_the_monday_line() {
    let e = Env::new();
    let now = T9 + 8 * 3600;
    day_of_entries(&e, now);
    let text = out(&ok(e.run_at(now, &["report"])));
    assert!(text.contains("À saisir dans Monday"), "{text}");
    assert!(
        text.contains("12406850311 : Front 2 h 10 · Back 1 h 44 · sans ID : 0 h 30"),
        "{text}"
    );
}

#[test]
fn report_json_matches_the_text() {
    let e = Env::new();
    let now = T9 + 8 * 3600;
    day_of_entries(&e, now);
    let v = json(&ok(e.run_at(now, &["report", "--json"])));
    assert_eq!(
        v["text"],
        "12406850311 : Front 2 h 10 · Back 1 h 44 · sans ID : 0 h 30"
    );
    assert_eq!(v["gap"], 0);
    assert_eq!(v["lines"][0]["monday_id"], "12406850311");
}

#[test]
fn close_day_then_declared_then_an_edit_goes_to_redeclare() {
    let e = Env::new();
    let now = T9 + 8 * 3600;
    day_of_entries(&e, now);
    assert!(out(&ok(e.run_at(now, &["close-day"]))).contains("clôturée"));
    assert!(out(&ok(e.run_at(now, &["mark-declared"]))).contains("saisie"));
    ok(e.run_at(now, &["edit", "2", "--to", "13:00"]));
    let v = json(&ok(e.run_at(now, &["today", "--json"])));
    assert_eq!(v["status"], "to_redeclare");
    let report = out(&ok(e.run_at(now, &["report"])));
    assert!(
        report.contains("12406850311 : Back 1 h 44 → 1 h 50"),
        "{report}"
    );
}

#[test]
fn close_day_with_a_running_lot_suggests_the_stop_flag() {
    let e = Env::new();
    ok(e.run(&["start", "--parent", "111"]));
    let refused = e.run_at(T9 + 3600, &["close-day"]);
    assert_eq!(refused.status.code(), Some(2));
    assert!(err(&refused).contains("--stop"), "{}", err(&refused));
    assert!(out(&ok(e.run_at(T9 + 3600, &["close-day", "--stop"]))).contains("clôturée"));
}

#[test]
fn an_invalid_date_exits_2_with_the_invalid_date_code() {
    let e = Env::new();
    assert_eq!(e.run(&["report", "--date", "hier"]).status.code(), Some(2));
    let v = json(&e.run(&["--json", "report", "--date", "2026-13-45"]));
    assert_eq!(v["error"]["code"], "invalid_date");
}

#[test]
fn an_invalid_time_exits_2_with_the_invalid_time_code() {
    let e = Env::new();
    let v = json(&e.run(&["--json", "start", "--parent", "1", "--at", "25:61"]));
    assert_eq!(v["error"]["code"], "invalid_time");
}

#[test]
fn config_main_subtask_and_rounding_roundtrip() {
    let e = Env::new();
    assert!(out(&ok(e.run(&["config", "main_subtask"]))).contains("main_subtask = Front"));
    ok(e.run(&["config", "main_subtask", "Back"]));
    assert!(out(&ok(e.run(&["config", "main_subtask"]))).contains("Back"));
    assert_eq!(
        e.run(&["config", "rounding_minutes", "7"]).status.code(),
        Some(2)
    );
    ok(e.run(&["config", "rounding_minutes", "15"]));
    let started = ok(e.run(&["start", "--parent", "1"]));
    assert!(
        out(&started).contains("Back sur 1"),
        "le poste principal change vite : {}",
        out(&started)
    );
}

#[test]
fn set_subtask_id_validates_digits() {
    let e = Env::new();
    ok(e.run(&[
        "set-subtask-id",
        "--parent",
        "111",
        "--subtask",
        "Front",
        "--id",
        "12406850412",
    ]));
    let bad = e.run(&[
        "--json",
        "set-subtask-id",
        "--parent",
        "111",
        "--subtask",
        "Front",
        "--id",
        "12a",
    ]);
    assert_eq!(json(&bad)["error"]["code"], "invalid_monday_id");
}

#[test]
fn help_lists_all_commands() {
    let e = Env::new();
    let help = out(&ok(e.run(&["--help"])));
    for command in [
        "start",
        "stop",
        "pause",
        "resume",
        "switch",
        "status",
        "today",
        "report",
        "close-day",
        "mark-declared",
        "add",
        "edit",
        "delete",
        "set-subtask-id",
        "config",
        "version",
    ] {
        assert!(help.contains(command), "{command} absent de l'aide");
    }
}

#[test]
fn an_unknown_flag_exits_2_without_a_panic() {
    let e = Env::new();
    let o = e.run(&["start", "--bogus"]);
    assert_eq!(o.status.code(), Some(2));
    assert!(!err(&o).contains("panicked"));
}

#[test]
fn an_unusable_database_path_is_an_error_not_a_panic() {
    let e = Env::new();
    let o = Command::new(env!("CARGO_BIN_EXE_kairos"))
        .args(["status"])
        .env("KAIROS_DB", e.dir.path()) // un dossier, pas un fichier
        .env("KAIROS_NOW", T9.to_string())
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(2));
    assert!(!err(&o).contains("panicked"), "{}", err(&o));
}

#[test]
fn the_report_lists_every_no_id_timer_under_its_name() {
    let e = Env::new();
    let now = T9 + 8 * 3600;
    day_of_entries(&e, now);
    ok(e.run_at(
        now,
        &[
            "add", "--name", "Réunion", "--from", "14:00", "--to", "14:15",
        ],
    ));
    let text = out(&ok(e.run_at(now, &["report"])));
    assert!(text.contains("sans ID"), "{text}");
    assert!(text.contains("Réunion  0 h 45"), "{text}");
    assert!(text.contains("#3  13 h 00 → 13 h 30  0 h 30"), "{text}");
    assert!(text.contains("#4  14 h 00 → 14 h 15  0 h 15"), "{text}");
    let json = json(&ok(e.run_at(now, &["report", "--json"])));
    assert_eq!(json["no_id"][0]["entries"].as_array().unwrap().len(), 2);
}

#[test]
fn recent_back_and_label_work_end_to_end() {
    let e = Env::new();
    ok(e.run_at(T9, &["start", "--parent", "111", "--label", "Bundle"]));
    ok(e.run_at(T9 + 600, &["switch", "--parent", "222"]));

    let recent = json(&ok(e.run_at(T9 + 1200, &["--json", "recent"])));
    let recent = recent.as_array().unwrap();
    assert_eq!(recent.len(), 2);
    assert_eq!(recent[0]["lot"]["parent_monday_id"], "222");
    assert_eq!(recent[0]["current"], true);
    assert_eq!(recent[1]["lot"]["parent_label"], "Bundle");
    assert_eq!(recent[1]["today_total"], 600);

    let text = out(&ok(e.run_at(T9 + 1200, &["recent"])));
    assert!(
        text.contains("▶") && text.contains("Bundle") && text.contains("222"),
        "{text}"
    );
    assert_eq!(
        ok(e.run_at(T9 + 1200, &["recent", "--limit", "1"]))
            .stdout
            .iter()
            .filter(|b| **b == b'\n')
            .count(),
        1
    );

    let back = ok(e.run_at(T9 + 1200, &["back"]));
    assert!(out(&back).contains("111"), "{}", out(&back));
    let again = json(&ok(e.run_at(T9 + 1800, &["--json", "back"])));
    assert_eq!(again["started"]["lot"]["parent_monday_id"], "222");

    let label = ok(e.run_at(T9 + 1800, &["label", "111", "Bundle v2"]));
    assert!(out(&label).contains("Bundle v2"), "{}", out(&label));
    let recent = json(&ok(e.run_at(T9 + 1800, &["--json", "recent"])));
    assert!(recent
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["lot"]["parent_label"] == "Bundle v2"));
}

#[test]
fn back_without_previous_lot_and_label_of_unknown_parent_fail_with_their_codes() {
    let e = Env::new();
    let back = e.run(&["--json", "back"]);
    assert_eq!(back.status.code(), Some(2));
    assert_eq!(json(&back)["error"]["code"], "no_previous_lot");
    let label = e.run(&["--json", "label", "999", "Nom"]);
    assert_eq!(label.status.code(), Some(2));
    assert_eq!(json(&label)["error"]["code"], "parent_not_found");
}
