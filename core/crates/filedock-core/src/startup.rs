//! Ordered startup and durable reconciliation. Later stories register their operation runners here.
use fd_journal::Journal;
use fd_observe::EventBus;
use fd_reconcile::Reconciler;
use fd_state::db::{self, Db};
use serde_json::json;
use std::sync::Arc;

pub async fn reconcile_operation(
    journal: &Journal,
    reconciler: &Reconciler,
    events: &EventBus,
    id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    journal.begin_step(id, "reconcile")?;
    events.emit(
        "operation.progressed",
        None,
        json!({"id":id,"step":"reconcile"}),
    )?;
    let result = reconciler
        .reconcile_now()
        .await
        .map(|_| json!({"reconciled":true}))
        .map_err(|e| e.to_string());
    let succeeded = result.is_ok();
    journal.finish_step(id, "reconcile", result)?;
    events.emit(
        if succeeded {
            "operation.completed"
        } else {
            "operation.failed"
        },
        None,
        json!({"id":id}),
    )?;
    Ok(())
}

pub async fn run(
    db: Db,
    journal: Arc<Journal>,
    reconciler: Arc<Reconciler>,
    events: Arc<EventBus>,
    runtime_enabled: bool,
    subsystems: &fd_subsystems::SubsystemRegistry,
    supervisor: &fd_subsystems::supervisor::Supervisor,
) -> Result<(), Box<dyn std::error::Error>> {
    db.with(|c| db::put_setting(c, "startup.ready", &json!(false)))?;
    events.emit("core.starting", None, json!({"phase":"resume-journal"}))?;
    // Supported asynchronous operations are resumed before the regular reconciliation pass.
    for operation in journal.unfinished()? {
        if operation.kind == "ReconcileAll" {
            reconcile_operation(&journal, &reconciler, &events, &operation.id).await?;
        }
    }
    journal.resume_all_except(
        &[],
        &[
            "MapPools",
            "AppAction",
            "CreatePool",
            "AdoptFilesystem",
            "ExtendPool",
            "RecreatePool",
            "SystemPoolResize",
            "Restore",
            "StateSnapshot",
            "InstallExtension",
            "UpgradeExtension",
            "RemoveExtension",
            "RestartExtension",
        ],
    )?;
    if runtime_enabled {
        loop {
            match reconciler.reconcile_now().await {
                Ok(()) => break,
                Err(error) => {
                    events.emit("core.degraded", None, json!({"detail":error.to_string()}))?;
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    }
    if runtime_enabled {
        loop {
            let settled = subsystems
                .list()?
                .into_iter()
                .filter(|s| s.enabled && s.class != "Extension")
                .all(|s| {
                    supervisor
                        .status(&s.id)
                        .is_ok_and(|state| matches!(state.state.as_str(), "Running" | "Failed"))
                });
            if settled {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
    }
    db.with(|c| db::put_setting(c, "startup.ready", &json!(true)))?;
    events.emit(
        "core.ready",
        None,
        json!({"phase":"desired-state-reconciled"}),
    )?;
    Ok(())
}
