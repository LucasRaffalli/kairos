//! La sortie : du texte français lisible, ou du JSON stable.

use kairos_core::model::{
    Day, DayStatus, DayView, Entry, RecentLot, StartedLot, Status, StoppedEntry, SwitchResult,
};
use kairos_core::report::Report;
use kairos_core::timeline::{format_duration, local_date};
use serde_json::{json, Value};

pub enum Outcome {
    Started(StartedLot),
    Resumed(StartedLot),
    Stopped(StoppedEntry),
    Paused(StoppedEntry),
    Switched(SwitchResult),
    Status(Status),
    Today(DayView),
    Report(Report),
    Closed(Day),
    Declared(Day),
    EntryAdded(Entry),
    EntryEdited(Entry),
    EntryDeleted(i64),
    SubtaskId { parent: String, subtask: String },
    Recent(Vec<RecentLot>),
    Label { parent: String, label: String },
    Config { key: String, value: String },
}

/// « 13 h 34 » : l'heure locale d'un instant.
fn hm(at: i64, offset: &dyn Fn(i64) -> i32) -> String {
    let secs = (at + i64::from(offset(at))).rem_euclid(86_400);
    format!("{} h {:02}", secs / 3600, (secs % 3600) / 60)
}

fn status_fr(status: DayStatus) -> &'static str {
    match status {
        DayStatus::Open => "ouverte",
        DayStatus::Closed => "clôturée",
        DayStatus::Declared => "saisie",
        DayStatus::ToRedeclare => "à resaisir",
    }
}

fn bounds_line(
    start: Option<i64>,
    end: Option<i64>,
    pauses: i64,
    total: i64,
    offset: &dyn Fn(i64) -> i32,
) -> String {
    match (start, end) {
        (Some(s), Some(e)) => format!(
            "Bornes : {} → {} · pauses {} · total {}",
            hm(s, offset),
            hm(e, offset),
            format_duration(pauses),
            format_duration(total)
        ),
        _ => "Aucune entrée.".to_string(),
    }
}

pub fn text(outcome: &Outcome, offset: &dyn Fn(i64) -> i32) -> String {
    match outcome {
        Outcome::Started(s) => format!(
            "Lot lancé : {} à {}.",
            s.lot.describe(),
            hm(s.entry.start, offset)
        ),
        Outcome::Resumed(s) => format!(
            "Reprise : {} à {}.",
            s.lot.describe(),
            hm(s.entry.start, offset)
        ),
        Outcome::Stopped(s) if s.duration > 0 => format!(
            "Lot arrêté : {}, {}.",
            s.lot.describe(),
            format_duration(s.duration)
        ),
        Outcome::Stopped(s) => format!("Lot arrêté : {} (rien de chronométré).", s.lot.describe()),
        Outcome::Paused(s) => format!(
            "En pause : {} ({} sur cette entrée).",
            s.lot.describe(),
            format_duration(s.duration)
        ),
        Outcome::Switched(r) => {
            let first = match &r.stopped {
                Some(s) => format!(
                    "Arrêté : {} ({}). ",
                    s.lot.describe(),
                    format_duration(s.duration)
                ),
                None => String::new(),
            };
            format!(
                "{first}Lancé : {} à {}.",
                r.started.lot.describe(),
                hm(r.started.entry.start, offset)
            )
        }
        Outcome::Status(Status::Idle) => "Rien en cours.".to_string(),
        Outcome::Status(Status::Running {
            lot,
            since,
            duration,
            today_total,
            ..
        }) => {
            format!(
                "En cours : {} depuis {} ({}). Aujourd'hui : {}.",
                lot.describe(),
                hm(*since, offset),
                format_duration(*duration),
                format_duration(*today_total)
            )
        }
        Outcome::Status(Status::Paused { lot, since, .. }) => format!(
            "En pause : {} depuis {}.",
            lot.describe(),
            hm(*since, offset)
        ),
        Outcome::Today(day) => {
            let mut out = format!("{} · {}\n", day.date, status_fr(day.status));
            for e in &day.entries {
                let end = e.end.map_or("en cours".to_string(), |end| hm(end, offset));
                let secs = e.end.unwrap_or(day.end.unwrap_or(e.start)) - e.start;
                let removed = if e.removed { "  (retiré)" } else { "" };
                out.push_str(&format!(
                    "  #{}  {} → {}  {}  {}{removed}\n",
                    e.id,
                    hm(e.start, offset),
                    end,
                    e.lot.describe(),
                    format_duration(secs)
                ));
            }
            out.push_str(&bounds_line(
                day.start, day.end, day.pauses, day.total, offset,
            ));
            out
        }
        Outcome::Report(r) => report_text(r, offset),
        Outcome::Closed(d) => format!(
            "Journée {} clôturée : {}.",
            d.date,
            format_duration(d.totals.values().flat_map(|m| m.values()).sum())
        ),
        Outcome::Declared(d) => format!("Journée {} marquée saisie dans Monday.", d.date),
        Outcome::EntryAdded(e) => format!(
            "Entrée #{} ajoutée : {} → {}.",
            e.id,
            hm(e.start, offset),
            e.end.map_or("en cours".into(), |end| hm(end, offset))
        ),
        Outcome::EntryEdited(e) => format!(
            "Entrée #{} modifiée : {} → {}.",
            e.id,
            hm(e.start, offset),
            e.end.map_or("en cours".into(), |end| hm(end, offset))
        ),
        Outcome::EntryDeleted(id) => format!("Entrée #{id} supprimée."),
        Outcome::SubtaskId { parent, subtask } => {
            format!("ID Monday enregistré pour {subtask} sous {parent}.")
        }
        Outcome::Config { key, value } => format!("{key} = {value}"),
        Outcome::Recent(lots) if lots.is_empty() => "Aucun lot récent.".to_string(),
        Outcome::Recent(lots) => lots
            .iter()
            .map(|r| {
                let name = match &r.lot.parent_label {
                    Some(label) => format!("{label} — {}", r.lot.describe()),
                    None => r.lot.describe(),
                };
                // Sans temps aujourd'hui, le lot a fini un autre jour : on le dit.
                let end = if r.today_total == 0 {
                    format!(
                        "{} {}",
                        local_date(r.last_end, offset),
                        hm(r.last_end, offset)
                    )
                } else {
                    hm(r.last_end, offset)
                };
                format!(
                    "{} {name} · {} aujourd'hui · fin {end}",
                    if r.current { "▶" } else { " " },
                    format_duration(r.today_total),
                )
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Outcome::Label { parent, label } => {
            format!("Le parent {parent} s'appelle maintenant « {label} ».")
        }
    }
}

fn report_text(r: &Report, offset: &dyn Fn(i64) -> i32) -> String {
    let mut out = format!(
        "Journée {} ({})\n{}\n",
        r.date,
        status_fr(r.status),
        bounds_line(r.start, r.end, r.pauses, r.total, offset)
    );
    for line in &r.lines {
        let subs: Vec<String> = line
            .subtasks
            .iter()
            .map(|(n, s)| format!("{n} {}", format_duration(*s)))
            .collect();
        out.push_str(&format!("  {}  {}\n", line.monday_id, subs.join(" · ")));
    }
    if !r.no_id.is_empty() {
        out.push_str("  sans ID\n");
        for line in &r.no_id {
            let entered = if line.rounded == line.secs {
                String::new()
            } else {
                format!(" (saisi {})", format_duration(line.rounded))
            };
            out.push_str(&format!(
                "    {}  {}{entered}\n",
                line.name,
                format_duration(line.secs)
            ));
            for e in &line.entries {
                out.push_str(&format!(
                    "      #{}  {} → {}  {}\n",
                    e.id,
                    hm(e.start, offset),
                    hm(e.end, offset),
                    format_duration(e.secs)
                ));
            }
        }
    }
    if r.rounding_minutes > 0 || r.gap != 0 {
        let sign = if r.gap < 0 { "−" } else { "" };
        out.push_str(&format!(
            "Arrondi : {} min · écart avec les bornes : {sign}{}\n",
            r.rounding_minutes,
            format_duration(r.gap.abs())
        ));
    }
    for c in &r.changes {
        out.push_str(&format!(
            "À resaisir : {} : {} {} → {}\n",
            c.monday_id,
            c.subtask,
            format_duration(c.from),
            format_duration(c.to)
        ));
    }
    if r.text.is_empty() {
        out.push_str("Rien à saisir.");
    } else {
        out.push_str(&format!("À saisir dans Monday :\n{}", r.text));
    }
    out
}

fn value<T: serde::Serialize>(x: &T) -> Value {
    serde_json::to_value(x).expect("un type du cœur se sérialise toujours")
}

pub fn to_json(outcome: &Outcome) -> Value {
    match outcome {
        Outcome::Started(s) | Outcome::Resumed(s) => value(s),
        Outcome::Stopped(s) | Outcome::Paused(s) => value(s),
        Outcome::Switched(r) => value(r),
        Outcome::Status(s) => value(s),
        Outcome::Today(d) => value(d),
        Outcome::Report(r) => value(r),
        Outcome::Closed(d) | Outcome::Declared(d) => value(d),
        Outcome::EntryAdded(e) | Outcome::EntryEdited(e) => value(e),
        Outcome::EntryDeleted(id) => json!({ "deleted": id }),
        Outcome::SubtaskId { parent, subtask } => {
            json!({ "parent": parent, "subtask": subtask, "saved": true })
        }
        Outcome::Config { key, value } => json!({ "key": key, "value": value }),
        Outcome::Recent(lots) => value(lots),
        Outcome::Label { parent, label } => {
            json!({ "parent": parent, "label": label, "saved": true })
        }
    }
}
