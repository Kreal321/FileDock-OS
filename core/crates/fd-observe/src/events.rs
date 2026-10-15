use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::broadcast;

use fd_secrets::redact::redact_value;
use fd_state::db::{self, Db, DbError};
use fd_state::{ResourceKind, ResourceRef};

const RETENTION: Duration = Duration::from_secs(7 * 24 * 3600);
const RING: usize = 4096;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Event {
    pub seq: u64,
    pub at: String,
    pub r#type: String,
    pub subject: Option<ResourceRef>,
    pub payload: Value,
}

#[derive(Clone)]
pub struct EventBus {
    db: Db,
    live: broadcast::Sender<Event>,
    ring: Arc<Mutex<VecDeque<Event>>>,
}

impl EventBus {
    pub fn new(db: Db) -> Self {
        let (live, _) = broadcast::channel(512);
        Self {
            db,
            live,
            ring: Arc::new(Mutex::new(VecDeque::with_capacity(RING))),
        }
    }

    pub fn latest_seq(&self) -> Result<u64, DbError> {
        self.db.with(|conn| {
            Ok(
                conn.query_row("SELECT COALESCE(MAX(seq), 0) FROM event", [], |r| {
                    r.get::<_, i64>(0)
                })? as u64,
            )
        })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.live.subscribe()
    }

    pub fn emit(
        &self,
        r#type: &str,
        subject: Option<ResourceRef>,
        payload: Value,
    ) -> Result<Event, DbError> {
        let payload = redact_value(payload);
        let at = db::now_text();
        let event = self.db.with(|conn| {
            conn.execute(
                "INSERT INTO event (at, type, subject_kind, subject_id, payload) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    at,
                    r#type,
                    subject.as_ref().map(|s| s.kind.as_str()),
                    subject.as_ref().map(|s| s.id.as_str()),
                    payload.to_string()
                ],
            )?;
            let seq: i64 = conn.query_row("SELECT last_insert_rowid()", [], |r| r.get(0))?;
            let cutoff = (chrono::Utc::now() - RETENTION).to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
            conn.execute("DELETE FROM event WHERE at < ?1", [cutoff])?;
            Ok::<Event, DbError>(Event {
                seq: seq as u64,
                at,
                r#type: r#type.to_owned(),
                subject,
                payload,
            })
        })?;
        {
            let mut ring = self.ring.lock().unwrap_or_else(|p| p.into_inner());
            if ring.len() == RING {
                ring.pop_front();
            }
            ring.push_back(event.clone());
        }
        // Journal only lifecycle boundaries, never progress ticks or event payloads.
        let operation_id = event
            .payload
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("");
        match event.r#type.as_str() {
            "operation.started" | "operation.completed" => {
                tracing::info!(event = %event.r#type, operation_id, "operation lifecycle");
            }
            "operation.failed" => {
                tracing::error!(event = %event.r#type, operation_id, "operation failed");
            }
            "operation.rolled_back" => {
                tracing::warn!(event = %event.r#type, operation_id, "operation rolled back");
            }
            _ => {}
        }
        let _ = self.live.send(event.clone());
        Ok(event)
    }

    pub fn after(&self, seq: u64, limit: usize) -> Result<Vec<Event>, DbError> {
        self.db.with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT seq, at, type, subject_kind, subject_id, payload
                 FROM event WHERE seq > ?1 ORDER BY seq LIMIT ?2",
            )?;
            let rows = stmt
                .query_map(params![seq as i64, limit as i64], |row| {
                    let kind: Option<String> = row.get(3)?;
                    let id: Option<String> = row.get(4)?;
                    Ok(Event {
                        seq: row.get::<_, i64>(0)? as u64,
                        at: row.get(1)?,
                        r#type: row.get(2)?,
                        subject: match (kind, id) {
                            (Some(kind), Some(id)) => {
                                ResourceKind::parse(&kind).map(|kind| ResourceRef { kind, id })
                            }
                            _ => None,
                        },
                        payload: serde_json::from_str(&row.get::<_, String>(5)?)
                            .unwrap_or(Value::Null),
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok::<Vec<Event>, DbError>(rows)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MIGRATION_0003_EVENTS;
    use fd_state::db::{Db, MIGRATION_0001_INIT};

    #[test]
    fn seq_is_monotonic_and_payload_is_redacted() {
        let db = Db::open_in_memory(&[MIGRATION_0001_INIT, MIGRATION_0003_EVENTS]).unwrap();
        let bus = EventBus::new(db);
        let first = bus
            .emit(
                "alert.raised",
                None,
                serde_json::json!({"password": "hunter2", "ok": true}),
            )
            .unwrap();
        let second = bus
            .emit("operation.progressed", None, serde_json::json!({}))
            .unwrap();
        assert!(second.seq > first.seq);
        assert_eq!(first.payload["password"], "••••");
        assert_eq!(bus.after(first.seq, 10).unwrap().len(), 1);
    }
}
