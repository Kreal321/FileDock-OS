//! Persisted supervision; data-plane units are owned by systemd, never Core children.
use crate::{AdapterError, SubsystemAdapter, SubsystemRegistry};
use fd_observe::EventBus;
use fd_state::db::{self, Db, DbError};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::VecDeque, sync::Arc, time::Duration};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Supervision {
    pub failures: VecDeque<i64>,
    pub retry_at: i64,
    pub awaiting_until: i64,
    pub state: String,
    pub detail: String,
    pub diagnostics: Value,
}
impl Supervision {
    fn fail(&mut self, now: i64, detail: String) {
        while self.failures.front().is_some_and(|at| now - at >= 900_000) {
            self.failures.pop_front();
        }
        self.failures.push_back(now);
        self.detail = detail;
        self.state = if self.failures.len() >= 5 {
            "Failed"
        } else {
            "Backoff"
        }
        .into();
        self.retry_at =
            now + (1i64 << (self.failures.len().saturating_sub(1).min(9))).min(300) * 1000;
        self.awaiting_until = 0;
    }
}

pub struct Supervisor {
    db: Db,
    events: Arc<EventBus>,
    registry: SubsystemRegistry,
}
impl Supervisor {
    pub fn new(db: Db, events: Arc<EventBus>) -> Self {
        Self {
            registry: SubsystemRegistry::new(db.clone()),
            db,
            events,
        }
    }
    pub fn status(&self, id: &str) -> Result<Supervision, DbError> {
        self.db.with(|conn| {
            db::get_setting(conn, &format!("supervision.{id}"))?
                .map(serde_json::from_value)
                .transpose()
                .map(|s| s.unwrap_or_default())
                .map_err(DbError::from)
        })
    }
    fn save(&self, id: &str, status: &Supervision) -> Result<(), DbError> {
        let value = serde_json::to_value(status)?;
        self.db.with(|conn| {
            db::put_setting(conn, &format!("supervision.{id}"), &value)?;
            conn.execute("UPDATE subsystem SET state=?1, restart_count=?2, last_failure_at=?3, updated_at=?4 WHERE id=?5",
                rusqlite::params![status.state, status.failures.len() as i64, status.failures.back().and_then(|at|chrono::DateTime::from_timestamp_millis(*at)).map(db::timestamp_text), db::now_text(), id])?;
            Ok(())
        })
    }
    pub async fn inspect(&self, adapter: &dyn SubsystemAdapter, now: i64) -> Result<(), DbError> {
        let id = adapter.id();
        let mut current = self.status(id)?;
        let before = current.clone();
        let previous = current.state.clone();
        let enabled = self.registry.get(id)?.is_none_or(|s| s.enabled);
        if !enabled {
            if previous != "Stopped" {
                let stopped = tokio::time::timeout(Duration::from_secs(3), adapter.stop()).await;
                if matches!(stopped, Ok(Ok(()))) {
                    current.state = "Stopped".into();
                } else {
                    current.state = "Stopping".into();
                    current.detail = "could not stop disabled subsystem; retrying".into();
                }
            }
        } else {
            let health = tokio::time::timeout(Duration::from_secs(2), adapter.health()).await;
            let healthy = matches!(&health, Ok(Ok(h)) if h.healthy);
            if healthy {
                current.state = "Running".into();
                current.awaiting_until = 0;
                current.detail = match health {
                    Ok(Ok(h)) => h.detail,
                    _ => String::new(),
                };
                if previous == "Failed"
                    || current
                        .failures
                        .back()
                        .is_some_and(|at| now - at >= 900_000)
                {
                    current.failures.clear();
                }
            } else if current.state != "Failed" && now >= current.awaiting_until {
                if current.state != "Backoff" {
                    let detail = match health {
                        Ok(Ok(h)) => h.detail,
                        Ok(Err(e)) => e.to_string(),
                        Err(_) => "health probe timed out".into(),
                    };
                    current.fail(now, detail);
                    current.diagnostics = fd_secrets::redact::redact_value(
                        tokio::time::timeout(Duration::from_secs(2), adapter.diagnostics())
                            .await
                            .ok()
                            .and_then(Result::ok)
                            .unwrap_or_else(|| json!({"detail": current.detail})),
                    );
                    self.save(id, &current)?;
                    self.events.emit("subsystem.failed", None, json!({"id":id,"state":current.state,"retryAt":current.retry_at,"diagnostics":current.diagnostics}))?;
                } else if now >= current.retry_at {
                    // Record before acting so a Core crash never loses the attempt/backoff.
                    current.state = "Starting".into();
                    current.awaiting_until = now.saturating_add(
                        adapter.startup_grace().as_millis().min(i64::MAX as u128) as i64,
                    );
                    self.save(id, &current)?;
                    let start = tokio::time::timeout(Duration::from_secs(3), adapter.start()).await;
                    if !matches!(start, Ok(Ok(()))) {
                        current.fail(now, "subsystem start failed or timed out".into());
                        self.events.emit(
                            "subsystem.failed",
                            None,
                            json!({"id":id,"state":current.state,"detail":current.detail}),
                        )?;
                    }
                }
            }
        }
        if current != before {
            self.save(id, &current)?;
        }
        if current.state != previous && current.state == "Running" {
            self.events.emit(
                "subsystem.started",
                None,
                json!({"id":id,"state":"Running"}),
            )?;
            if !previous.is_empty() {
                self.events
                    .emit("subsystem.recovered", None, json!({"id":id}))?;
            }
        } else if current.state != previous && current.state == "Stopped" {
            self.events
                .emit("subsystem.stopped", None, json!({"id":id}))?;
        }
        Ok(())
    }
    pub async fn run(self: Arc<Self>, adapter: Arc<dyn SubsystemAdapter>) {
        let mut timer = tokio::time::interval(if adapter.id() == "kopia" {
            Duration::from_secs(30)
        } else {
            Duration::from_millis(500)
        });
        timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            timer.tick().await;
            if let Err(error) = self
                .inspect(adapter.as_ref(), chrono::Utc::now().timestamp_millis())
                .await
            {
                tracing::error!(id = adapter.id(), %error, "supervision failed");
            }
            let state = self
                .status(adapter.id())
                .map(|s| s.state)
                .unwrap_or_default();
            timer.reset_after(probe_interval(adapter.id(), &state));
        }
    }
}

// Healthy data-plane units do not need two D-Bus handshakes/RPC probes per second.
// Keep startup/backoff responsive and failure detection comfortably below one minute.
fn probe_interval(id: &str, state: &str) -> Duration {
    if id == "kopia" {
        Duration::from_secs(30)
    } else if matches!(state, "Running" | "Stopped" | "Failed") {
        Duration::from_secs(5)
    } else {
        Duration::from_millis(500)
    }
}

#[cfg(test)]
mod probe_interval_tests {
    use super::*;
    #[test]
    fn stable_units_yield_while_startup_keeps_fast_probes() {
        for id in ["fd-s3", "rclone", "samba", "nfs", "docker"] {
            for state in ["Running", "Stopped", "Failed"] {
                assert_eq!(probe_interval(id, state), Duration::from_secs(5));
            }
            for state in ["", "Starting", "Backoff"] {
                assert_eq!(probe_interval(id, state), Duration::from_millis(500));
            }
        }
        assert_eq!(probe_interval("kopia", "Running"), Duration::from_secs(30));
    }
}

/// A persistent or transient systemd unit whose lifecycle is independent of the Core.
pub struct UnitAdapter {
    pub id: String,
    pub unit: String,
}
#[async_trait::async_trait]
impl SubsystemAdapter for UnitAdapter {
    fn id(&self) -> &str {
        &self.id
    }
    async fn describe(&self) -> Result<crate::adapter::Description, AdapterError> {
        Ok(crate::adapter::Description {
            id: self.id.clone(),
            name: self.id.clone(),
            version: String::new(),
            capabilities: vec![],
        })
    }
    async fn state(&self) -> Result<Value, AdapterError> {
        self.diagnostics().await
    }
    async fn configure(&self, _: Value) -> Result<(), AdapterError> {
        Ok(())
    }
    async fn start(&self) -> Result<(), AdapterError> {
        let systemd = crate::Systemd::default();
        systemd.reset_failed(&self.unit).await?;
        systemd.restart(&self.unit).await
    }
    async fn stop(&self) -> Result<(), AdapterError> {
        crate::Systemd::default().stop(&self.unit).await
    }
    async fn health(&self) -> Result<crate::adapter::Health, AdapterError> {
        let healthy = crate::Systemd::default().is_active(&self.unit).await?;
        Ok(crate::adapter::Health {
            healthy,
            detail: format!(
                "{} {}",
                self.unit,
                if healthy { "active" } else { "inactive" }
            ),
        })
    }
    async fn diagnostics(&self) -> Result<Value, AdapterError> {
        let output = tokio::process::Command::new("systemctl")
            .args([
                "show",
                &self.unit,
                "--property=ActiveState,SubState,Result,ExecMainStatus,NRestarts",
            ])
            .kill_on_drop(true)
            .output()
            .await
            .map_err(|e| AdapterError::Failed(e.to_string()))?;
        Ok(json!({"unit":self.unit,"status":String::from_utf8_lossy(&output.stdout)}))
    }
}

/// Existing Compose containers stay owned by Docker and can be re-adopted after a Core crash.
pub struct ContainerAdapter {
    pub id: String,
    pub project: String,
}
impl ContainerAdapter {
    async fn containers(&self) -> Result<Vec<String>, AdapterError> {
        let output = tokio::process::Command::new("docker")
            .args([
                "ps",
                "-aq",
                "--filter",
                &format!("label=com.docker.compose.project={}", self.project),
            ])
            .kill_on_drop(true)
            .output()
            .await
            .map_err(|e| AdapterError::Failed(e.to_string()))?;
        if !output.status.success() {
            return Err(AdapterError::Failed("Docker unavailable".into()));
        }
        Ok(String::from_utf8_lossy(&output.stdout)
            .split_whitespace()
            .map(str::to_owned)
            .collect())
    }
}
#[async_trait::async_trait]
impl SubsystemAdapter for ContainerAdapter {
    fn id(&self) -> &str {
        &self.id
    }
    async fn describe(&self) -> Result<crate::adapter::Description, AdapterError> {
        Ok(crate::adapter::Description {
            id: self.id.clone(),
            name: self.id.clone(),
            version: String::new(),
            capabilities: vec![],
        })
    }
    async fn state(&self) -> Result<Value, AdapterError> {
        self.diagnostics().await
    }
    async fn configure(&self, _: Value) -> Result<(), AdapterError> {
        Ok(())
    }
    async fn start(&self) -> Result<(), AdapterError> {
        let ids = self.containers().await?;
        if ids.is_empty() {
            return Err(AdapterError::Failed("containers not provisioned".into()));
        }
        let status = tokio::process::Command::new("docker")
            .arg("start")
            .args(ids)
            .kill_on_drop(true)
            .status()
            .await
            .map_err(|e| AdapterError::Failed(e.to_string()))?;
        if status.success() {
            Ok(())
        } else {
            Err(AdapterError::Failed("container start failed".into()))
        }
    }
    async fn stop(&self) -> Result<(), AdapterError> {
        let ids = self.containers().await?;
        if ids.is_empty() {
            return Ok(());
        }
        let status = tokio::process::Command::new("docker")
            .arg("stop")
            .args(ids)
            .kill_on_drop(true)
            .status()
            .await
            .map_err(|e| AdapterError::Failed(e.to_string()))?;
        if status.success() {
            Ok(())
        } else {
            Err(AdapterError::Failed("container stop failed".into()))
        }
    }
    async fn health(&self) -> Result<crate::adapter::Health, AdapterError> {
        let ids = self.containers().await?;
        if ids.is_empty() {
            return Ok(crate::adapter::Health {
                healthy: false,
                detail: "containers not provisioned".into(),
            });
        }
        let output = tokio::process::Command::new("docker")
            .args([
                "inspect",
                "--format={{.State.Running}} {{.State.Restarting}}",
            ])
            .args(ids)
            .kill_on_drop(true)
            .output()
            .await
            .map_err(|e| AdapterError::Failed(e.to_string()))?;
        let healthy = output.status.success()
            && String::from_utf8_lossy(&output.stdout)
                .lines()
                .all(|line| line.trim() == "true false");
        Ok(crate::adapter::Health {
            healthy,
            detail: format!("Compose project {}", self.project),
        })
    }
    async fn diagnostics(&self) -> Result<Value, AdapterError> {
        Ok(json!({"project":self.project,"containers":self.containers().await?}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn backoff_threshold_window_and_cap() {
        let mut status = Supervision::default();
        for (i, delay) in [1, 2, 4, 8, 16].into_iter().enumerate() {
            status.fail(100_000 + i as i64 * 1000, "crash".into());
            assert_eq!(status.retry_at, 100_000 + i as i64 * 1000 + delay * 1000);
        }
        assert_eq!(status.state, "Failed");
        status.fail(1_100_000, "later".into());
        assert_eq!(status.failures.len(), 1);
        assert_eq!(status.state, "Backoff");
        for _ in 0..12 {
            status.fail(1_100_000, "crash".into());
        }
        assert_eq!(status.retry_at, 1_400_000);
    }
    #[tokio::test]
    async fn terminal_failure_survives_core_restart_and_manual_recovery_emits_event() {
        let db = Db::open_in_memory(&[
            fd_state::db::MIGRATION_0001_INIT,
            fd_observe::MIGRATION_0003_EVENTS,
            crate::MIGRATION_0005_SUBSYSTEMS,
        ])
        .unwrap();
        let events = Arc::new(EventBus::new(db.clone()));
        let supervisor = Supervisor::new(db.clone(), events.clone());
        let mut failed = Supervision::default();
        for _ in 0..5 {
            failed.fail(100_000, "crash".into());
        }
        supervisor.save("sample", &failed).unwrap();
        let restarted = Supervisor::new(db, events.clone());
        assert_eq!(restarted.status("sample").unwrap().state, "Failed");
        struct Healthy;
        #[async_trait::async_trait]
        impl SubsystemAdapter for Healthy {
            fn id(&self) -> &str {
                "sample"
            }
            async fn describe(&self) -> Result<crate::adapter::Description, AdapterError> {
                unreachable!()
            }
            async fn state(&self) -> Result<Value, AdapterError> {
                unreachable!()
            }
            async fn configure(&self, _: Value) -> Result<(), AdapterError> {
                unreachable!()
            }
            async fn start(&self) -> Result<(), AdapterError> {
                panic!("healthy unit must be adopted")
            }
            async fn stop(&self) -> Result<(), AdapterError> {
                unreachable!()
            }
            async fn health(&self) -> Result<crate::adapter::Health, AdapterError> {
                Ok(crate::adapter::Health {
                    healthy: true,
                    detail: "ok".into(),
                })
            }
        }
        restarted.inspect(&Healthy, 101_000).await.unwrap();
        assert_eq!(restarted.status("sample").unwrap().state, "Running");
        assert!(
            events
                .after(0, 10)
                .unwrap()
                .iter()
                .any(|e| e.r#type == "subsystem.recovered")
        );
    }
}
