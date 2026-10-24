//! La surveillance de la base : la ligne de commande, Claude ou l'app elle-même y écrivent ;
//! un fil relève `PRAGMA data_version` chaque seconde et prévient l'interface quand il change.

use std::time::Duration;

use kairos_core::db::Db;
use tauri::{AppHandle, Emitter};

use crate::bridge::OPEN_TIMEOUT;
use crate::tray::MenuTitle;

/// Émis quand la base a changé : l'interface redemande un `snapshot`.
pub const CHANGED: &str = "kairos://changed";

const TICK: Duration = Duration::from_secs(1);

/// La base a-t-elle changé ? Le premier relevé n'est qu'un point de départ.
pub fn changed(previous: Option<i64>, now: i64) -> bool {
    previous.is_some_and(|previous| previous != now)
}

/// Lance le fil de surveillance, qui tient aussi le titre de l'icône à jour.
pub fn spawn(app: AppHandle) -> std::io::Result<()> {
    std::thread::Builder::new()
        .name("kairos-watcher".into())
        .spawn(move || {
            let mut watch = Watch::default();
            let mut title = MenuTitle::default();
            loop {
                let base_changed = watch.poll();
                if base_changed {
                    let _ = app.emit(CHANGED, ());
                }
                title.tick(&app, base_changed);
                std::thread::sleep(TICK);
            }
        })?;
    Ok(())
}

/// Une connexion ouverte une fois, et le dernier `data_version` lu par elle.
#[derive(Default)]
struct Watch {
    db: Option<Db>,
    version: Option<i64>,
}

impl Watch {
    /// Relève `data_version` ; vrai si la base a changé depuis le relevé précédent. Une base
    /// verrouillée ou illisible n'est pas une erreur : on rouvre à la seconde suivante.
    fn poll(&mut self) -> bool {
        let reopened = self.db.is_none();
        if reopened {
            self.db = Db::open_with_timeout(&Db::default_path(), OPEN_TIMEOUT).ok();
        }
        let Some(db) = &self.db else {
            return false;
        };
        match db.data_version() {
            Ok(version) => {
                // Chaque connexion a son propre compteur : après une reconnexion, on ne peut pas
                // comparer, et on a pu manquer une écriture pendant la coupure.
                let change = if reopened {
                    self.version.is_some()
                } else {
                    changed(self.version, version)
                };
                self.version = Some(version);
                change
            }
            Err(_) => {
                self.db = None;
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge;
    use kairos_core::clock::FakeClock;
    use kairos_core::model::StartArgs;
    use kairos_core::service::Kairos;

    #[test]
    fn the_first_reading_is_not_a_change() {
        assert!(!changed(None, 1));
    }

    #[test]
    fn a_different_value_is_a_change() {
        assert!(changed(Some(1), 2));
        assert!(!changed(Some(2), 2));
    }

    #[test]
    fn a_write_from_another_connection_changes_data_version() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("kairos.db");
        let watched = Db::open(&path).unwrap();
        let before = watched.data_version().unwrap();

        let other = Kairos::new(Db::open(&path).unwrap(), FakeClock::new(1_792_746_000, 0));
        other
            .start(StartArgs {
                parent_monday_id: Some("12406850311".into()),
                ..StartArgs::default()
            })
            .unwrap();

        assert!(changed(Some(before), watched.data_version().unwrap()));
    }

    #[test]
    fn a_snapshot_does_not_change_data_version() {
        // Sinon chaque `kairos://changed` ferait relire l'interface, qui relancerait l'événement.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("kairos.db");
        let watched = Db::open(&path).unwrap();
        let reader = Kairos::new(Db::open(&path).unwrap(), FakeClock::new(1_792_746_000, 0));
        reader
            .start(StartArgs {
                parent_monday_id: Some("12406850311".into()),
                ..StartArgs::default()
            })
            .unwrap();
        let before = watched.data_version().unwrap();

        bridge::snapshot(&reader, "/Users/lucas").unwrap();

        assert!(!changed(Some(before), watched.data_version().unwrap()));
    }
}
