//! La base SQLite : ouverture, mode WAL, migrations.

use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::Connection;

use crate::error::{Error, Result};

/// Les migrations, dans l'ordre : l'index + 1 est le `user_version` qu'elles posent.
const MIGRATIONS: &[&str] = &[include_str!("migrations/001_init.sql")];

/// Le `user_version` de cette version de Kairos.
pub const LATEST_VERSION: i64 = MIGRATIONS.len() as i64;

/// Attente maximale quand un autre processus tient la base.
const BUSY_TIMEOUT: Duration = Duration::from_millis(5_000);

pub struct Db {
    conn: Connection,
}

impl Db {
    /// Le chemin de la base : `KAIROS_DB`, sinon `~/Library/Application Support/com.aegis.kairos/kairos.db`.
    pub fn default_path() -> PathBuf {
        if let Some(path) = std::env::var_os("KAIROS_DB") {
            return PathBuf::from(path);
        }
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default();
        home.join("Library/Application Support/com.aegis.kairos/kairos.db")
    }

    /// Ouvre la base (créée au besoin, dossier compris), la passe en WAL et applique les migrations.
    ///
    /// Une base dont le `user_version` dépasse celui connu est refusée **avant** toute
    /// écriture : une version plus ancienne ne doit jamais abîmer une base plus récente.
    pub fn open(path: &Path) -> Result<Db> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let conn = Connection::open(path)?;
        conn.busy_timeout(BUSY_TIMEOUT)?;
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version > LATEST_VERSION {
            return Err(Error::DbNewer);
        }
        conn.query_row("PRAGMA journal_mode = WAL", [], |r| r.get::<_, String>(0))?;
        conn.pragma_update(None, "foreign_keys", true)?;
        let db = Db { conn };
        db.migrate(version)?;
        Ok(db)
    }

    fn migrate(&self, from: i64) -> Result<()> {
        for (index, sql) in MIGRATIONS.iter().enumerate().skip(from as usize) {
            // Chaque migration et son `user_version` forment une seule transaction.
            self.conn.execute_batch(&format!(
                "BEGIN IMMEDIATE;\n{sql}\nPRAGMA user_version = {};\nCOMMIT;",
                index + 1
            ))?;
        }
        Ok(())
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Error;

    fn temp_path() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sous-dossier").join("kairos.db");
        (dir, path)
    }

    #[test]
    fn open_creates_missing_directory() {
        let (_dir, path) = temp_path();
        Db::open(&path).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn fresh_db_has_default_project_and_base_subtasks() {
        let (_dir, path) = temp_path();
        let db = Db::open(&path).unwrap();
        let (name, default): (String, i64) = db
            .conn()
            .query_row("SELECT name, is_default FROM projects", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!((name.as_str(), default), ("Mon projet", 1));
        let mut stmt = db
            .conn()
            .prepare(
                "SELECT name FROM subtasks WHERE parent_id IS NULL AND is_base = 1 ORDER BY id",
            )
            .unwrap();
        let names: Vec<String> = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(names, ["Back", "Front", "Recettage", "Maquette"]);
        let main: String = db
            .conn()
            .query_row(
                "SELECT value FROM settings WHERE key = 'main_subtask'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(main, "Front");
    }

    fn lot(db: &Db) -> i64 {
        db.conn()
            .execute("INSERT INTO lots (project_id, name) VALUES (1, 'x')", [])
            .unwrap();
        db.conn().last_insert_rowid()
    }

    #[test]
    fn two_open_entries_are_impossible() {
        let (_dir, path) = temp_path();
        let db = Db::open(&path).unwrap();
        let lot = lot(&db);
        db.conn()
            .execute(
                "INSERT INTO entries (lot_id, start_at) VALUES (?1, 100)",
                [lot],
            )
            .unwrap();
        let second = db.conn().execute(
            "INSERT INTO entries (lot_id, start_at) VALUES (?1, 200)",
            [lot],
        );
        assert!(
            second.is_err(),
            "deux entrées ouvertes ne doivent pas coexister"
        );
    }

    #[test]
    fn entry_with_end_before_start_is_rejected() {
        let (_dir, path) = temp_path();
        let db = Db::open(&path).unwrap();
        let lot = lot(&db);
        assert!(db
            .conn()
            .execute(
                "INSERT INTO entries (lot_id, start_at, end_at) VALUES (?1, 100, 100)",
                [lot]
            )
            .is_err());
        assert!(db
            .conn()
            .execute(
                "INSERT INTO entries (lot_id, start_at, end_at) VALUES (?1, 100, 50)",
                [lot]
            )
            .is_err());
        assert!(db
            .conn()
            .execute(
                "INSERT INTO entries (lot_id, start_at, end_at) VALUES (?1, 100, 101)",
                [lot]
            )
            .is_ok());
    }

    #[test]
    fn newer_user_version_is_refused_without_writing() {
        let (_dir, path) = temp_path();
        drop(Db::open(&path).unwrap());
        {
            let c = rusqlite::Connection::open(&path).unwrap();
            c.pragma_update(None, "user_version", 99).unwrap();
        }
        let before = std::fs::read(&path).unwrap();
        let refused = Db::open(&path);
        assert!(matches!(refused, Err(Error::DbNewer)));
        assert_eq!(
            std::fs::read(&path).unwrap(),
            before,
            "la base n'a pas été touchée"
        );
    }

    #[test]
    fn reopening_does_not_redo_migrations() {
        let (_dir, path) = temp_path();
        drop(Db::open(&path).unwrap());
        let db = Db::open(&path).unwrap();
        let projects: i64 = db
            .conn()
            .query_row("SELECT COUNT(*) FROM projects", [], |r| r.get(0))
            .unwrap();
        assert_eq!(projects, 1);
        let version: i64 = db
            .conn()
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, LATEST_VERSION);
    }

    #[test]
    fn the_database_runs_in_wal_mode_with_foreign_keys() {
        let (_dir, path) = temp_path();
        let db = Db::open(&path).unwrap();
        let mode: String = db
            .conn()
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))
            .unwrap();
        let fk: i64 = db
            .conn()
            .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
            .unwrap();
        assert_eq!((mode.as_str(), fk), ("wal", 1));
    }

    #[test]
    fn default_path_follows_the_kairos_db_variable() {
        // Lecture seule de l'environnement : le test n'écrit pas la variable (il est parallèle aux autres).
        match std::env::var_os("KAIROS_DB") {
            Some(v) => assert_eq!(Db::default_path(), std::path::PathBuf::from(v)),
            None => assert!(Db::default_path().ends_with("com.aegis.kairos/kairos.db")),
        }
    }
}
