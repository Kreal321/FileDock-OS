//! The `core.db` connection and migration runner.
//!
//! Every Core component that keeps tables in `core.db` contributes its schema as a migration. The
//! daemon assembles one ordered, append-only list (see `filedock-core`), and that list is applied
//! with `rusqlite_migration`, which tracks progress in SQLite's `user_version`.

use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

use rusqlite::{Connection, ErrorCode, OpenFlags};
use rusqlite_migration::{M, Migrations};

/// The schema owned by this crate: resources, settings, and the change sequence.
pub const MIGRATION_0001_INIT: &str = include_str!("../migrations/0001_init.sql");

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("the system disk is full")]
    DiskFull,
    #[error("database error: {0}")]
    Sqlite(#[source] rusqlite::Error),
    #[error("migration failed: {0}")]
    Migration(#[from] rusqlite_migration::Error),
    #[error("stored JSON is invalid: {0}")]
    Json(#[from] serde_json::Error),
    #[error("database task failed: {0}")]
    Join(String),
}

impl From<rusqlite::Error> for DbError {
    fn from(error: rusqlite::Error) -> Self {
        match error.sqlite_error_code() {
            Some(ErrorCode::DiskFull) => DbError::DiskFull,
            _ => DbError::Sqlite(error),
        }
    }
}

/// A shared handle to `core.db`.
///
/// SQLite work is short and synchronous, so the connection sits behind a mutex. Async callers use
/// [`Db::call`], which moves the work onto the blocking pool.
#[derive(Clone)]
pub struct Db {
    conn: Arc<Mutex<Connection>>,
}

impl Db {
    /// Opens (creating if needed) the database at `path` and applies `migrations` in order.
    pub fn open(path: &Path, migrations: &[&'static str]) -> Result<Self, DbError> {
        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let conn = Connection::open_with_flags(path, flags)?;
        Self::init(conn, migrations, true)
    }

    /// An in-memory database, for tests.
    pub fn open_in_memory(migrations: &[&'static str]) -> Result<Self, DbError> {
        Self::init(Connection::open_in_memory()?, migrations, false)
    }

    fn init(mut conn: Connection, migrations: &[&'static str], wal: bool) -> Result<Self, DbError> {
        if wal {
            let mode: String = conn.query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))?;
            if !mode.eq_ignore_ascii_case("wal") {
                return Err(DbError::Sqlite(rusqlite::Error::InvalidQuery));
            }
        }
        // A committed change is acknowledged to the administrator, so it must survive power loss.
        conn.pragma_update(None, "synchronous", "FULL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;

        let steps: Vec<M<'static>> = migrations.iter().map(|sql| M::up(sql)).collect();
        Migrations::new(steps).to_latest(&mut conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Runs `f` with the connection on the current thread.
    pub fn with<T>(&self, f: impl FnOnce(&mut Connection) -> T) -> T {
        f(&mut self.lock())
    }

    /// Runs `f` with the connection on tokio's blocking pool.
    pub async fn call<T, F>(&self, f: F) -> Result<T, DbError>
    where
        T: Send + 'static,
        F: FnOnce(&mut Connection) -> Result<T, DbError> + Send + 'static,
    {
        let db = self.clone();
        tokio::task::spawn_blocking(move || db.with(f))
            .await
            .map_err(|e| DbError::Join(e.to_string()))?
    }

    fn lock(&self) -> MutexGuard<'_, Connection> {
        // A panic while holding the lock leaves SQLite itself consistent (the transaction rolls
        // back on drop), so a poisoned mutex is safe to keep using.
        self.conn
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// The current UTC time in the fixed-width RFC 3339 form stored in `core.db`.
pub fn now_text() -> String {
    timestamp_text(chrono::Utc::now())
}

pub fn timestamp_text(at: chrono::DateTime<chrono::Utc>) -> String {
    at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

pub fn parse_timestamp(text: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|t| t.with_timezone(&chrono::Utc))
}

/// Reads a Core-internal setting.
pub fn get_setting(conn: &Connection, key: &str) -> Result<Option<serde_json::Value>, DbError> {
    use rusqlite::OptionalExtension;
    let raw: Option<String> = conn
        .query_row("SELECT value FROM setting WHERE key = ?1", [key], |row| {
            row.get(0)
        })
        .optional()?;
    Ok(raw.map(|text| serde_json::from_str(&text)).transpose()?)
}

/// Writes a Core-internal setting.
pub fn put_setting(conn: &Connection, key: &str, value: &serde_json::Value) -> Result<(), DbError> {
    conn.execute(
        "INSERT INTO setting (key, value, updated_at) VALUES (?1, ?2, ?3)
         ON CONFLICT (key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        rusqlite::params![key, value.to_string(), now_text()],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_file_in_wal_mode_and_migrates() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("core.db");
        let db = Db::open(&path, &[MIGRATION_0001_INIT]).unwrap();
        db.with(|conn| {
            let mode: String = conn
                .query_row("PRAGMA journal_mode", [], |r| r.get(0))
                .unwrap();
            assert_eq!(mode, "wal");
            let fk: i64 = conn
                .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
                .unwrap();
            assert_eq!(fk, 1);
        });
        drop(db);
        // Reopening applies nothing new and keeps the data.
        let db = Db::open(&path, &[MIGRATION_0001_INIT]).unwrap();
        let version: i64 = db.with(|c| {
            c.query_row("PRAGMA user_version", [], |r| r.get(0))
                .unwrap()
        });
        assert_eq!(version, 1);
    }

    #[test]
    fn settings_round_trip() {
        let db = Db::open_in_memory(&[MIGRATION_0001_INIT]).unwrap();
        db.with(|conn| {
            assert!(get_setting(conn, "appliance_id").unwrap().is_none());
            put_setting(conn, "appliance_id", &serde_json::json!("abc")).unwrap();
            put_setting(conn, "appliance_id", &serde_json::json!("def")).unwrap();
            assert_eq!(
                get_setting(conn, "appliance_id").unwrap(),
                Some(serde_json::json!("def"))
            );
        });
    }
}
