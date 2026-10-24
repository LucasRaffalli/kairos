//! Retrouver un lot précédent : `resolve_monday_id`, `recent_lots`, `back`, `set_label`.

use chrono::NaiveDate;
use kairos_core::clock::FakeClock;
use kairos_core::db::Db;
use kairos_core::error::Error;
use kairos_core::model::{KnownId, Source, StartArgs};
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
fn kairos() -> (Kairos<FakeClock>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("kairos.db")).unwrap();
    (Kairos::new(db, FakeClock::new(at(9, 0), 0)), dir)
}

fn lot(parent: &str) -> StartArgs {
    StartArgs {
        parent_monday_id: Some(parent.into()),
        ..StartArgs::default()
    }
}

fn labelled(parent: &str, label: &str) -> StartArgs {
    StartArgs {
        parent_label: Some(label.into()),
        ..lot(parent)
    }
}

fn code<T: std::fmt::Debug>(r: Result<T, Error>) -> &'static str {
    r.unwrap_err().code()
}

#[test]
fn resolve_distinguishes_parent_subtask_and_unknown() {
    let (k, _d) = kairos();
    k.start(labelled("12406850311", "Bundle")).unwrap();
    k.set_subtask_id("12406850311", "Back", "12406856533")
        .unwrap();
    assert_eq!(
        k.resolve_monday_id("12406850311").unwrap(),
        KnownId::Parent {
            label: Some("Bundle".into())
        }
    );
    assert_eq!(
        k.resolve_monday_id("12406856533").unwrap(),
        KnownId::Subtask {
            parent_monday_id: "12406850311".into(),
            subtask: "Back".into(),
            label: Some("Bundle".into()),
        }
    );
    assert_eq!(
        k.resolve_monday_id("12406850412").unwrap(),
        KnownId::Unknown
    );
    assert_eq!(code(k.resolve_monday_id("abc")), "invalid_monday_id");
}

#[test]
fn recent_lots_lists_each_lot_once_most_recent_first() {
    let (k, _d) = kairos();
    k.start(labelled("12406850311", "Bundle")).unwrap();
    k.clock().advance(3600);
    k.switch_to(lot("12406856533")).unwrap();
    k.clock().advance(1800);
    k.switch_to(lot("12406850311")).unwrap();
    k.clock().advance(600);
    let recent = k.recent_lots(8).unwrap();
    assert_eq!(recent.len(), 2);
    assert_eq!(
        recent[0].lot.parent_monday_id.as_deref(),
        Some("12406850311")
    );
    assert!(recent[0].current);
    assert_eq!(recent[0].lot.parent_label.as_deref(), Some("Bundle"));
    // 1 h + 10 min sur Bundle, et l'entrée ouverte compte jusqu'à maintenant.
    assert_eq!(recent[0].today_total, 3600 + 600);
    assert_eq!(
        recent[1].lot.parent_monday_id.as_deref(),
        Some("12406856533")
    );
    assert!(!recent[1].current);
    assert_eq!(recent[1].today_total, 1800);
    assert_eq!(recent[1].last_end, at(10, 30));
}

#[test]
fn recent_lots_ignores_lots_older_than_seven_days_and_honors_limit() {
    let (k, _d) = kairos();
    k.clock().advance(-8 * 86_400);
    k.start(lot("12406850412")).unwrap();
    k.clock().advance(3600);
    k.stop(None).unwrap();
    k.clock().set_now(at(9, 0));
    k.start(lot("12406850311")).unwrap();
    k.clock().advance(600);
    k.switch_to(lot("12406856533")).unwrap();
    let recent = k.recent_lots(8).unwrap();
    let ids: Vec<_> = recent
        .iter()
        .map(|r| r.lot.parent_monday_id.clone().unwrap())
        .collect();
    assert_eq!(ids, ["12406856533", "12406850311"]);
    assert_eq!(k.recent_lots(1).unwrap().len(), 1);
}

#[test]
fn back_toggles_between_two_lots() {
    let (k, _d) = kairos();
    k.start(lot("12406850311")).unwrap();
    k.clock().advance(600);
    k.switch_to(lot("12406856533")).unwrap();
    k.clock().advance(600);
    let first = k.back(Source::ClaudeCode).unwrap();
    assert_eq!(
        first.started.lot.parent_monday_id.as_deref(),
        Some("12406850311")
    );
    assert_eq!(
        first.stopped.unwrap().lot.parent_monday_id.as_deref(),
        Some("12406856533")
    );
    k.clock().advance(600);
    let second = k.back(Source::ClaudeCode).unwrap();
    assert_eq!(
        second.started.lot.parent_monday_id.as_deref(),
        Some("12406856533")
    );
}

#[test]
fn back_with_nothing_running_resumes_the_last_lot() {
    let (k, _d) = kairos();
    k.start(lot("12406850311")).unwrap();
    k.clock().advance(600);
    k.stop(None).unwrap();
    k.clock().advance(60);
    let back = k.back(Source::Manual).unwrap();
    assert!(back.stopped.is_none());
    assert_eq!(
        back.started.lot.parent_monday_id.as_deref(),
        Some("12406850311")
    );
}

#[test]
fn back_without_previous_lot_fails() {
    let (k, _d) = kairos();
    assert_eq!(code(k.back(Source::Manual)), "no_previous_lot");
    k.start(lot("12406850311")).unwrap();
    k.clock().advance(60);
    assert_eq!(code(k.back(Source::Manual)), "no_previous_lot");
}

#[test]
fn set_label_sets_and_replaces_and_survives_a_start_without_label() {
    let (k, _d) = kairos();
    k.start(lot("12406850311")).unwrap();
    k.set_label("12406850311", "Bundle").unwrap();
    assert_eq!(label_of(&k), Some("Bundle".into()));
    k.set_label("12406850311", "Bundle v2").unwrap();
    assert_eq!(label_of(&k), Some("Bundle v2".into()));
    k.clock().advance(60);
    k.stop(None).unwrap();
    k.start(lot("12406850311")).unwrap();
    assert_eq!(label_of(&k), Some("Bundle v2".into()));
}

fn label_of(k: &Kairos<FakeClock>) -> Option<String> {
    k.recent_lots(8).unwrap()[0].lot.parent_label.clone()
}

#[test]
fn set_label_rejects_unknown_parent_and_bad_labels() {
    let (k, _d) = kairos();
    assert_eq!(
        code(k.set_label("12406850311", "Bundle")),
        "parent_not_found"
    );
    k.start(lot("12406850311")).unwrap();
    for bad in ["", "   ", "deux\nlignes", &"x".repeat(201)] {
        assert_eq!(
            code(k.set_label("12406850311", bad)),
            "invalid_name",
            "{bad:?}"
        );
    }
    assert_eq!(
        code(k.set_label("pas-un-id", "Bundle")),
        "invalid_monday_id"
    );
}

#[test]
fn back_during_a_pause_goes_to_the_lot_before_the_paused_one() {
    let (k, _d) = kairos();
    k.start(lot("12406850311")).unwrap();
    k.clock().advance(600);
    k.switch_to(lot("12406856533")).unwrap();
    k.clock().advance(600);
    k.pause().unwrap();
    k.clock().advance(60);
    // `recent` marque B comme lot courant (en pause) : `back` doit donc aller sur A, pas relancer B.
    assert!(k.recent_lots(8).unwrap()[0].current);
    let back = k.back(Source::ClaudeCode).unwrap();
    assert_eq!(
        back.started.lot.parent_monday_id.as_deref(),
        Some("12406850311")
    );
}
