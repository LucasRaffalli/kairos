//! Cœur de Kairos. Toutes les règles métier vivent ici, pour que l'app et la
//! ligne de commande donnent les mêmes refus et les mêmes messages.

pub mod clock;
pub mod db;
pub mod error;
pub mod model;
pub mod timeline;

/// Version du cœur, celle du workspace.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_matches_cargo() {
        assert_eq!(version(), "0.1.0");
    }
}
