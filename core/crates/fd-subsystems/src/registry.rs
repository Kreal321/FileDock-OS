use rusqlite::{OptionalExtension, params};
use serde_json::Value;

use fd_state::db::{self, Db, DbError};

use crate::manifest::LoadedManifest;

#[derive(Clone, Debug)]
pub struct SubsystemRecord {
    pub id: String,
    pub class: String,
    pub manifest: Value,
    pub version: String,
    pub enabled: bool,
    pub state: String,
}

pub struct SubsystemRegistry {
    db: Db,
}

impl SubsystemRegistry {
    pub fn db(&self) -> &Db {
        &self.db
    }

    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub fn upsert_from_manifest(
        &self,
        manifest: &LoadedManifest,
    ) -> Result<SubsystemRecord, DbError> {
        let now = db::now_text();
        self.db.with(|conn| {
            conn.execute(
                "INSERT INTO subsystem
                    (id, class, manifest, version, approved_permissions, signature, state, enabled, data_bindings, updated_at)
                 VALUES (?1, ?2, ?3, ?4, '[]', 'Unsigned', 'Running', 1, '{}', ?5)
                 ON CONFLICT (id) DO UPDATE SET
                    manifest = excluded.manifest,
                    previous_version = CASE WHEN subsystem.version = excluded.version THEN subsystem.previous_version ELSE subsystem.version END,
                    version = excluded.version,
                    class = excluded.class,
                    updated_at = excluded.updated_at",
                params![
                    manifest.id,
                    class_label(&manifest.class),
                    manifest.raw.to_string(),
                    manifest.version,
                    now
                ],
            )?;
            load(conn, &manifest.id)?.ok_or_else(|| DbError::Sqlite(rusqlite::Error::QueryReturnedNoRows))
        })
    }

    pub fn get(&self, id: &str) -> Result<Option<SubsystemRecord>, DbError> {
        self.db.with(|conn| load(conn, id))
    }

    pub fn list(&self) -> Result<Vec<SubsystemRecord>, DbError> {
        self.db.with(|conn| {
            let mut stmt = conn.prepare("SELECT id FROM subsystem ORDER BY id")?;
            let ids = stmt
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            ids.into_iter()
                .filter_map(|id| load(conn, &id).transpose())
                .collect()
        })
    }

    pub fn record_capabilities(&self, id: &str, capabilities: &[String]) -> Result<(), DbError> {
        self.db.with(|c| {
            db::put_setting(
                c,
                &format!("subsystem.registered_capabilities.{id}"),
                &serde_json::json!(capabilities),
            )
        })
    }
    pub fn effective_capabilities(&self, row: &SubsystemRecord) -> Result<Vec<String>, DbError> {
        if row.class == "Extension" && row.state == "Running" {
            return self.db.with(|c| {
                Ok(
                    db::get_setting(c, &format!("subsystem.registered_capabilities.{}", row.id))?
                        .and_then(|v| serde_json::from_value(v).ok())
                        .unwrap_or_default(),
                )
            });
        }
        Ok(row.manifest["capabilities"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect())
    }

    pub fn set_enabled(&self, id: &str, enabled: bool) -> Result<SubsystemRecord, DbError> {
        self.db.with(|conn| {
            conn.execute(
                "UPDATE subsystem SET enabled = ?1, updated_at = ?2 WHERE id = ?3",
                params![enabled as i64, db::now_text(), id],
            )?;
            load(conn, id)?.ok_or_else(|| DbError::Sqlite(rusqlite::Error::QueryReturnedNoRows))
        })
    }

    pub fn connectors(&self) -> Result<Vec<SubsystemRecord>, DbError> {
        Ok(self
            .list()?
            .into_iter()
            .filter(|row| {
                row.manifest
                    .get("cloud")
                    .and_then(Value::as_array)
                    .is_some_and(|c| !c.is_empty())
            })
            .collect())
    }
}

fn class_label(class: &str) -> &'static str {
    match class {
        "core" => "Core",
        "built-in" => "BuiltIn",
        _ => "Extension",
    }
}

fn load(conn: &rusqlite::Connection, id: &str) -> Result<Option<SubsystemRecord>, DbError> {
    Ok(conn
        .query_row(
            "SELECT id, class, manifest, version, enabled, state FROM subsystem WHERE id = ?1",
            [id],
            |row| {
                Ok(SubsystemRecord {
                    id: row.get(0)?,
                    class: row.get(1)?,
                    manifest: serde_json::from_str(&row.get::<_, String>(2)?)
                        .unwrap_or(Value::Null),
                    version: row.get(3)?,
                    enabled: row.get::<_, i64>(4)? != 0,
                    state: row.get(5)?,
                })
            },
        )
        .optional()?)
}
