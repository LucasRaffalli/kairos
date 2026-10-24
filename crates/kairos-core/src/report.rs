//! La ventilation d'une journée : lignes par parent et sous-tâche, arrondi, recoupement
//! avec les bornes, texte à saisir dans Monday. Fonctions pures, sans base de données.

use serde::Serialize;

use crate::model::{DayStatus, Totals};
use crate::timeline::format_duration;

/// Ce qu'il faut savoir d'une entrée pour la ventiler (déjà sans les entrées retirées, dans l'ordre).
#[derive(Debug, Clone)]
pub struct ReportEntry {
    pub id: i64,
    pub start: i64,
    /// Fin de l'entrée ; pour une entrée ouverte, « maintenant ».
    pub end: i64,
    pub parent_monday_id: Option<String>,
    pub subtask: Option<String>,
    pub name: Option<String>,
}

/// Un parent (ID Monday) et ses sous-tâches, durées arrondies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ParentLine {
    pub monday_id: String,
    pub subtasks: Vec<(String, i64)>,
    pub total: i64,
}

/// Un chronomètre sans ID Monday : l'entrée, avec ses heures.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NoIdEntry {
    pub id: i64,
    pub start: i64,
    pub end: i64,
    pub secs: i64,
}

/// Un nom de tâche sans ID Monday : sa durée réelle, la durée arrondie qui sera saisie, et chacune
/// de ses entrées. Dans le texte à saisir, tous les noms se regroupent en une seule ligne « sans ID ».
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NoIdLine {
    pub name: String,
    pub secs: i64,
    pub rounded: i64,
    pub entries: Vec<NoIdEntry>,
}

/// Une ligne qui a bougé depuis la saisie dans Monday.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Change {
    pub monday_id: String,
    pub subtask: String,
    pub from: i64,
    pub to: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Report {
    pub date: String,
    pub status: DayStatus,
    pub start: Option<i64>,
    pub end: Option<i64>,
    pub pauses: i64,
    /// Le temps réellement chronométré (jamais arrondi).
    pub total: i64,
    pub lines: Vec<ParentLine>,
    pub no_id: Vec<NoIdLine>,
    /// Le total « sans ID » : la somme des durées arrondies de chaque nom.
    pub no_id_total: i64,
    pub rounding_minutes: i64,
    /// La somme des lignes telles qu'elles seront saisies.
    pub lines_total: i64,
    /// `(fin − début − pauses) − somme des lignes` : nul sans arrondi, signalé sinon.
    pub gap: i64,
    /// La ligne à saisir dans Monday.
    pub text: String,
    pub changes: Vec<Change>,
}

/// Arrondit à `step_minutes` (la moitié vers le haut), jamais en dessous du pas pour une durée
/// non nulle : une ligne de 1 minute ne disparaît pas. Un pas de 0 rend la durée telle quelle.
pub fn round_to(secs: i64, step_minutes: i64) -> i64 {
    if step_minutes <= 0 {
        return secs.max(0);
    }
    if secs <= 0 {
        return 0;
    }
    let step = step_minutes * 60;
    (((secs + step / 2) / step) * step).max(step)
}

const NO_ID: &str = "sans ID";

pub fn build(
    date: &str,
    status: DayStatus,
    entries: &[ReportEntry],
    rounding_minutes: i64,
    declared: Option<&Totals>,
) -> Report {
    // Les durées réelles, dans l'ordre de première apparition.
    let mut parents: Vec<(String, Vec<(String, i64)>)> = Vec::new();
    let mut no_id: Vec<NoIdLine> = Vec::new();
    for entry in entries {
        let secs = entry.end - entry.start;
        match &entry.parent_monday_id {
            Some(id) => {
                let subtask = entry.subtask.clone().unwrap_or_else(|| "?".into());
                let parent = match parents.iter().position(|(p, _)| p == id) {
                    Some(i) => i,
                    None => {
                        parents.push((id.clone(), Vec::new()));
                        parents.len() - 1
                    }
                };
                add_to(&mut parents[parent].1, subtask, secs);
            }
            None => {
                let name = entry
                    .name
                    .clone()
                    .or_else(|| entry.subtask.clone())
                    .unwrap_or_else(|| "?".into());
                let timer = NoIdEntry {
                    id: entry.id,
                    start: entry.start,
                    end: entry.end,
                    secs,
                };
                match no_id.iter_mut().find(|n| n.name == name) {
                    Some(line) => {
                        line.secs += secs;
                        line.entries.push(timer);
                    }
                    None => no_id.push(NoIdLine {
                        name,
                        secs,
                        rounded: 0,
                        entries: vec![timer],
                    }),
                }
            }
        }
    }
    for line in &mut no_id {
        line.rounded = round_to(line.secs, rounding_minutes);
    }

    let total: i64 = entries.iter().map(|e| e.end - e.start).sum();
    let start = entries.first().map(|e| e.start);
    let end = entries.last().map(|e| e.end);
    let pauses: i64 = entries
        .windows(2)
        .map(|w| (w[1].start - w[0].end).max(0))
        .sum();

    let lines: Vec<ParentLine> = parents
        .iter()
        .map(|(id, subs)| {
            let subtasks: Vec<(String, i64)> = subs
                .iter()
                .map(|(name, secs)| (name.clone(), round_to(*secs, rounding_minutes)))
                .collect();
            let total = subtasks.iter().map(|(_, s)| s).sum();
            ParentLine {
                monday_id: id.clone(),
                subtasks,
                total,
            }
        })
        .collect();
    let no_id_total = no_id.iter().map(|n| n.rounded).sum();
    let lines_total = lines.iter().map(|l| l.total).sum::<i64>() + no_id_total;
    let gap = match (start, end) {
        (Some(start), Some(end)) => (end - start - pauses) - lines_total,
        _ => 0,
    };

    let mut tokens: Vec<String> = Vec::new();
    for line in &lines {
        for (i, (name, secs)) in line.subtasks.iter().enumerate() {
            let prefix = if i == 0 {
                format!("{} : ", line.monday_id)
            } else {
                String::new()
            };
            tokens.push(format!("{prefix}{name} {}", format_duration(*secs)));
        }
    }
    if !no_id.is_empty() {
        tokens.push(format!("{NO_ID} : {}", format_duration(no_id_total)));
    }

    let changes = declared
        .map(|declared| diff(declared, &real_totals(&parents, &no_id)))
        .unwrap_or_default();
    Report {
        date: date.to_string(),
        status,
        start,
        end,
        pauses,
        total,
        lines,
        no_id,
        no_id_total,
        rounding_minutes,
        lines_total,
        gap,
        text: tokens.join(" · "),
        changes,
    }
}

fn add_to(subs: &mut Vec<(String, i64)>, name: String, secs: i64) {
    match subs.iter_mut().find(|(n, _)| *n == name) {
        Some((_, total)) => *total += secs,
        None => subs.push((name, secs)),
    }
}

fn real_totals(parents: &[(String, Vec<(String, i64)>)], no_id: &[NoIdLine]) -> Totals {
    let mut totals = Totals::new();
    for (id, subs) in parents {
        totals
            .entry(id.clone())
            .or_default()
            .extend(subs.iter().cloned());
    }
    for line in no_id {
        *totals
            .entry(NO_ID.to_string())
            .or_default()
            .entry(line.name.clone())
            .or_insert(0) += line.secs;
    }
    totals
}

/// Ce qui a bougé entre les totaux déclarés et les totaux réels, trié par parent puis sous-tâche.
fn diff(declared: &Totals, real: &Totals) -> Vec<Change> {
    let mut parents: Vec<&String> = declared.keys().chain(real.keys()).collect();
    parents.sort();
    parents.dedup();
    let mut changes = Vec::new();
    for parent in parents {
        let (before, after) = (declared.get(parent), real.get(parent));
        let mut subs: Vec<&String> = before
            .into_iter()
            .flat_map(|m| m.keys())
            .chain(after.into_iter().flat_map(|m| m.keys()))
            .collect();
        subs.sort();
        subs.dedup();
        for sub in subs {
            let from = before.and_then(|m| m.get(sub)).copied().unwrap_or(0);
            let to = after.and_then(|m| m.get(sub)).copied().unwrap_or(0);
            if from != to {
                changes.push(Change {
                    monday_id: parent.clone(),
                    subtask: sub.clone(),
                    from,
                    to,
                });
            }
        }
    }
    changes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounding_is_half_up_and_never_below_the_step_for_a_non_zero_duration() {
        assert_eq!(round_to(60, 15), 900);
        assert_eq!(round_to(449, 15), 900);
        assert_eq!(round_to(450, 15), 900);
        assert_eq!(round_to(1349, 15), 900);
        assert_eq!(round_to(1350, 15), 1800);
        assert_eq!(round_to(0, 15), 0);
        assert_eq!(round_to(150, 5), 300);
        assert_eq!(
            round_to(7800, 0),
            7800,
            "pas d'arrondi : la durée est rendue telle quelle"
        );
        assert_eq!(round_to(-5, 15), 0);
    }
}
