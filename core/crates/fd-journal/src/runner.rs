use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use fd_state::db::{self, Db, DbError};
use fd_state::{ResourceKind, ResourceRef, new_id};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum OperationState {
    Pending,
    Running,
    Succeeded,
    Failed,
    RollingBack,
    RolledBack,
    AwaitingConfirmation,
}

impl OperationState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "Pending",
            Self::Running => "Running",
            Self::Succeeded => "Succeeded",
            Self::Failed => "Failed",
            Self::RollingBack => "RollingBack",
            Self::RolledBack => "RolledBack",
            Self::AwaitingConfirmation => "AwaitingConfirmation",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "Pending" => Self::Pending,
            "Running" => Self::Running,
            "Succeeded" => Self::Succeeded,
            "Failed" => Self::Failed,
            "RollingBack" => Self::RollingBack,
            "RolledBack" => Self::RolledBack,
            "AwaitingConfirmation" => Self::AwaitingConfirmation,
            _ => return None,
        })
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::RolledBack)
    }

    pub fn is_unfinished(self) -> bool {
        !self.is_terminal()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationStep {
    pub name: String,
    pub state: String,
    pub intent: Option<Value>,
    pub result: Option<Value>,
    pub attempts: u32,
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Operation {
    pub id: String,
    pub kind: String,
    pub state: OperationState,
    pub steps: Vec<OperationStep>,
    pub resource: Option<ResourceRef>,
    pub started_by: String,
    pub params: Value,
    pub error: Option<String>,
    pub started_at: String,
    pub finished_at: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum JournalError {
    #[error("operation {0} was not found")]
    NotFound(String),
    #[error("step {0} failed: {1}")]
    StepFailed(String, String),
    #[error(transparent)]
    Db(#[from] DbError),
}

impl From<rusqlite::Error> for JournalError {
    fn from(error: rusqlite::Error) -> Self {
        JournalError::Db(error.into())
    }
}

/// What a step did. The runner records `intent` before this is called.
pub enum StepOutcome {
    Done { result: Value, detail: String },
    Retry { detail: String },
    Unrecoverable { error: String },
    Fatal { error: String },
}

/// One operation kind. Steps are named and run in order. Each step must be safe to retry.
pub trait StepRunner: Send + Sync {
    fn kind(&self) -> &'static str;
    fn steps(&self) -> &'static [&'static str];
    fn run_step(
        &self,
        operation: &Operation,
        step: &str,
        intent: &Value,
    ) -> Result<StepOutcome, JournalError>;
    fn rollback_step(&self, operation: &Operation, step: &str) -> Result<(), JournalError> {
        let _ = (operation, step);
        Ok(())
    }
}

pub struct Journal {
    db: Db,
}

impl Journal {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub fn start(
        &self,
        kind: &str,
        params: Value,
        resource: Option<ResourceRef>,
        started_by: &str,
        step_names: &[&str],
    ) -> Result<Operation, JournalError> {
        let id = new_id();
        let now = db::now_text();
        self.db.with(|conn| {
            let tx=conn.transaction()?;
            tx.execute(
                "INSERT INTO operation
                    (id, kind, state, resource_kind, resource_id, started_by, params, started_at, updated_at)
                 VALUES (?1, ?2, 'Pending', ?3, ?4, ?5, ?6, ?7, ?7)",
                params![
                    id,
                    kind,
                    resource.as_ref().map(|r| r.kind.as_str()),
                    resource.as_ref().map(|r| r.id.as_str()),
                    started_by,
                    params.to_string(),
                    now
                ],
            )?;
            for name in step_names {
                tx.execute(
                    "INSERT INTO operation_step (operation_id, name, state, attempts) VALUES (?1, ?2, 'pending', 0)",
                    params![id, name],
                )?;
            }
            let operation=load(&tx,&id)?;
            tx.commit()?;
            Ok(operation)
        })
    }

    pub fn get(&self, id: &str) -> Result<Operation, JournalError> {
        self.db.with(|conn| load(conn, id))
    }

    pub fn list(
        &self,
        active_only: bool,
        after_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<Operation>, JournalError> {
        self.db.with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id FROM operation
                 WHERE (?1 = 0 OR state IN ('Pending','Running','RollingBack','AwaitingConfirmation'))
                   AND (?2 IS NULL OR id < ?2)
                 ORDER BY started_at DESC, id DESC
                 LIMIT ?3",
            )?;
            let ids = stmt
                .query_map(params![active_only as i64, after_id, limit as i64], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            ids.into_iter().map(|id| load(conn, &id)).collect()
        })
    }

    pub fn unfinished(&self) -> Result<Vec<Operation>, JournalError> {
        let mut result = Vec::new();
        let mut after = None;
        loop {
            let page = self.list(true, after.as_deref(), 256)?;
            if page.is_empty() {
                break;
            }
            after = page.last().map(|operation| operation.id.clone());
            result.extend(page);
        }
        Ok(result)
    }

    /// Records intent, then runs the step. Safe to call again after a crash.
    pub fn run_next(&self, runner: &dyn StepRunner, id: &str) -> Result<Operation, JournalError> {
        let operation = self.get(id)?;
        if operation.kind != runner.kind() {
            return Err(JournalError::StepFailed(
                id.into(),
                "runner kind mismatch".into(),
            ));
        }
        let Some(step) = operation
            .steps
            .iter()
            .find(|s| s.state != "done" && s.state != "rolled_back")
        else {
            self.set_state(id, OperationState::Succeeded, None)?;
            return self.get(id);
        };
        self.db.with(|conn| {
            set_state(conn, id, OperationState::Running, None)?;
            let intent = serde_json::json!({"step": step.name, "attempt": step.attempts + 1});
            conn.execute(
                "UPDATE operation_step
                 SET intent = COALESCE(intent, ?1), state = 'acting', attempts = attempts + 1
                 WHERE operation_id = ?2 AND name = ?3",
                params![intent.to_string(), id, step.name],
            )?;
            Ok::<_, JournalError>(())
        })?;
        let prepared = self.get(id)?;
        let intent = prepared
            .steps
            .iter()
            .find(|s| s.name == step.name)
            .and_then(|s| s.intent.as_ref())
            .unwrap();
        let outcome = runner.run_step(&prepared, &step.name, intent)?;
        match outcome {
            StepOutcome::Done { result, detail } => {
                self.db.with(|conn| {
                    conn.execute(
                        "UPDATE operation_step SET state = 'done', result = ?1, detail = ?2
                         WHERE operation_id = ?3 AND name = ?4",
                        params![result.to_string(), detail, id, step.name],
                    )?;
                    Ok::<_, JournalError>(())
                })?;
                if operation
                    .steps
                    .iter()
                    .filter(|s| s.name != step.name)
                    .all(|s| s.state == "done")
                    && operation.steps.iter().any(|s| s.name == step.name)
                {
                    // After this step, check remaining.
                    let after = self.get(id)?;
                    if after.steps.iter().all(|s| s.state == "done") {
                        self.set_state(id, OperationState::Succeeded, None)?;
                    }
                }
            }
            StepOutcome::Retry { detail } => {
                self.db.with(|conn| {
                    conn.execute(
                        "UPDATE operation_step SET state = 'pending', detail = ?1
                         WHERE operation_id = ?2 AND name = ?3",
                        params![detail, id, step.name],
                    )?;
                    Ok::<_, JournalError>(())
                })?;
            }
            StepOutcome::Unrecoverable { error } => {
                self.set_state(id, OperationState::RollingBack, Some(&error))?;
                self.rollback(runner, id)?;
            }
            StepOutcome::Fatal { error } => {
                self.set_state(id, OperationState::Failed, Some(&error))?;
            }
        }
        self.get(id)
    }

    pub fn begin_step(&self, id: &str, name: &str) -> Result<(), JournalError> {
        self.db.with(|conn| {
            set_state(conn, id, OperationState::Running, None)?;
            conn.execute("UPDATE operation_step SET state='acting', intent=?1, attempts=attempts+1 WHERE operation_id=?2 AND name=?3", params![serde_json::json!({"step":name}).to_string(),id,name])?;
            Ok(())
        })
    }
    pub fn finish_step(
        &self,
        id: &str,
        name: &str,
        result: Result<Value, String>,
    ) -> Result<Operation, JournalError> {
        self.db.with(|conn| {
            let tx = conn.transaction()?;
            match result {
                Ok(value) => {
                    tx.execute(
                        "UPDATE operation_step SET state='done', result=?1, detail='' WHERE operation_id=?2 AND name=?3",
                        params![value.to_string(), id, name],
                    )?;
                    let remaining: i64 = tx.query_row(
                        "SELECT COUNT(*) FROM operation_step WHERE operation_id=?1 AND state<>'done'",
                        [id], |row| row.get(0),
                    )?;
                    set_state(&tx, id, if remaining == 0 { OperationState::Succeeded } else { OperationState::Running }, None)?;
                }
                Err(error) => {
                    tx.execute(
                        "UPDATE operation_step SET state='failed', result=NULL, detail=?1 WHERE operation_id=?2 AND name=?3",
                        params![error, id, name],
                    )?;
                    set_state(&tx, id, OperationState::Failed, Some(&error))?;
                }
            }
            let operation = load(&tx, id)?;
            tx.commit()?;
            Ok(operation)
        })
    }

    pub fn resume_all(&self, runners: &[&dyn StepRunner]) -> Result<Vec<Operation>, JournalError> {
        self.resume_all_except(runners, &[])
    }
    /// Application-owned async state machines resume independently of synchronous runners.
    pub fn resume_all_except(
        &self,
        runners: &[&dyn StepRunner],
        deferred_kinds: &[&str],
    ) -> Result<Vec<Operation>, JournalError> {
        let unfinished = self.unfinished()?;
        let mut out = Vec::new();
        for operation in unfinished {
            if deferred_kinds.contains(&operation.kind.as_str()) {
                continue;
            }
            let Some(runner) = runners.iter().copied().find(|r| r.kind() == operation.kind) else {
                self.set_state(&operation.id, OperationState::Failed, Some("No installed runner for this operation; manual review required. Side effects were preserved."))?;
                out.push(self.get(&operation.id)?);
                continue;
            };
            match operation.state {
                OperationState::RollingBack => out.push(self.rollback(runner, &operation.id)?),
                OperationState::AwaitingConfirmation => out.push(operation),
                _ => {
                    let mut current = operation;
                    while current.state == OperationState::Pending
                        || current.state == OperationState::Running
                    {
                        let next = self.run_next(runner, &current.id)?;
                        if next.id == current.id
                            && next.state == current.state
                            && next.steps.iter().map(|s| s.attempts).sum::<u32>()
                                == current.steps.iter().map(|s| s.attempts).sum::<u32>()
                        {
                            break;
                        }
                        let retry_pending = next
                            .steps
                            .iter()
                            .any(|s| s.state == "pending" && s.attempts > 0);
                        current = next;
                        if retry_pending {
                            break;
                        }
                    }
                    out.push(current);
                }
            }
        }
        Ok(out)
    }

    pub fn rollback(&self, runner: &dyn StepRunner, id: &str) -> Result<Operation, JournalError> {
        self.set_state(id, OperationState::RollingBack, None)?;
        let operation = self.get(id)?;
        for step in operation
            .steps
            .iter()
            .rev()
            .filter(|s| s.state == "done" || s.state == "acting")
        {
            runner.rollback_step(&operation, &step.name)?;
            self.db.with(|conn| {
                conn.execute(
                    "UPDATE operation_step SET state = 'rolled_back' WHERE operation_id = ?1 AND name = ?2",
                    params![id, step.name],
                )?;
                Ok::<_, JournalError>(())
            })?;
        }
        self.set_state(id, OperationState::RolledBack, operation.error.as_deref())?;
        self.get(id)
    }

    pub fn set_state(
        &self,
        id: &str,
        state: OperationState,
        error: Option<&str>,
    ) -> Result<(), JournalError> {
        self.db.with(|conn| set_state(conn, id, state, error))
    }
}

fn set_state(
    conn: &Connection,
    id: &str,
    state: OperationState,
    error: Option<&str>,
) -> Result<(), JournalError> {
    let now = db::now_text();
    let finished = if state.is_terminal() {
        Some(now.clone())
    } else {
        None
    };
    let n = conn.execute(
        "UPDATE operation SET state = ?1, error = COALESCE(?2, error), updated_at = ?3, finished_at = COALESCE(?4, finished_at)
         WHERE id = ?5",
        params![state.as_str(), error, now, finished, id],
    )?;
    if n == 0 {
        Err(JournalError::NotFound(id.into()))
    } else {
        Ok(())
    }
}

fn load(conn: &Connection, id: &str) -> Result<Operation, JournalError> {
    let found = conn
        .query_row(
            "SELECT id, kind, state, resource_kind, resource_id, started_by, params, error, started_at, finished_at
             FROM operation WHERE id = ?1",
            [id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, Option<String>>(9)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| JournalError::NotFound(id.into()))?;
    let (
        id,
        kind,
        state,
        resource_kind,
        resource_id,
        started_by,
        params,
        error,
        started_at,
        finished_at,
    ) = found;
    let mut stmt = conn.prepare(
        "SELECT name, state, intent, result, attempts, detail FROM operation_step WHERE operation_id = ?1 ORDER BY rowid",
    )?;
    let steps = stmt
        .query_map([&id], |row| {
            Ok(OperationStep {
                name: row.get(0)?,
                state: row.get(1)?,
                intent: row
                    .get::<_, Option<String>>(2)?
                    .map(|t| serde_json::from_str(&t).unwrap_or(Value::Null)),
                result: row
                    .get::<_, Option<String>>(3)?
                    .map(|t| serde_json::from_str(&t).unwrap_or(Value::Null)),
                attempts: row.get::<_, i64>(4)? as u32,
                detail: row.get::<_, Option<String>>(5)?.unwrap_or_default(),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Operation {
        id,
        kind,
        state: OperationState::parse(&state).unwrap_or(OperationState::Failed),
        steps,
        resource: match (resource_kind, resource_id) {
            (Some(kind), Some(id)) => {
                ResourceKind::parse(&kind).map(|kind| ResourceRef { kind, id })
            }
            _ => None,
        },
        started_by,
        params: serde_json::from_str(&params).unwrap_or(Value::Null),
        error,
        started_at,
        finished_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MIGRATION_0002_JOURNAL;
    use fd_state::db::{Db, MIGRATION_0001_INIT};

    struct Echo;

    impl StepRunner for Echo {
        fn kind(&self) -> &'static str {
            "Echo"
        }
        fn steps(&self) -> &'static [&'static str] {
            &["one", "two"]
        }
        fn run_step(
            &self,
            _operation: &Operation,
            step: &str,
            _intent: &Value,
        ) -> Result<StepOutcome, JournalError> {
            Ok(StepOutcome::Done {
                result: serde_json::json!({"step": step}),
                detail: step.into(),
            })
        }
    }

    fn journal() -> Journal {
        Journal::new(Db::open_in_memory(&[MIGRATION_0001_INIT, MIGRATION_0002_JOURNAL]).unwrap())
    }

    #[test]
    fn runs_steps_and_records_intent() {
        let journal = journal();
        let op = journal
            .start("Echo", serde_json::json!({}), None, "system", Echo.steps())
            .unwrap();
        let after = journal.run_next(&Echo, &op.id).unwrap();
        assert_eq!(after.steps[0].state, "done");
        assert!(after.steps[0].intent.is_some());
        let done = journal.run_next(&Echo, &op.id).unwrap();
        assert_eq!(done.state, OperationState::Succeeded);
    }

    #[test]
    fn resume_finishes_unfinished() {
        let journal = journal();
        let op = journal
            .start("Echo", serde_json::json!({}), None, "system", Echo.steps())
            .unwrap();
        journal.run_next(&Echo, &op.id).unwrap();
        journal.resume_all(&[&Echo]).unwrap();
        assert_eq!(
            journal.get(&op.id).unwrap().state,
            OperationState::Succeeded
        );
    }
}
