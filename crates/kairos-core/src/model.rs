//! Les types partagés du cœur.

use serde::Serialize;

/// Qui a lancé le lot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Source {
    Manual,
    ClaudeCode,
    ClaudeDesktop,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Source::Manual => "manual",
            Source::ClaudeCode => "claude-code",
            Source::ClaudeDesktop => "claude-desktop",
        }
    }

    pub fn parse(text: &str) -> Option<Source> {
        match text {
            "manual" => Some(Source::Manual),
            "claude-code" => Some(Source::ClaudeCode),
            "claude-desktop" => Some(Source::ClaudeDesktop),
            _ => None,
        }
    }
}

/// L'état d'une journée (voir le diagramme du spec).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DayStatus {
    Open,
    Closed,
    Declared,
    ToRedeclare,
}

impl DayStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            DayStatus::Open => "open",
            DayStatus::Closed => "closed",
            DayStatus::Declared => "declared",
            DayStatus::ToRedeclare => "to_redeclare",
        }
    }

    pub fn parse(text: &str) -> Option<DayStatus> {
        match text {
            "open" => Some(DayStatus::Open),
            "closed" => Some(DayStatus::Closed),
            "declared" => Some(DayStatus::Declared),
            "to_redeclare" => Some(DayStatus::ToRedeclare),
            _ => None,
        }
    }
}

/// Un lot, avec ce qu'il faut pour l'afficher.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LotView {
    pub id: i64,
    pub parent_monday_id: Option<String>,
    pub parent_label: Option<String>,
    pub subtask: Option<String>,
    pub name: Option<String>,
}

impl LotView {
    /// Le libellé court d'un lot : « Front sur 12406850311 », « Front · Refonte », ou le nom seul.
    pub fn describe(&self) -> String {
        let target = match (&self.parent_monday_id, &self.name) {
            (Some(id), _) => id.clone(),
            (None, Some(name)) => name.clone(),
            (None, None) => "?".to_string(),
        };
        match &self.subtask {
            Some(sub) if self.parent_monday_id.is_some() => format!("{sub} sur {target}"),
            Some(sub) => format!("{sub} · {target}"),
            None => target,
        }
    }
}

/// Une plage de temps passée sur un lot. `end` vide : l'entrée est ouverte.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Entry {
    pub id: i64,
    pub lot_id: i64,
    pub start: i64,
    pub end: Option<i64>,
    pub removed: bool,
    pub note: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_and_status_roundtrip_through_their_text_form() {
        for s in [Source::Manual, Source::ClaudeCode, Source::ClaudeDesktop] {
            assert_eq!(Source::parse(s.as_str()), Some(s));
        }
        for d in [
            DayStatus::Open,
            DayStatus::Closed,
            DayStatus::Declared,
            DayStatus::ToRedeclare,
        ] {
            assert_eq!(DayStatus::parse(d.as_str()), Some(d));
        }
        assert_eq!(Source::parse("robot"), None);
        assert_eq!(DayStatus::parse("fermée"), None);
    }

    #[test]
    fn json_forms_match_the_text_forms() {
        assert_eq!(
            serde_json::to_string(&Source::ClaudeCode).unwrap(),
            "\"claude-code\""
        );
        assert_eq!(
            serde_json::to_string(&DayStatus::ToRedeclare).unwrap(),
            "\"to_redeclare\""
        );
    }

    #[test]
    fn a_lot_is_described_the_short_way() {
        let lot = |id: Option<&str>, name: Option<&str>, sub: Option<&str>| LotView {
            id: 1,
            parent_monday_id: id.map(str::to_string),
            parent_label: None,
            subtask: sub.map(str::to_string),
            name: name.map(str::to_string),
        };
        assert_eq!(
            lot(Some("12406850311"), None, Some("Front")).describe(),
            "Front sur 12406850311"
        );
        assert_eq!(
            lot(None, Some("Réunion"), Some("Front")).describe(),
            "Front · Réunion"
        );
        assert_eq!(lot(None, Some("Réunion"), None).describe(), "Réunion");
    }
}
