//! Les erreurs du cœur. Étendu à la tâche 2 ; ici, seulement ce que le calendrier produit.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// Une date qui n'est pas `AAAA-MM-JJ`.
    InvalidDate(String),
}

impl Error {
    /// Identifiant stable, pour le JSON de la ligne de commande.
    pub fn code(&self) -> &'static str {
        match self {
            Error::InvalidDate(_) => "invalid_date",
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::InvalidDate(d) => write!(
                f,
                "« {d} » n'est pas une date valide : écris-la AAAA-MM-JJ, par exemple 2026-10-23."
            ),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;
