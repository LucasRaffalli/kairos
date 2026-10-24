//! Les types partagés du cœur.

use serde::Serialize;

/// Qui a lancé le lot.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Source {
    #[default]
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
    /// L'ID Monday de la sous-tâche, quand il est connu (posé au lancement ou par `set_subtask_id`).
    pub subtask_monday_id: Option<String>,
    pub name: Option<String>,
}

impl LotView {
    /// Ce sur quoi porte le lot : l'ID Monday du parent, à défaut le nom de la tâche.
    pub fn target(&self) -> String {
        match (&self.parent_monday_id, &self.name) {
            (Some(id), _) => id.clone(),
            (None, Some(name)) => name.clone(),
            (None, None) => "?".to_string(),
        }
    }

    /// Le libellé court d'un lot : « Front sur 12406850311 », « Front · Refonte », ou le nom seul.
    pub fn describe(&self) -> String {
        let target = self.target();
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
    /// Qui a lancé l'entrée (vide pour une entrée ajoutée après coup ou d'avant la migration 3).
    pub source: Option<Source>,
    /// Le dossier de travail du lancement.
    pub workdir: Option<String>,
}

/// Ce qu'il faut pour lancer un lot (aussi pour `switch`).
#[derive(Debug, Clone, Default)]
pub struct StartArgs {
    /// ID Monday du parent (chiffres, 1 à 20).
    pub parent_monday_id: Option<String>,
    /// Nom affiché du parent, mémorisé avec son ID.
    pub parent_label: Option<String>,
    /// Nom de la tâche, quand il n'y a pas d'ID Monday.
    pub name: Option<String>,
    /// Sous-tâche ; à défaut, le poste principal quand il y a un ID.
    pub subtask: Option<String>,
    /// ID Monday de la sous-tâche, collé à la main.
    pub subtask_monday_id: Option<String>,
    /// Heure de début, pour un lancement en retard (vide : maintenant).
    pub at: Option<i64>,
    pub source: Source,
    /// Dossier de travail du lancement (nettoyé : jamais une erreur, au pire ignoré).
    pub workdir: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StartedLot {
    pub lot: LotView,
    pub entry: Entry,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StoppedEntry {
    pub lot: LotView,
    pub entry: Entry,
    /// Secondes passées sur cette entrée.
    pub duration: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SwitchResult {
    pub stopped: Option<StoppedEntry>,
    pub started: StartedLot,
}

/// Ce que fait Kairos à cet instant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Status {
    Idle,
    Running {
        lot: LotView,
        since: i64,
        duration: i64,
        today_total: i64,
        launched_by: Option<Source>,
        workdir: Option<String>,
    },
    Paused {
        lot: LotView,
        /// Début de la pause : la fin de la dernière entrée.
        since: i64,
        /// Secondes de la dernière entrée, figées par la pause.
        duration: i64,
        launched_by: Option<Source>,
        workdir: Option<String>,
    },
}

/// Un temps ajouté après coup.
#[derive(Debug, Clone, Default)]
pub struct AddEntry {
    pub parent_monday_id: Option<String>,
    pub parent_label: Option<String>,
    pub name: Option<String>,
    pub subtask: Option<String>,
    pub from: i64,
    pub to: i64,
}

/// Ce qu'on corrige sur une entrée ; ce qui est vide ne change pas.
#[derive(Debug, Clone, Default)]
pub struct EditEntry {
    pub from: Option<i64>,
    pub to: Option<i64>,
    pub parent_monday_id: Option<String>,
    pub name: Option<String>,
    pub subtask: Option<String>,
}

/// Une entrée avec son lot, pour l'affichage d'une journée.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EntryView {
    pub id: i64,
    pub lot: LotView,
    pub start: i64,
    pub end: Option<i64>,
    pub removed: bool,
}

/// Une journée : ses entrées, ses bornes et son total.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DayView {
    pub date: String,
    pub status: DayStatus,
    pub entries: Vec<EntryView>,
    pub start: Option<i64>,
    pub end: Option<i64>,
    /// Somme des trous entre deux entrées.
    pub pauses: i64,
    /// Somme des durées des entrées non retirées.
    pub total: i64,
}

/// Les secondes par parent (ID Monday, ou « sans ID ») puis par sous-tâche.
pub type Totals = std::collections::BTreeMap<String, std::collections::BTreeMap<String, i64>>;

/// L'état d'une journée après une clôture ou une saisie.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Day {
    pub date: String,
    pub status: DayStatus,
    pub closed_at: Option<i64>,
    pub declared_at: Option<i64>,
    pub totals: Totals,
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
            subtask_monday_id: None,
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

/// Ce que Kairos sait d'un ID Monday.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum KnownId {
    /// Un parent connu.
    Parent { label: Option<String> },
    /// L'ID mémorisé d'une sous-tâche : le parent et le nom de la sous-tâche.
    Subtask {
        parent_monday_id: String,
        subtask: String,
        label: Option<String>,
    },
    /// Jamais vu.
    Unknown,
}

/// Les sous-tâches proposées pour un parent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SubtaskChoices {
    /// Le poste principal (réglage `main_subtask`).
    pub main: String,
    /// Les sous-tâches de base, communes à tous les parents.
    pub base: Vec<String>,
    /// Les sous-tâches propres au parent demandé.
    pub own: Vec<String>,
}

/// Un lot récent, avec ce qu'il faut pour le retrouver.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RecentLot {
    pub lot: LotView,
    /// Fin de sa dernière entrée (maintenant pour l'entrée ouverte).
    pub last_end: i64,
    /// Secondes chronométrées aujourd'hui sur ce lot.
    pub today_total: i64,
    /// Le lot en cours ou en pause.
    pub current: bool,
}

/// Tout ce qu'il faut pour dessiner l'app, lu d'un coup (voir `Kairos::overview`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Overview {
    pub status: Status,
    pub today: DayView,
    pub recent: Vec<RecentLot>,
    pub choices: SubtaskChoices,
}
