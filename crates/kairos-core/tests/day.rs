//! Entrées manuelles, journées et réglages.

use chrono::NaiveDate;
use kairos_core::clock::FakeClock;
use kairos_core::db::Db;
use kairos_core::error::Error;
use kairos_core::model::{AddEntry, DayStatus, EditEntry, StartArgs};
use kairos_core::service::Kairos;

fn ts(y: i32, m: u32, d: u32, h: u32, mi: u32) -> i64 {
    NaiveDate::from_ymd_opt(y, m, d)
        .unwrap()
        .and_hms_opt(h, mi, 0)
        .unwrap()
        .and_utc()
        .timestamp()
}

fn at(h: u32, m: u32) -> i64 {
    ts(2026, 10, 23, h, m)
}

/// Un service sur une base temporaire, à 17 h 00 le vendredi 23 octobre 2026 (heure locale = UTC).
fn kairos() -> (Kairos<FakeClock>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("kairos.db")).unwrap();
    (Kairos::new(db, FakeClock::new(at(17, 0), 0)), dir)
}

fn add(parent: &str, from: i64, to: i64) -> AddEntry {
    AddEntry {
        parent_monday_id: Some(parent.into()),
        subtask: Some("Front".into()),
        from,
        to,
        ..AddEntry::default()
    }
}

fn code<T: std::fmt::Debug>(r: Result<T, Error>) -> &'static str {
    r.unwrap_err().code()
}

fn count(k: &Kairos<FakeClock>, sql: &str) -> i64 {
    k.db().conn().query_row(sql, [], |r| r.get(0)).unwrap()
}

const DAY: &str = "2026-10-23";

#[test]
fn add_entry_refuses_overlap_future_and_midnight_crossing() {
    let (k, _d) = kairos();
    k.add_entry(add("12406850311", at(9, 0), at(10, 0)))
        .unwrap();
    assert_eq!(
        code(k.add_entry(add("12406856533", at(9, 30), at(10, 30)))),
        "overlap"
    );
    assert_eq!(
        code(k.add_entry(add("12406856533", at(16, 0), at(18, 0)))),
        "in_future"
    );
    assert_eq!(
        code(k.add_entry(add("12406856533", ts(2026, 10, 22, 23, 0), at(1, 0)))),
        "crosses_midnight"
    );
    assert_eq!(
        count(&k, "SELECT COUNT(*) FROM entries"),
        1,
        "aucun refus n'a laissé de trace"
    );
}

#[test]
fn an_overlap_names_the_clashing_entry() {
    let (k, _d) = kairos();
    k.add_entry(add("12406850311", at(14, 10), at(15, 2)))
        .unwrap();
    let err = k
        .add_entry(add("12406856533", at(14, 30), at(15, 30)))
        .unwrap_err();
    assert!(
        err.to_string()
            .contains("14 h 10 – 15 h 02 sur 12406850311"),
        "{err}"
    );
}

#[test]
fn a_zero_or_negative_entry_is_refused_empty_entry() {
    let (k, _d) = kairos();
    assert_eq!(
        code(k.add_entry(add("12406850311", at(9, 0), at(9, 0)))),
        "empty_entry"
    );
    assert_eq!(
        code(k.add_entry(add("12406850311", at(10, 0), at(9, 0)))),
        "empty_entry"
    );
}

#[test]
fn touching_entries_are_accepted() {
    let (k, _d) = kairos();
    k.add_entry(add("12406850311", at(9, 0), at(10, 0)))
        .unwrap();
    k.add_entry(add("12406856533", at(10, 0), at(11, 0)))
        .unwrap();
    k.add_entry(add("12406850311", at(8, 0), at(9, 0))).unwrap();
    assert_eq!(count(&k, "SELECT COUNT(*) FROM entries"), 3);
}

#[test]
fn an_entry_cannot_overlap_the_running_lot() {
    let (k, _d) = kairos();
    k.start(StartArgs {
        parent_monday_id: Some("12406850311".into()),
        at: Some(at(16, 0)),
        ..StartArgs::default()
    })
    .unwrap();
    assert_eq!(
        code(k.add_entry(add("12406856533", at(16, 30), at(16, 45)))),
        "overlap"
    );
    assert!(k
        .add_entry(add("12406856533", at(15, 0), at(16, 0)))
        .is_ok());
}

#[test]
fn an_entry_needs_a_reference() {
    let (k, _d) = kairos();
    assert_eq!(
        code(k.add_entry(AddEntry {
            from: at(9, 0),
            to: at(10, 0),
            ..AddEntry::default()
        })),
        "reference_missing"
    );
}

#[test]
fn edit_entry_applies_the_same_rules() {
    let (k, _d) = kairos();
    let a = k
        .add_entry(add("12406850311", at(9, 0), at(10, 0)))
        .unwrap();
    let b = k
        .add_entry(add("12406856533", at(10, 0), at(11, 0)))
        .unwrap();
    assert_eq!(
        code(k.edit_entry(
            b.id,
            EditEntry {
                from: Some(at(9, 30)),
                ..EditEntry::default()
            }
        )),
        "overlap"
    );
    assert_eq!(
        code(k.edit_entry(
            b.id,
            EditEntry {
                to: Some(at(18, 0)),
                ..EditEntry::default()
            }
        )),
        "in_future"
    );
    assert_eq!(
        code(k.edit_entry(
            b.id,
            EditEntry {
                to: Some(at(10, 0)),
                ..EditEntry::default()
            }
        )),
        "empty_entry"
    );
    assert_eq!(
        code(k.edit_entry(999, EditEntry::default())),
        "entry_not_found"
    );
    // Une modification valide, et le lot ne change pas si on ne le demande pas.
    let edited = k
        .edit_entry(
            a.id,
            EditEntry {
                to: Some(at(9, 45)),
                ..EditEntry::default()
            },
        )
        .unwrap();
    assert_eq!(
        (edited.start, edited.end, edited.lot_id),
        (at(9, 0), Some(at(9, 45)), a.lot_id)
    );
    // Changer de parent change de lot.
    let moved = k
        .edit_entry(
            a.id,
            EditEntry {
                parent_monday_id: Some("12406856533".into()),
                ..EditEntry::default()
            },
        )
        .unwrap();
    assert_eq!(moved.lot_id, b.lot_id);
    // Une entrée ne peut pas être déplacée d'un jour à l'autre à cheval sur minuit.
    assert_eq!(
        code(k.edit_entry(
            a.id,
            EditEntry {
                from: Some(ts(2026, 10, 22, 23, 0)),
                ..EditEntry::default()
            }
        )),
        "crosses_midnight"
    );
}

#[test]
fn delete_entry_removes_it_and_an_unknown_id_fails() {
    let (k, _d) = kairos();
    let e = k
        .add_entry(add("12406850311", at(9, 0), at(10, 0)))
        .unwrap();
    k.delete_entry(e.id).unwrap();
    assert_eq!(count(&k, "SELECT COUNT(*) FROM entries"), 0);
    assert_eq!(code(k.delete_entry(e.id)), "entry_not_found");
}

#[test]
fn an_invalid_date_argument_is_refused() {
    let (k, _d) = kairos();
    assert_eq!(code(k.today(Some("2026-13-45"))), "invalid_date");
    assert_eq!(code(k.today(Some("hier"))), "invalid_date");
    assert_eq!(code(k.close_day(Some("hier"), false)), "invalid_date");
    assert_eq!(code(k.mark_declared(Some("2026-02-30"))), "invalid_date");
}

#[test]
fn today_gives_the_bounds_the_pauses_and_the_total() {
    let (k, _d) = kairos();
    k.add_entry(add("12406850311", at(9, 0), at(10, 0)))
        .unwrap();
    k.add_entry(add("12406856533", at(10, 30), at(11, 30)))
        .unwrap();
    let day = k.today(None).unwrap();
    assert_eq!(day.date, DAY);
    assert_eq!(
        (day.start, day.end, day.pauses, day.total),
        (Some(at(9, 0)), Some(at(11, 30)), 1800, 7200)
    );
    assert_eq!(day.entries.len(), 2);
    assert_eq!(day.entries[0].lot.describe(), "Front sur 12406850311");
}

#[test]
fn today_counts_the_running_entry_until_now() {
    let (k, _d) = kairos();
    k.start(StartArgs {
        parent_monday_id: Some("12406850311".into()),
        at: Some(at(16, 0)),
        ..StartArgs::default()
    })
    .unwrap();
    let day = k.today(None).unwrap();
    assert_eq!((day.end, day.total), (Some(at(17, 0)), 3600));
}

#[test]
fn removed_entries_are_excluded_from_totals_and_bounds() {
    let (k, _d) = kairos();
    k.add_entry(add("12406850311", at(9, 0), at(10, 0)))
        .unwrap();
    let gone = k
        .add_entry(add("12406856533", at(10, 0), at(11, 0)))
        .unwrap();
    k.db()
        .conn()
        .execute("UPDATE entries SET removed = 1 WHERE id = ?1", [gone.id])
        .unwrap();
    let day = k.today(None).unwrap();
    assert_eq!(
        (day.start, day.end, day.total),
        (Some(at(9, 0)), Some(at(10, 0)), 3600)
    );
    assert_eq!(
        day.entries.len(),
        2,
        "l'entrée retirée reste visible, marquée"
    );
    assert!(day.entries[1].removed);
}

#[test]
fn a_day_without_a_row_is_open() {
    let (k, _d) = kairos();
    assert_eq!(k.today(Some("2026-09-01")).unwrap().status, DayStatus::Open);
    assert_eq!(count(&k, "SELECT COUNT(*) FROM days"), 0);
}

#[test]
fn close_day_freezes_the_totals_and_changes_the_status() {
    let (k, _d) = kairos();
    k.add_entry(add("12406850311", at(9, 0), at(10, 0)))
        .unwrap();
    k.add_entry(AddEntry {
        name: Some("Réunion".into()),
        from: at(10, 0),
        to: at(10, 30),
        ..AddEntry::default()
    })
    .unwrap();
    let day = k.close_day(None, false).unwrap();
    assert_eq!(
        (day.status, day.closed_at),
        (DayStatus::Closed, Some(at(17, 0)))
    );
    let frozen: String = k
        .db()
        .conn()
        .query_row(
            "SELECT closed_totals FROM days WHERE date = ?1",
            [DAY],
            |r| r.get(0),
        )
        .unwrap();
    let value: serde_json::Value = serde_json::from_str(&frozen).unwrap();
    assert_eq!(value["12406850311"]["Front"], 3600);
    assert_eq!(value["sans ID"]["Réunion"], 1800);
    assert_eq!(k.today(None).unwrap().status, DayStatus::Closed);
}

#[test]
fn close_day_is_refused_while_a_lot_runs_unless_stop_is_given() {
    let (k, _d) = kairos();
    k.start(StartArgs {
        parent_monday_id: Some("12406850311".into()),
        at: Some(at(16, 0)),
        ..StartArgs::default()
    })
    .unwrap();
    assert_eq!(code(k.close_day(None, false)), "running_lot_on_close");
    assert_eq!(k.today(None).unwrap().status, DayStatus::Open);
    let day = k.close_day(None, true).unwrap();
    assert_eq!(day.status, DayStatus::Closed);
    assert_eq!(
        count(&k, "SELECT COUNT(*) FROM entries WHERE end_at IS NULL"),
        0
    );
}

#[test]
fn closing_a_past_day_does_not_care_about_a_lot_running_today() {
    let (k, _d) = kairos();
    k.add_entry(add(
        "12406850311",
        ts(2026, 10, 22, 9, 0),
        ts(2026, 10, 22, 10, 0),
    ))
    .unwrap();
    k.start(StartArgs {
        parent_monday_id: Some("12406850311".into()),
        at: Some(at(16, 0)),
        ..StartArgs::default()
    })
    .unwrap();
    assert!(k.close_day(Some("2026-10-22"), false).is_ok());
}

#[test]
fn close_day_on_an_empty_date_is_day_empty_and_creates_no_row() {
    let (k, _d) = kairos();
    assert_eq!(code(k.close_day(Some("2026-10-01"), false)), "day_empty");
    assert_eq!(count(&k, "SELECT COUNT(*) FROM days"), 0);
}

#[test]
fn close_day_twice_is_refused() {
    let (k, _d) = kairos();
    k.add_entry(add("12406850311", at(9, 0), at(10, 0)))
        .unwrap();
    k.close_day(None, false).unwrap();
    assert_eq!(code(k.close_day(None, false)), "day_not_open");
}

#[test]
fn mark_declared_requires_a_closed_day() {
    let (k, _d) = kairos();
    k.add_entry(add("12406850311", at(9, 0), at(10, 0)))
        .unwrap();
    assert_eq!(code(k.mark_declared(None)), "day_not_closed");
    k.close_day(None, false).unwrap();
    let day = k.mark_declared(None).unwrap();
    assert_eq!(
        (day.status, day.declared_at),
        (DayStatus::Declared, Some(at(17, 0)))
    );
}

#[test]
fn editing_a_declared_day_sends_it_to_redeclare_and_declaring_again_settles_it() {
    let (k, _d) = kairos();
    let e = k
        .add_entry(add("12406850311", at(9, 0), at(10, 0)))
        .unwrap();
    k.close_day(None, false).unwrap();
    k.mark_declared(None).unwrap();
    k.edit_entry(
        e.id,
        EditEntry {
            to: Some(at(10, 15)),
            ..EditEntry::default()
        },
    )
    .unwrap();
    assert_eq!(k.today(None).unwrap().status, DayStatus::ToRedeclare);
    let declared: String = k
        .db()
        .conn()
        .query_row(
            "SELECT declared_totals FROM days WHERE date = ?1",
            [DAY],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        declared.contains("3600"),
        "le total déclaré reste celui d'avant : {declared}"
    );
    k.add_entry(add("12406856533", at(11, 0), at(11, 30)))
        .unwrap();
    assert_eq!(
        k.today(None).unwrap().status,
        DayStatus::ToRedeclare,
        "toujours à resaisir"
    );
    k.delete_entry(e.id).unwrap();
    assert_eq!(k.today(None).unwrap().status, DayStatus::ToRedeclare);
    let again = k.mark_declared(None).unwrap();
    assert_eq!(again.status, DayStatus::Declared);
    let declared: String = k
        .db()
        .conn()
        .query_row(
            "SELECT declared_totals FROM days WHERE date = ?1",
            [DAY],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        declared.contains("1800") && !declared.contains("3600"),
        "{declared}"
    );
}

#[test]
fn editing_a_closed_day_keeps_it_closed_and_refreshes_its_totals() {
    let (k, _d) = kairos();
    let e = k
        .add_entry(add("12406850311", at(9, 0), at(10, 0)))
        .unwrap();
    k.close_day(None, false).unwrap();
    k.edit_entry(
        e.id,
        EditEntry {
            to: Some(at(10, 30)),
            ..EditEntry::default()
        },
    )
    .unwrap();
    assert_eq!(k.today(None).unwrap().status, DayStatus::Closed);
    let frozen: String = k
        .db()
        .conn()
        .query_row(
            "SELECT closed_totals FROM days WHERE date = ?1",
            [DAY],
            |r| r.get(0),
        )
        .unwrap();
    assert!(frozen.contains("5400"), "{frozen}");
}

#[test]
fn editing_an_open_day_changes_nothing_about_its_state() {
    let (k, _d) = kairos();
    let e = k
        .add_entry(add("12406850311", at(9, 0), at(10, 0)))
        .unwrap();
    k.edit_entry(
        e.id,
        EditEntry {
            to: Some(at(10, 30)),
            ..EditEntry::default()
        },
    )
    .unwrap();
    assert_eq!(count(&k, "SELECT COUNT(*) FROM days"), 0);
}

#[test]
fn a_backdated_start_on_a_declared_day_sends_it_to_redeclare() {
    let (k, _d) = kairos();
    k.add_entry(add(
        "12406850311",
        ts(2026, 10, 22, 9, 0),
        ts(2026, 10, 22, 10, 0),
    ))
    .unwrap();
    k.close_day(Some("2026-10-22"), false).unwrap();
    k.mark_declared(Some("2026-10-22")).unwrap();
    k.add_entry(add(
        "12406856533",
        ts(2026, 10, 22, 11, 0),
        ts(2026, 10, 22, 11, 30),
    ))
    .unwrap();
    assert_eq!(
        k.today(Some("2026-10-22")).unwrap().status,
        DayStatus::ToRedeclare
    );
}

#[test]
fn set_subtask_id_validates_digits_and_remembers_the_id() {
    let (k, _d) = kairos();
    assert_eq!(
        code(k.set_subtask_id("12406850311", "Front", "12a")),
        "invalid_monday_id"
    );
    assert_eq!(
        code(k.set_subtask_id("abc", "Front", "12406850412")),
        "invalid_monday_id"
    );
    k.set_subtask_id("12406850311", "Retour icône", "12406850412")
        .unwrap();
    let id: String = k
        .db()
        .conn()
        .query_row(
            "SELECT monday_item_id FROM subtasks WHERE name = 'Retour icône'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(id, "12406850412");
}

#[test]
fn config_roundtrips_and_validates() {
    let (k, _d) = kairos();
    assert_eq!(k.config_get("main_subtask").unwrap(), "Front");
    assert_eq!(k.config_get("rounding_minutes").unwrap(), "0");
    k.config_set("main_subtask", "  Back ").unwrap();
    assert_eq!(k.config_get("main_subtask").unwrap(), "Back");
    for good in ["0", "5", "10", "15"] {
        k.config_set("rounding_minutes", good).unwrap();
    }
    for bad in ["7", "-5", "abc", "", "15 "] {
        assert_eq!(
            code(k.config_set("rounding_minutes", bad)),
            "invalid_setting",
            "{bad}"
        );
    }
    assert_eq!(k.config_get("rounding_minutes").unwrap(), "15");
    assert_eq!(code(k.config_get("couleur")), "invalid_setting");
    assert_eq!(code(k.config_set("couleur", "rouge")), "invalid_setting");
    assert_eq!(code(k.config_set("main_subtask", "   ")), "invalid_setting");
}

#[test]
fn a_subtask_monday_id_belongs_to_its_parent_even_for_a_base_subtask() {
    let (k, _d) = kairos();
    let with_item = |parent: &str, item: &str| StartArgs {
        parent_monday_id: Some(parent.into()),
        subtask: Some("Front".into()),
        subtask_monday_id: Some(item.into()),
        ..StartArgs::default()
    };
    k.start(with_item("12406850311", "12406850412")).unwrap();
    k.stop(None).unwrap();
    k.clock().advance(60);
    k.start(with_item("12406856533", "12406856601")).unwrap();
    assert_eq!(
        count(
            &k,
            "SELECT COUNT(*) FROM subtasks WHERE name = 'Front' AND monday_item_id IS NOT NULL"
        ),
        2
    );
    assert_eq!(count(&k, "SELECT COUNT(*) FROM subtasks WHERE name = 'Front' AND parent_id IS NULL AND monday_item_id IS NOT NULL"), 0, "la sous-tâche de base partagée ne porte aucun ID");
}

#[test]
fn a_backdated_start_on_a_declared_day_also_sends_it_to_redeclare() {
    let (k, _d) = kairos();
    k.add_entry(add(
        "12406850311",
        ts(2026, 10, 22, 9, 0),
        ts(2026, 10, 22, 10, 0),
    ))
    .unwrap();
    k.close_day(Some("2026-10-22"), false).unwrap();
    k.mark_declared(Some("2026-10-22")).unwrap();
    k.start(StartArgs {
        parent_monday_id: Some("12406856533".into()),
        at: Some(ts(2026, 10, 22, 22, 0)),
        ..StartArgs::default()
    })
    .unwrap();
    assert_eq!(
        k.today(Some("2026-10-22")).unwrap().status,
        DayStatus::ToRedeclare
    );
    assert_eq!(
        k.today(Some("2026-10-23")).unwrap().entries.len(),
        1,
        "la suite du lot est sur le jour courant"
    );
}

#[test]
fn resuming_on_a_declared_day_sends_it_to_redeclare_and_so_does_the_next_stop() {
    let (k, _d) = kairos();
    k.start(StartArgs {
        parent_monday_id: Some("12406850311".into()),
        at: Some(at(9, 0)),
        ..StartArgs::default()
    })
    .unwrap();
    k.pause().unwrap(); // 9 h 00 → 17 h 00
    k.close_day(None, false).unwrap();
    k.mark_declared(None).unwrap();
    k.clock().advance(600);
    k.resume().unwrap();
    assert_eq!(
        k.today(None).unwrap().status,
        DayStatus::ToRedeclare,
        "du temps vient de s'ajouter à une journée saisie"
    );
    k.mark_declared(None).unwrap();
    k.clock().advance(600);
    k.stop(None).unwrap();
    assert_eq!(
        k.today(None).unwrap().status,
        DayStatus::ToRedeclare,
        "l'arrêt change l'entrée d'une journée saisie"
    );
}

#[test]
fn a_date_is_normalized_so_a_day_never_splits_in_two_rows() {
    let (k, _d) = kairos();
    k.add_entry(add(
        "12406850311",
        ts(2026, 10, 5, 9, 0),
        ts(2026, 10, 5, 10, 0),
    ))
    .unwrap();
    assert_eq!(k.today(Some("2026-10-5")).unwrap().date, "2026-10-05");
    k.close_day(Some("2026-10-5"), false).unwrap();
    k.mark_declared(Some("2026-10-05")).unwrap();
    k.add_entry(add(
        "12406856533",
        ts(2026, 10, 5, 11, 0),
        ts(2026, 10, 5, 11, 30),
    ))
    .unwrap();
    assert_eq!(
        k.today(Some("2026-10-5")).unwrap().status,
        DayStatus::ToRedeclare
    );
    assert_eq!(count(&k, "SELECT COUNT(*) FROM days"), 1);
    assert_eq!(
        count(&k, "SELECT COUNT(*) FROM days WHERE date = '2026-10-05'"),
        1
    );
}
