//! Les lots et le chronomètre : une règle du spec, un test.

use chrono::NaiveDate;
use kairos_core::clock::FakeClock;
use kairos_core::db::Db;
use kairos_core::error::Error;
use kairos_core::model::{Source, StartArgs, Status};
use kairos_core::service::Kairos;

fn ts(y: i32, m: u32, d: u32, h: u32, mi: u32) -> i64 {
    NaiveDate::from_ymd_opt(y, m, d)
        .unwrap()
        .and_hms_opt(h, mi, 0)
        .unwrap()
        .and_utc()
        .timestamp()
}

/// Un service sur une base temporaire, à 9 h 00 le vendredi 23 octobre 2026 (heure locale = UTC).
fn kairos() -> (Kairos<FakeClock>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("kairos.db")).unwrap();
    (
        Kairos::new(db, FakeClock::new(ts(2026, 10, 23, 9, 0), 0)),
        dir,
    )
}

fn id(parent: &str) -> StartArgs {
    StartArgs {
        parent_monday_id: Some(parent.into()),
        ..StartArgs::default()
    }
}

fn front(parent: &str) -> StartArgs {
    StartArgs {
        subtask: Some("Front".into()),
        ..id(parent)
    }
}

/// Les entrées (début, fin), de la plus ancienne à la plus récente.
fn entries(k: &Kairos<FakeClock>) -> Vec<(i64, Option<i64>)> {
    let mut stmt = k
        .db()
        .conn()
        .prepare("SELECT start_at, end_at FROM entries ORDER BY start_at, id")
        .unwrap();
    stmt.query_map([], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, Option<i64>>(1)?))
    })
    .unwrap()
    .map(|r| r.unwrap())
    .collect()
}

fn count(k: &Kairos<FakeClock>, sql: &str) -> i64 {
    k.db().conn().query_row(sql, [], |r| r.get(0)).unwrap()
}

fn code<T: std::fmt::Debug>(r: Result<T, Error>) -> &'static str {
    r.unwrap_err().code()
}

const T9: i64 = 1_792_746_000; // 2026-10-23 09:00 UTC, vérifié par `start_creates_lot_and_open_entry`

#[test]
fn start_creates_lot_and_open_entry() {
    let (k, _d) = kairos();
    assert_eq!(ts(2026, 10, 23, 9, 0), T9);
    let started = k.start(front("12406850311")).unwrap();
    assert_eq!(started.lot.describe(), "Front sur 12406850311");
    assert_eq!(entries(&k), vec![(T9, None)]);
}

#[test]
fn second_start_is_refused_already_running() {
    let (k, _d) = kairos();
    k.start(front("12406850311")).unwrap();
    let err = k.start(front("12406856533")).unwrap_err();
    assert_eq!(err.code(), "already_running");
    assert!(
        err.to_string()
            .contains("Front sur 12406850311 depuis 9 h 00"),
        "{err}"
    );
    assert_eq!(entries(&k).len(), 1);
}

#[test]
fn start_with_an_id_defaults_to_the_main_subtask() {
    let (k, _d) = kairos();
    assert_eq!(
        k.start(id("12406850311")).unwrap().lot.subtask.as_deref(),
        Some("Front")
    );
    k.stop(None).unwrap();
    k.clock().advance(60);
    k.db()
        .conn()
        .execute(
            "UPDATE settings SET value = 'Back' WHERE key = 'main_subtask'",
            [],
        )
        .unwrap();
    assert_eq!(
        k.start(id("12406856533")).unwrap().lot.subtask.as_deref(),
        Some("Back")
    );
}

#[test]
fn an_unknown_subtask_is_created_for_that_parent_only() {
    let (k, _d) = kairos();
    let retour = |parent: &str| StartArgs {
        subtask: Some("Retour icône mal faite".into()),
        ..id(parent)
    };
    k.start(retour("12406856533")).unwrap();
    k.clock().advance(60);
    k.stop(None).unwrap();
    assert_eq!(count(&k, "SELECT COUNT(*) FROM subtasks WHERE name = 'Retour icône mal faite' AND is_base = 0 AND parent_id IS NOT NULL"), 1);
    k.clock().advance(60);
    k.start(retour("12406850311")).unwrap();
    assert_eq!(
        count(
            &k,
            "SELECT COUNT(*) FROM subtasks WHERE name = 'Retour icône mal faite'"
        ),
        2,
        "une sous-tâche propre par parent"
    );
    assert_eq!(
        count(&k, "SELECT COUNT(*) FROM subtasks WHERE is_base = 1"),
        4,
        "les sous-tâches de base ne bougent pas"
    );
}

#[test]
fn a_base_subtask_name_reuses_the_shared_subtask() {
    let (k, _d) = kairos();
    k.start(front("12406850311")).unwrap();
    k.clock().advance(60);
    k.stop(None).unwrap();
    k.clock().advance(60);
    k.start(front("12406856533")).unwrap();
    assert_eq!(count(&k, "SELECT COUNT(*) FROM subtasks"), 4);
}

#[test]
fn the_same_lot_is_reused() {
    let (k, _d) = kairos();
    for _ in 0..2 {
        k.start(front("12406850311")).unwrap();
        k.clock().advance(600);
        k.stop(None).unwrap();
        k.clock().advance(600);
    }
    assert_eq!(count(&k, "SELECT COUNT(*) FROM lots"), 1);
    assert_eq!(entries(&k).len(), 2);
}

#[test]
fn a_reference_is_required_but_an_id_or_a_name_each_suffice() {
    let (k, _d) = kairos();
    assert_eq!(code(k.start(StartArgs::default())), "reference_missing");
    assert_eq!(
        code(k.start(StartArgs {
            name: Some("   ".into()),
            parent_monday_id: Some("  ".into()),
            ..StartArgs::default()
        })),
        "reference_missing"
    );
    k.start(StartArgs {
        name: Some("Réunion".into()),
        ..StartArgs::default()
    })
    .unwrap();
    k.stop(None).unwrap();
    k.clock().advance(60);
    let both = StartArgs {
        name: Some("Refonte".into()),
        ..front("12406850311")
    };
    assert!(k.start(both).is_ok());
}

#[test]
fn a_monday_id_is_digits_only_up_to_twenty() {
    let (k, _d) = kairos();
    assert_eq!(code(k.start(id("12a"))), "invalid_monday_id");
    assert_eq!(
        code(k.start(id("123456789012345678901"))),
        "invalid_monday_id"
    );
    assert_eq!(code(k.start(id("-12"))), "invalid_monday_id");
    assert!(k.start(id("12345678901234567890")).is_ok());
}

#[test]
fn a_task_name_is_trimmed_single_line_and_bounded() {
    let (k, _d) = kairos();
    let named = |n: &str| StartArgs {
        name: Some(n.into()),
        ..StartArgs::default()
    };
    assert_eq!(code(k.start(named("Re\nfonte"))), "invalid_name");
    assert_eq!(code(k.start(named(&"é".repeat(121)))), "invalid_name");
    assert_eq!(
        k.start(named("  Refonte \n")).unwrap().lot.name.as_deref(),
        Some("Refonte")
    );
    k.stop(None).unwrap();
    k.clock().advance(60);
    assert_eq!(
        k.start(named("Refonte 🎨 du site"))
            .unwrap()
            .lot
            .name
            .as_deref(),
        Some("Refonte 🎨 du site")
    );
    k.stop(None).unwrap();
    k.clock().advance(60);
    assert!(k.start(named(&"é".repeat(120))).is_ok());
}

#[test]
fn a_past_start_is_ok_when_free_and_refused_when_overlapping() {
    let (k, _d) = kairos();
    let at = |h, m| Some(ts(2026, 10, 23, h, m));
    k.start(StartArgs {
        at: at(8, 0),
        ..front("12406850311")
    })
    .unwrap();
    k.stop(None).unwrap(); // 8 h 00 → 9 h 00
    assert_eq!(entries(&k), vec![(ts(2026, 10, 23, 8, 0), Some(T9))]);
    let overlap = k
        .start(StartArgs {
            at: at(8, 30),
            ..front("12406856533")
        })
        .unwrap_err();
    assert_eq!(overlap.code(), "overlap");
    assert!(
        overlap
            .to_string()
            .contains("8 h 00 – 9 h 00 sur 12406850311"),
        "{overlap}"
    );
    assert!(
        k.start(StartArgs {
            at: at(9, 0),
            ..front("12406856533")
        })
        .is_ok(),
        "bornes qui se touchent : accepté"
    );
}

#[test]
fn a_start_in_the_future_is_refused() {
    let (k, _d) = kairos();
    assert_eq!(
        code(k.start(StartArgs {
            at: Some(T9 + 60),
            ..front("12406850311")
        })),
        "in_future"
    );
}

#[test]
fn pause_closes_the_entry_and_resume_opens_a_new_one() {
    let (k, _d) = kairos();
    k.start(front("12406850311")).unwrap();
    k.clock().advance(1800);
    let paused = k.pause().unwrap();
    assert_eq!(paused.duration, 1800);
    k.clock().advance(600);
    k.resume().unwrap();
    assert_eq!(entries(&k), vec![(T9, Some(T9 + 1800)), (T9 + 2400, None)]);
    assert_eq!(
        count(&k, "SELECT COUNT(DISTINCT lot_id) FROM entries"),
        1,
        "même lot"
    );
}

#[test]
fn pause_without_a_lot_and_resume_without_a_pause_fail() {
    let (k, _d) = kairos();
    assert_eq!(code(k.pause()), "nothing_running");
    assert_eq!(code(k.resume()), "nothing_to_resume");
    assert_eq!(code(k.stop(None)), "nothing_running");
}

#[test]
fn switch_closes_and_opens_at_the_same_second() {
    let (k, _d) = kairos();
    k.start(front("12406850311")).unwrap();
    k.clock().advance(600);
    let switched = k.switch_to(front("12406856533")).unwrap();
    assert_eq!(switched.stopped.unwrap().duration, 600);
    assert_eq!(entries(&k), vec![(T9, Some(T9 + 600)), (T9 + 600, None)]);
}

#[test]
fn a_failed_switch_leaves_the_running_lot_untouched() {
    let (k, _d) = kairos();
    k.start(front("12406850311")).unwrap();
    k.clock().advance(600);
    assert_eq!(code(k.switch_to(StartArgs::default())), "reference_missing");
    assert_eq!(entries(&k), vec![(T9, None)], "rien n'a été fermé");
}

#[test]
fn switch_when_idle_behaves_like_start() {
    let (k, _d) = kairos();
    let switched = k.switch_to(front("12406850311")).unwrap();
    assert!(switched.stopped.is_none());
    assert_eq!(entries(&k), vec![(T9, None)]);
}

#[test]
fn switch_to_the_same_lot_is_refused() {
    let (k, _d) = kairos();
    k.start(front("12406850311")).unwrap();
    k.clock().advance(600);
    assert_eq!(code(k.switch_to(front("12406850311"))), "same_lot");
    assert_eq!(entries(&k), vec![(T9, None)]);
}

#[test]
fn stop_and_start_clear_a_pause() {
    let (k, _d) = kairos();
    k.start(front("12406850311")).unwrap();
    k.clock().advance(60);
    k.pause().unwrap();
    k.stop(None).unwrap();
    assert_eq!(code(k.resume()), "nothing_to_resume");
    k.start(front("12406856533")).unwrap();
    k.clock().advance(60);
    k.pause().unwrap();
    k.clock().advance(60);
    k.start(front("12406850311")).unwrap();
    assert_eq!(code(k.resume()), "nothing_to_resume");
}

#[test]
fn a_lot_open_at_midnight_is_split() {
    let (k, _d) = kairos();
    k.clock().set_now(ts(2026, 10, 23, 23, 30));
    k.start(front("12406850311")).unwrap();
    k.clock().advance(2 * 3600);
    assert!(matches!(k.status().unwrap(), Status::Running { .. }));
    assert_eq!(
        entries(&k),
        vec![
            (ts(2026, 10, 23, 23, 30), Some(ts(2026, 10, 24, 0, 0))),
            (ts(2026, 10, 24, 0, 0), None)
        ]
    );
}

#[test]
fn a_lot_left_open_for_three_days_gives_one_entry_per_day() {
    let (k, _d) = kairos();
    k.clock().set_now(ts(2026, 10, 23, 23, 30));
    k.start(front("12406850311")).unwrap();
    k.clock().set_now(ts(2026, 10, 26, 8, 0));
    k.status().unwrap();
    let e = entries(&k);
    assert_eq!(e.len(), 4);
    assert_eq!(e[1], (ts(2026, 10, 24, 0, 0), Some(ts(2026, 10, 25, 0, 0))));
    assert_eq!(e[3], (ts(2026, 10, 26, 0, 0), None));
}

#[test]
fn a_lot_started_exactly_at_midnight_is_not_split_and_a_stop_there_keeps_one_entry() {
    let (k, _d) = kairos();
    k.clock().set_now(ts(2026, 10, 23, 22, 0));
    k.start(front("12406850311")).unwrap();
    k.clock().set_now(ts(2026, 10, 24, 0, 0));
    k.stop(None).unwrap();
    assert_eq!(
        entries(&k),
        vec![(ts(2026, 10, 23, 22, 0), Some(ts(2026, 10, 24, 0, 0)))]
    );
}

#[test]
fn a_clock_going_backwards_makes_stop_fail_and_keeps_the_entry_open() {
    let (k, _d) = kairos();
    k.start(front("12406850311")).unwrap();
    k.clock().set_now(T9 - 3600);
    let err = k.stop(None).unwrap_err();
    assert_eq!(err.code(), "clock_behind");
    assert!(err.to_string().contains("9 h 00"), "{err}");
    assert_eq!(entries(&k), vec![(T9, None)]);
}

#[test]
fn stopping_at_the_very_instant_of_the_start_drops_the_empty_entry() {
    let (k, _d) = kairos();
    k.start(front("12406850311")).unwrap();
    let stopped = k.stop(None).unwrap();
    assert_eq!(stopped.duration, 0);
    assert!(entries(&k).is_empty());
}

#[test]
fn stop_at_a_given_time_is_checked() {
    let (k, _d) = kairos();
    k.clock().set_now(ts(2026, 10, 23, 10, 0));
    k.start(StartArgs {
        at: Some(T9),
        ..front("12406850311")
    })
    .unwrap();
    assert_eq!(code(k.stop(Some(ts(2026, 10, 23, 10, 5)))), "in_future");
    assert_eq!(code(k.stop(Some(T9))), "empty_entry");
    assert_eq!(code(k.stop(Some(T9 - 60))), "empty_entry");
    let stopped = k.stop(Some(ts(2026, 10, 23, 9, 45))).unwrap();
    assert_eq!(stopped.duration, 45 * 60);
}

#[test]
fn status_reports_the_running_duration_and_the_total_of_today() {
    let (k, _d) = kairos();
    assert!(matches!(k.status().unwrap(), Status::Idle));
    k.start(front("12406850311")).unwrap();
    k.clock().advance(1800);
    k.pause().unwrap();
    match k.status().unwrap() {
        Status::Paused {
            lot,
            since,
            duration,
            ..
        } => assert_eq!(
            (lot.describe().as_str(), since, duration),
            ("Front sur 12406850311", T9 + 1800, 1800)
        ),
        other => panic!("attendu Paused, reçu {other:?}"),
    }
    k.clock().advance(600);
    k.resume().unwrap();
    k.clock().advance(900);
    match k.status().unwrap() {
        Status::Running {
            lot,
            since,
            duration,
            today_total,
            ..
        } => {
            assert_eq!(
                (lot.describe().as_str(), since, duration, today_total),
                ("Front sur 12406850311", T9 + 2400, 900, 2700)
            );
        }
        other => panic!("attendu Running, reçu {other:?}"),
    }
}

#[test]
fn the_source_is_kept_on_the_lot() {
    let (k, _d) = kairos();
    k.start(StartArgs {
        source: Source::ClaudeCode,
        ..front("12406850311")
    })
    .unwrap();
    let source: String = k
        .db()
        .conn()
        .query_row("SELECT source FROM lots", [], |r| r.get(0))
        .unwrap();
    assert_eq!(source, "claude-code");
}

// --- Passe de correction après la relecture finale -----------------------------------------

#[test]
fn resume_after_the_clock_went_back_is_refused_instead_of_overlapping() {
    let (k, _d) = kairos();
    k.start(front("12406850311")).unwrap();
    k.clock().advance(600);
    k.pause().unwrap();
    k.clock().set_now(T9 + 300);
    assert_eq!(code(k.resume()), "overlap");
    assert_eq!(entries(&k), vec![(T9, Some(T9 + 600))]);
}

#[test]
fn a_start_too_far_in_the_past_is_refused_instead_of_creating_one_entry_per_day() {
    let (k, _d) = kairos();
    let err = k
        .start(StartArgs {
            at: Some(T9 - 3 * 86_400),
            ..front("12406850311")
        })
        .unwrap_err();
    assert_eq!(err.code(), "too_old");
    assert!(err.to_string().contains("kairos add"), "{err}");
    assert!(entries(&k).is_empty());
    assert!(
        k.start(StartArgs {
            at: Some(T9 - 86_400),
            ..front("12406850311")
        })
        .is_ok(),
        "hier reste permis"
    );
}

/// 2026-10-23 07:00, un lot lancé hier à 20:00 puis oublié : coupé à minuit, ouvert depuis 00:00.
fn forgotten_lot() -> (Kairos<FakeClock>, tempfile::TempDir) {
    let (k, d) = kairos();
    k.clock().set_now(ts(2026, 10, 23, 7, 0));
    k.start(StartArgs {
        at: Some(ts(2026, 10, 22, 20, 0)),
        ..front("12406850311")
    })
    .unwrap();
    assert_eq!(
        entries(&k),
        vec![
            (ts(2026, 10, 22, 20, 0), Some(ts(2026, 10, 23, 0, 0))),
            (ts(2026, 10, 23, 0, 0), None)
        ]
    );
    (k, d)
}

#[test]
fn stopping_at_an_earlier_time_corrects_a_lot_forgotten_overnight() {
    let (k, _d) = forgotten_lot();
    let stopped = k.stop(Some(ts(2026, 10, 22, 23, 30))).unwrap();
    assert_eq!(stopped.duration, 3 * 3600 + 30 * 60);
    assert_eq!(
        entries(&k),
        vec![(ts(2026, 10, 22, 20, 0), Some(ts(2026, 10, 22, 23, 30)))]
    );
    assert!(matches!(k.status().unwrap(), Status::Idle));
}

#[test]
fn stopping_exactly_at_midnight_keeps_the_previous_piece_whole() {
    let (k, _d) = forgotten_lot();
    k.stop(Some(ts(2026, 10, 23, 0, 0))).unwrap();
    assert_eq!(
        entries(&k),
        vec![(ts(2026, 10, 22, 20, 0), Some(ts(2026, 10, 23, 0, 0)))]
    );
}

#[test]
fn stopping_before_the_whole_chain_is_refused_and_changes_nothing() {
    let (k, _d) = forgotten_lot();
    assert_eq!(code(k.stop(Some(ts(2026, 10, 22, 20, 0)))), "empty_entry");
    assert_eq!(code(k.stop(Some(ts(2026, 10, 22, 19, 0)))), "empty_entry");
    assert_eq!(entries(&k).len(), 2);
}

#[test]
fn stopping_inside_a_chain_of_three_days_drops_the_later_pieces() {
    let (k, _d) = kairos();
    k.clock().set_now(ts(2026, 10, 23, 7, 0));
    k.start(StartArgs {
        at: Some(ts(2026, 10, 21, 22, 0)),
        ..front("12406850311")
    })
    .unwrap();
    assert_eq!(entries(&k).len(), 3);
    k.stop(Some(ts(2026, 10, 22, 12, 0))).unwrap();
    assert_eq!(
        entries(&k),
        vec![
            (ts(2026, 10, 21, 22, 0), Some(ts(2026, 10, 22, 0, 0))),
            (ts(2026, 10, 22, 0, 0), Some(ts(2026, 10, 22, 12, 0)))
        ]
    );
}

#[test]
fn durations_are_never_negative_when_the_clock_goes_back() {
    let (k, _d) = kairos();
    k.start(front("12406850311")).unwrap();
    k.clock().set_now(T9 - 3600);
    match k.status().unwrap() {
        Status::Running {
            duration,
            today_total,
            ..
        } => assert_eq!((duration, today_total), (0, 0)),
        other => panic!("attendu Running, reçu {other:?}"),
    }
    let day = k.today(None).unwrap();
    assert_eq!((day.total, day.pauses), (0, 0));
    assert!(
        day.end.unwrap() >= day.start.unwrap(),
        "la fin d'une journée ne passe jamais sous son début"
    );
    let report = k.report(None).unwrap();
    assert_eq!((report.total, report.lines_total, report.pauses), (0, 0, 0));
    assert!(report.end.unwrap() >= report.start.unwrap());
}
