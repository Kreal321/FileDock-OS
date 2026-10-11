use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

use fd_secrets::redact::redact_text;
use fd_state::db::{self, Db, DbError};
use fd_state::{ResourceKind, ResourceRef, new_id};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Severity {
    Info,
    Warning,
    Critical,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Info => "Info",
            Self::Warning => "Warning",
            Self::Critical => "Critical",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "Info" => Self::Info,
            "Warning" => Self::Warning,
            "Critical" => Self::Critical,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertState {
    Active,
    Acknowledged,
    Resolved,
}

impl AlertState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "Active",
            Self::Acknowledged => "Acknowledged",
            Self::Resolved => "Resolved",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "Active" => Self::Active,
            "Acknowledged" => Self::Acknowledged,
            "Resolved" => Self::Resolved,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Alert {
    pub id: String,
    pub dedup_key: String,
    pub severity: Severity,
    pub kind: String,
    pub source: String,
    pub object: Option<ResourceRef>,
    pub message: String,
    pub first_seen_at: String,
    pub last_seen_at: String,
    pub state: AlertState,
    pub acknowledged_by: Option<String>,
}

pub struct AlertStore {
    db: Db,
}

impl AlertStore {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    /// Raise or refresh by `(source, object, kind)`.
    pub fn raise(
        &self,
        source: &str,
        object: Option<ResourceRef>,
        kind: &str,
        severity: Severity,
        message: &str,
    ) -> Result<Alert, DbError> {
        let dedup = dedup_key(source, object.as_ref(), kind);
        let message = redact_text(message);
        let now = db::now_text();
        self.db.with(|conn| {
            if let Some(id) = conn
                .query_row("SELECT id, state FROM alert WHERE dedup_key = ?1", [&dedup], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .optional()?
            {
                let (id, state) = id;
                if state == "Resolved" {
                    conn.execute(
                        "UPDATE alert SET state = 'Active', severity = ?1, message = ?2,
                            first_seen_at = ?3, last_seen_at = ?3, acknowledged_by = NULL
                         WHERE id = ?4",
                        params![severity.as_str(), message, now, id],
                    )?;
                } else {
                    conn.execute(
                        "UPDATE alert SET last_seen_at = ?1, severity = ?2, message = ?3 WHERE id = ?4",
                        params![now, severity.as_str(), message, id],
                    )?;
                }
                return load(conn, &id);
            }
            let id = new_id();
            conn.execute(
                "INSERT INTO alert
                    (id, dedup_key, severity, kind, source, object_kind, object_id, message,
                     first_seen_at, last_seen_at, state)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9, 'Active')",
                params![
                    id,
                    dedup,
                    severity.as_str(),
                    kind,
                    source,
                    object.as_ref().map(|o| o.kind.as_str()),
                    object.as_ref().map(|o| o.id.as_str()),
                    message,
                    now
                ],
            )?;
            load(conn, &id)
        })
    }

    pub fn resolve(
        &self,
        source: &str,
        object: Option<&ResourceRef>,
        kind: &str,
    ) -> Result<Option<Alert>, DbError> {
        let dedup = dedup_key(source, object, kind);
        self.db.with(|conn| {
            let Some(id) = conn
                .query_row(
                    "SELECT id FROM alert WHERE dedup_key = ?1 AND state != 'Resolved'",
                    [&dedup],
                    |r| r.get::<_, String>(0),
                )
                .optional()?
            else {
                return Ok(None);
            };
            conn.execute("UPDATE alert SET state = 'Resolved' WHERE id = ?1", [&id])?;
            Ok(Some(load(conn, &id)?))
        })
    }

    pub fn acknowledge(&self, id: &str, actor: &str) -> Result<Alert, DbError> {
        self.db.with(|conn| {
            let n = conn.execute(
                "UPDATE alert SET state = 'Acknowledged', acknowledged_by = ?1 WHERE id = ?2 AND state = 'Active'",
                params![actor, id],
            )?;
            if n == 0 {
                return load(conn, id);
            }
            load(conn, id)
        })
    }

    pub fn get(&self, id: &str) -> Result<Alert, DbError> {
        self.db.with(|conn| load(conn, id))
    }

    pub fn list(
        &self,
        states: &[AlertState],
        after_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<Alert>, DbError> {
        self.db.with(|conn| {
            let mut sql = String::from("SELECT id FROM alert WHERE (?1 IS NULL OR id < ?1)");
            if !states.is_empty() {
                sql.push_str(" AND state IN (");
                sql.push_str(&states.iter().map(|_| "?").collect::<Vec<_>>().join(","));
                sql.push(')');
            }
            sql.push_str(" ORDER BY last_seen_at DESC, id DESC LIMIT ?");
            let mut stmt = conn.prepare(&sql)?;
            let mut values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
            values.push(Box::new(after_id.map(str::to_owned)));
            for state in states {
                values.push(Box::new(state.as_str().to_owned()));
            }
            values.push(Box::new(limit as i64));
            let params: Vec<&dyn rusqlite::types::ToSql> =
                values.iter().map(|v| v.as_ref()).collect();
            let ids = stmt
                .query_map(params.as_slice(), |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            ids.into_iter().map(|id| load(conn, &id)).collect()
        })
    }
}

fn dedup_key(source: &str, object: Option<&ResourceRef>, kind: &str) -> String {
    match object {
        Some(object) => format!("{source}|{object}|{kind}"),
        None => format!("{source}|-|{kind}"),
    }
}

fn load(conn: &rusqlite::Connection, id: &str) -> Result<Alert, DbError> {
    Ok(conn.query_row(
        "SELECT id, dedup_key, severity, kind, source, object_kind, object_id, message,
                first_seen_at, last_seen_at, state, acknowledged_by
         FROM alert WHERE id = ?1",
        [id],
        |row| {
            let kind: Option<String> = row.get(5)?;
            let oid: Option<String> = row.get(6)?;
            Ok(Alert {
                id: row.get(0)?,
                dedup_key: row.get(1)?,
                severity: Severity::parse(&row.get::<_, String>(2)?).unwrap_or(Severity::Info),
                kind: row.get(3)?,
                source: row.get(4)?,
                object: match (kind, oid) {
                    (Some(kind), Some(id)) => {
                        ResourceKind::parse(&kind).map(|kind| ResourceRef { kind, id })
                    }
                    _ => None,
                },
                message: row.get(7)?,
                first_seen_at: row.get(8)?,
                last_seen_at: row.get(9)?,
                state: AlertState::parse(&row.get::<_, String>(10)?).unwrap_or(AlertState::Active),
                acknowledged_by: row.get(11)?,
            })
        },
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MIGRATION_0004_ALERTS;
    use fd_state::db::{Db, MIGRATION_0001_INIT};

    #[test]
    fn dedup_refresh_resolve_and_recur() {
        let store = AlertStore::new(
            Db::open_in_memory(&[MIGRATION_0001_INIT, MIGRATION_0004_ALERTS]).unwrap(),
        );
        let a = store
            .raise("pool", None, "pool.failed", Severity::Critical, "disk gone")
            .unwrap();
        let b = store
            .raise(
                "pool",
                None,
                "pool.failed",
                Severity::Critical,
                "still gone",
            )
            .unwrap();
        assert_eq!(a.id, b.id);
        store.resolve("pool", None, "pool.failed").unwrap();
        let c = store
            .raise("pool", None, "pool.failed", Severity::Critical, "again")
            .unwrap();
        assert_eq!(c.id, a.id);
        assert_eq!(c.state, AlertState::Active);
        assert_eq!(store.get(&c.id).unwrap().state, AlertState::Active);
    }
}
