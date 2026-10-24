//! Les erreurs du cœur : un variant par cas métier, un `code()` stable pour le
//! JSON de la ligne de commande, et un message français qui dit **comment corriger**.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// Un lot tourne déjà (`lot` : son libellé, `since` : l'heure de départ).
    AlreadyRunning {
        lot: String,
        since: String,
    },
    NothingRunning,
    /// Chevauche une entrée existante (`with` : sa description).
    Overlap {
        with: String,
    },
    InFuture {
        at: String,
        now: String,
    },
    CrossesMidnight,
    EmptyEntry,
    ReferenceMissing,
    InvalidMondayId(String),
    /// Nom de tâche refusé, avec la raison.
    InvalidName(String),
    InvalidDate(String),
    InvalidTime(String),
    /// Un lancement qui remonte à plus de 2 jours (`at` : date et heure demandées).
    TooOld {
        at: String,
    },
    /// L'horloge est passée sous le début du lot ouvert.
    ClockBehind {
        since: String,
    },
    SameLot(String),
    DayNotOpen(String),
    DayNotClosed(String),
    DayEmpty(String),
    RunningLotOnClose(String),
    EntryNotFound(i64),
    NothingToResume,
    /// `back` sans lot précédent à reprendre.
    NoPreviousLot,
    /// Aucun parent connu avec cet ID Monday.
    ParentNotFound(String),
    InvalidSetting(String),
    DbNewer,
    DbUnavailable,
    Internal(String),
}

impl Error {
    /// Identifiant stable, pour le JSON de la ligne de commande.
    pub fn code(&self) -> &'static str {
        match self {
            Error::AlreadyRunning { .. } => "already_running",
            Error::NothingRunning => "nothing_running",
            Error::Overlap { .. } => "overlap",
            Error::InFuture { .. } => "in_future",
            Error::CrossesMidnight => "crosses_midnight",
            Error::EmptyEntry => "empty_entry",
            Error::ReferenceMissing => "reference_missing",
            Error::InvalidMondayId(_) => "invalid_monday_id",
            Error::InvalidName(_) => "invalid_name",
            Error::InvalidDate(_) => "invalid_date",
            Error::InvalidTime(_) => "invalid_time",
            Error::ClockBehind { .. } => "clock_behind",
            Error::TooOld { .. } => "too_old",
            Error::SameLot(_) => "same_lot",
            Error::DayNotOpen(_) => "day_not_open",
            Error::DayNotClosed(_) => "day_not_closed",
            Error::DayEmpty(_) => "day_empty",
            Error::RunningLotOnClose(_) => "running_lot_on_close",
            Error::EntryNotFound(_) => "entry_not_found",
            Error::NothingToResume => "nothing_to_resume",
            Error::NoPreviousLot => "no_previous_lot",
            Error::ParentNotFound(_) => "parent_not_found",
            Error::InvalidSetting(_) => "invalid_setting",
            Error::DbNewer => "db_newer",
            Error::DbUnavailable => "db_unavailable",
            Error::Internal(_) => "internal",
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::AlreadyRunning { lot, since } => write!(f, "Un lot est déjà en cours : {lot} depuis {since}. `kairos switch` l'arrête et lance le suivant."),
            Error::NothingRunning => write!(f, "Aucun lot en cours."),
            Error::Overlap { with } => write!(f, "Chevauche {with} : raccourcis cette entrée ou l'autre."),
            Error::InFuture { at, now } => write!(f, "{at} est dans le futur (il est {now})."),
            Error::CrossesMidnight => write!(f, "Cette entrée traverse minuit : scinde-la en deux, une par jour."),
            Error::EmptyEntry => write!(f, "Une entrée doit durer au moins une seconde : la fin doit venir après le début."),
            Error::ReferenceMissing => write!(f, "Il faut un ID Monday ou un nom de tâche."),
            Error::InvalidMondayId(id) => write!(f, "L'ID Monday s'écrit avec des chiffres seulement, de 1 à 20 (par exemple 12406850311) : « {id} » n'est pas valide."),
            Error::InvalidName(why) => write!(f, "Nom de tâche invalide : {why}."),
            Error::InvalidDate(d) => write!(f, "« {d} » n'est pas une date valide : écris-la AAAA-MM-JJ, par exemple 2026-10-23."),
            Error::InvalidTime(t) => write!(f, "« {t} » n'est pas une heure valide : écris 13:30, 2026-10-23T13:30 ou 2026-10-23T13:30:00+02:00."),
            Error::ClockBehind { since } => write!(f, "L'horloge est en retard sur le début du lot ({since}) : corrige l'heure ou utilise --at."),
            Error::TooOld { at } => write!(f, "Un lancement ne peut pas remonter à plus de 2 jours ({at}) : utilise `kairos add` pour un temps plus ancien."),
            Error::SameLot(lot) => write!(f, "Ce lot tourne déjà : {lot}."),
            Error::DayNotOpen(d) => write!(f, "La journée {d} est déjà clôturée."),
            Error::DayNotClosed(d) => write!(f, "La journée {d} n'est pas clôturée : clôture-la d'abord (kairos close-day)."),
            Error::DayEmpty(d) => write!(f, "Rien à clôturer le {d} : aucune entrée."),
            Error::RunningLotOnClose(lot) => write!(f, "Un lot est en cours ({lot}) : arrête-le d'abord, ou ajoute --stop pour l'arrêter et clôturer."),
            Error::EntryNotFound(id) => write!(f, "Aucune entrée n°{id} : `kairos today` liste les numéros."),
            Error::NothingToResume => write!(f, "Rien à reprendre : aucune pause en cours."),
            Error::NoPreviousLot => write!(f, "Aucun lot précédent à reprendre."),
            Error::ParentNotFound(id) => write!(f, "Aucun parent avec l'ID {id}."),
            Error::InvalidSetting(why) => write!(f, "Réglage invalide : {why}."),
            Error::DbNewer => write!(f, "Cette base a été créée par une version plus récente de Kairos : mets Kairos à jour."),
            Error::DbUnavailable => write!(f, "La base est verrouillée par un autre processus depuis plus de 5 secondes : réessaie dans un instant."),
            Error::Internal(why) => write!(f, "Erreur interne : {why}."),
        }
    }
}

impl std::error::Error for Error {}

impl From<rusqlite::Error> for Error {
    fn from(error: rusqlite::Error) -> Self {
        use rusqlite::ErrorCode::{DatabaseBusy, DatabaseLocked};
        match &error {
            rusqlite::Error::SqliteFailure(e, _)
                if matches!(e.code, DatabaseBusy | DatabaseLocked) =>
            {
                Error::DbUnavailable
            }
            _ => Error::Internal(error.to_string()),
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Error::Internal(error.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    fn all() -> Vec<(Error, &'static str)> {
        vec![
            (
                Error::AlreadyRunning {
                    lot: "Front sur 1".into(),
                    since: "13 h 34".into(),
                },
                "already_running",
            ),
            (Error::NothingRunning, "nothing_running"),
            (
                Error::Overlap {
                    with: "14 h 10 – 15 h 02".into(),
                },
                "overlap",
            ),
            (
                Error::InFuture {
                    at: "17 h 40".into(),
                    now: "15 h 02".into(),
                },
                "in_future",
            ),
            (Error::CrossesMidnight, "crosses_midnight"),
            (Error::EmptyEntry, "empty_entry"),
            (Error::ReferenceMissing, "reference_missing"),
            (Error::InvalidMondayId("12a".into()), "invalid_monday_id"),
            (
                Error::InvalidName("plusieurs lignes".into()),
                "invalid_name",
            ),
            (Error::InvalidDate("hier".into()), "invalid_date"),
            (Error::InvalidTime("25:61".into()), "invalid_time"),
            (
                Error::ClockBehind {
                    since: "13 h 34".into(),
                },
                "clock_behind",
            ),
            (Error::SameLot("Front sur 1".into()), "same_lot"),
            (Error::DayNotOpen("2026-10-23".into()), "day_not_open"),
            (Error::DayNotClosed("2026-10-23".into()), "day_not_closed"),
            (Error::DayEmpty("2026-10-23".into()), "day_empty"),
            (
                Error::RunningLotOnClose("Front sur 1".into()),
                "running_lot_on_close",
            ),
            (Error::EntryNotFound(7), "entry_not_found"),
            (Error::NothingToResume, "nothing_to_resume"),
            (Error::NoPreviousLot, "no_previous_lot"),
            (
                Error::ParentNotFound("12406850311".into()),
                "parent_not_found",
            ),
            (
                Error::InvalidSetting("rounding_minutes = 7".into()),
                "invalid_setting",
            ),
            (Error::DbNewer, "db_newer"),
            (Error::DbUnavailable, "db_unavailable"),
            (Error::Internal("boum".into()), "internal"),
        ]
    }

    #[test]
    fn codes_are_stable() {
        for (error, code) in all() {
            assert_eq!(error.code(), code);
        }
    }

    #[test]
    fn every_message_is_a_complete_french_sentence() {
        for (error, _) in all() {
            let message = error.to_string();
            assert!(message.ends_with('.'), "{message}");
            assert!(message.len() > 12, "{message}");
            assert!(
                !message.contains('{') && !message.contains("Error"),
                "message brut : {message}"
            );
        }
    }

    #[test]
    fn busy_sqlite_maps_to_db_unavailable() {
        let busy = rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_BUSY),
            None,
        );
        assert_eq!(Error::from(busy), Error::DbUnavailable);
        let other = rusqlite::Error::QueryReturnedNoRows;
        assert!(matches!(Error::from(other), Error::Internal(_)));
    }
}
