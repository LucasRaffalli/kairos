//! La ventilation, le recoupement et le texte à saisir dans Monday.

use chrono::NaiveDate;
use kairos_core::clock::FakeClock;
use kairos_core::db::Db;
use kairos_core::model::{AddEntry, DayStatus, EditEntry, StartArgs};
use kairos_core::report::Change;
use kairos_core::service::Kairos;

fn at(h: u32, m: u32) -> i64 {
    NaiveDate::from_ymd_opt(2026, 10, 23)
        .unwrap()
        .and_hms_opt(h, m, 0)
        .unwrap()
        .and_utc()
        .timestamp()
}

/// 17 h 00 le vendredi 23 octobre 2026, heure locale = UTC.
fn kairos() -> (Kairos<FakeClock>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("kairos.db")).unwrap();
    (Kairos::new(db, FakeClock::new(at(17, 0), 0)), dir)
}

fn id_entry(k: &Kairos<FakeClock>, parent: &str, subtask: &str, from: i64, to: i64) -> i64 {
    k.add_entry(AddEntry {
        parent_monday_id: Some(parent.into()),
        subtask: Some(subtask.into()),
        from,
        to,
        ..AddEntry::default()
    })
    .unwrap()
    .id
}

fn named_entry(k: &Kairos<FakeClock>, name: &str, from: i64, to: i64) {
    k.add_entry(AddEntry {
        name: Some(name.into()),
        from,
        to,
        ..AddEntry::default()
    })
    .unwrap();
}

/// La journée d'exemple du spec : 7 h 33 chronométrées entre 6 h 00 et 13 h 51, deux pauses.
fn example_day(k: &Kairos<FakeClock>) {
    id_entry(k, "12406850311", "Front", at(6, 0), at(8, 10)); // 2 h 10
    id_entry(k, "12406850311", "Back", at(8, 10), at(9, 54)); // 1 h 44
    id_entry(k, "12406856533", "Recettage", at(10, 0), at(11, 5)); // 1 h 05
    id_entry(
        k,
        "12406856533",
        "Retour icône mal faite",
        at(11, 5),
        at(11, 48),
    ); // 0 h 43
    named_entry(k, "Maquette back-office", at(12, 0), at(13, 51)); // 1 h 51
}

const EXAMPLE: &str = "12406850311 : Front 2 h 10 · Back 1 h 44 · 12406856533 : Recettage 1 h 05 · Retour icône mal faite 0 h 43 · sans ID : 1 h 51";

#[test]
fn the_text_matches_the_spec_example() {
    let (k, _d) = kairos();
    example_day(&k);
    let report = k.report(None).unwrap();
    assert_eq!(report.text, EXAMPLE);
    assert_eq!(report.lines.len(), 2);
    assert_eq!(
        report.lines[0].subtasks,
        vec![("Front".to_string(), 7800), ("Back".to_string(), 6240)]
    );
    assert_eq!(report.lines[1].total, 3900 + 2580);
}

#[test]
fn the_gap_is_zero_without_rounding_and_the_bounds_add_up() {
    let (k, _d) = kairos();
    example_day(&k);
    let report = k.report(None).unwrap();
    assert_eq!(
        (report.start, report.end, report.pauses, report.total),
        (Some(at(6, 0)), Some(at(13, 51)), 1080, 27_180)
    );
    assert_eq!(
        (report.lines_total, report.gap, report.rounding_minutes),
        (27_180, 0, 0)
    );
}

#[test]
fn lines_follow_the_order_of_first_appearance() {
    let (k, _d) = kairos();
    id_entry(&k, "222", "Front", at(9, 0), at(10, 0));
    id_entry(&k, "111", "Front", at(10, 0), at(11, 0));
    id_entry(&k, "222", "Back", at(11, 0), at(12, 0));
    let report = k.report(None).unwrap();
    let ids: Vec<_> = report.lines.iter().map(|l| l.monday_id.as_str()).collect();
    assert_eq!(ids, ["222", "111"]);
    assert_eq!(
        report.lines[0]
            .subtasks
            .iter()
            .map(|(n, _)| n.as_str())
            .collect::<Vec<_>>(),
        ["Front", "Back"]
    );
    assert_eq!(
        report.text,
        "222 : Front 1 h 00 · Back 1 h 00 · 111 : Front 1 h 00"
    );
}

#[test]
fn entries_without_an_id_are_grouped_and_their_names_kept() {
    let (k, _d) = kairos();
    named_entry(&k, "Réunion", at(9, 0), at(9, 30));
    named_entry(&k, "Mail", at(9, 30), at(9, 45));
    named_entry(&k, "Réunion", at(10, 0), at(10, 15));
    let report = k.report(None).unwrap();
    assert_eq!(
        report
            .no_id
            .iter()
            .map(|n| (n.name.as_str(), n.secs))
            .collect::<Vec<_>>(),
        vec![("Réunion", 2700), ("Mail", 900)]
    );
    assert_eq!(report.no_id_total, 3600);
    assert_eq!(report.text, "sans ID : 1 h 00");
    assert!(report.lines.is_empty());
}

#[test]
fn rounding_applies_per_line_and_the_gap_is_reported_not_hidden() {
    let (k, _d) = kairos();
    example_day(&k);
    k.config_set("rounding_minutes", "15").unwrap();
    let report = k.report(None).unwrap();
    assert_eq!(report.rounding_minutes, 15);
    assert_eq!(report.text, "12406850311 : Front 2 h 15 · Back 1 h 45 · 12406856533 : Recettage 1 h 00 · Retour icône mal faite 0 h 45 · sans ID : 1 h 45");
    assert_eq!((report.lines_total, report.gap), (27_000, 180));
    assert_eq!(report.total, 27_180, "le total réel, lui, ne bouge pas");
}

#[test]
fn an_empty_day_has_an_empty_report() {
    let (k, _d) = kairos();
    let report = k.report(Some("2026-10-01")).unwrap();
    assert_eq!(
        (report.text.as_str(), report.gap, report.total, report.start),
        ("", 0, 0, None)
    );
    assert_eq!(report.status, DayStatus::Open);
}

#[test]
fn a_report_on_a_day_to_redeclare_lists_what_changed() {
    let (k, _d) = kairos();
    let front = id_entry(&k, "12406850311", "Front", at(9, 0), at(11, 10)); // 2 h 10
    k.close_day(None, false).unwrap();
    k.mark_declared(None).unwrap();
    assert!(k.report(None).unwrap().changes.is_empty());
    k.edit_entry(
        front,
        EditEntry {
            to: Some(at(11, 25)),
            ..EditEntry::default()
        },
    )
    .unwrap(); // 2 h 25
    let report = k.report(None).unwrap();
    assert_eq!(report.status, DayStatus::ToRedeclare);
    assert_eq!(
        report.changes,
        vec![Change {
            monday_id: "12406850311".into(),
            subtask: "Front".into(),
            from: 7800,
            to: 8700
        }]
    );
}

#[test]
fn a_new_line_and_a_removed_line_both_show_up_in_the_changes() {
    let (k, _d) = kairos();
    let a = id_entry(&k, "111", "Front", at(9, 0), at(10, 0));
    k.close_day(None, false).unwrap();
    k.mark_declared(None).unwrap();
    id_entry(&k, "222", "Back", at(10, 0), at(11, 0));
    k.delete_entry(a).unwrap();
    let changes = k.report(None).unwrap().changes;
    assert_eq!(
        changes,
        vec![
            Change {
                monday_id: "111".into(),
                subtask: "Front".into(),
                from: 3600,
                to: 0
            },
            Change {
                monday_id: "222".into(),
                subtask: "Back".into(),
                from: 0,
                to: 3600
            },
        ]
    );
}

#[test]
fn removed_entries_are_excluded_from_the_report() {
    let (k, _d) = kairos();
    id_entry(&k, "111", "Front", at(9, 0), at(10, 0));
    let gone = id_entry(&k, "222", "Back", at(10, 0), at(11, 0));
    k.db()
        .conn()
        .execute("UPDATE entries SET removed = 1 WHERE id = ?1", [gone])
        .unwrap();
    let report = k.report(None).unwrap();
    assert_eq!(report.text, "111 : Front 1 h 00");
    assert_eq!((report.end, report.total), (Some(at(10, 0)), 3600));
}

#[test]
fn a_running_entry_counts_until_now() {
    let (k, _d) = kairos();
    k.start(StartArgs {
        parent_monday_id: Some("111".into()),
        at: Some(at(16, 0)),
        ..StartArgs::default()
    })
    .unwrap();
    let report = k.report(None).unwrap();
    assert_eq!(report.text, "111 : Front 1 h 00");
    assert_eq!(report.end, Some(at(17, 0)));
}

#[test]
fn an_invalid_date_is_refused() {
    let (k, _d) = kairos();
    assert_eq!(
        k.report(Some("2026-13-45")).unwrap_err().code(),
        "invalid_date"
    );
}
