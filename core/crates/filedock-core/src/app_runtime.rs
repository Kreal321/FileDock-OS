use crate::api::App;
use fd_state::{ResourceKind, ResourceRef};
use serde_json::json;
use std::sync::Arc;
use tonic::Status;
pub async fn resume(app: Arc<App>, id: &str) -> Result<(), Status> {
    let op = app
        .journal
        .get(id)
        .map_err(|_| Status::internal("app.journal_failed"))?;
    let object = op.params["id"]
        .as_str()
        .ok_or_else(|| Status::invalid_argument("app.id_missing"))?;
    let action = op.params["action"].as_str().unwrap_or("");
    let reference = ResourceRef::new(ResourceKind::ContainerApp, object);
    let row = app
        .store
        .get(&reference)
        .map_err(|_| Status::internal("app.read_failed"))?
        .ok_or_else(|| Status::not_found("app.not_found"))?;
    app.journal
        .begin_step(id, "apply")
        .map_err(|_| Status::internal("app.journal_failed"))?;
    let stopped = action == "stop";
    app.store
        .db()
        .with(|c| fd_state::db::put_setting(c, &format!("app.{object}.stopped"), &json!(stopped)))
        .map_err(|_| Status::internal("app.state_failed"))?;
    let controller = fd_reconcile::controllers::container_app::ContainerAppController {
        store: app.store.clone(),
        events: app.events.clone(),
        root: app.config.apps_root.clone(),
        secrets_path: app.config.secrets.clone(),
        dek_path: app.config.dek.clone(),
        secrets_root: app.config.secrets_run.clone(),
        enabled: crate::storage_runtime::enabled().is_ok(),
    };
    let outcome = async {
        if action == "update" {
            let dir = app.config.apps_root.join(object);
            let status = tokio::process::Command::new("docker")
                .args(["compose", "--project-directory"])
                .arg(dir)
                .args([
                    "-p",
                    &format!("fd-{}", row.spec["name"].as_str().unwrap_or("")),
                    "pull",
                ])
                .kill_on_drop(true)
                .output()
                .await
                .map_err(|e| e.to_string())?;
            if !status.status.success() {
                return Err("app.image_pull_failed".into());
            }
        }
        use fd_reconcile::Controller;
        controller
            .reconcile(&reference)
            .await
            .map_err(|e| e.to_string())?;
        Ok(json!({"action":action}))
    }
    .await;
    let failed = outcome.as_ref().err().cloned();
    app.journal
        .finish_step(id, "apply", outcome)
        .map_err(|_| Status::internal("app.journal_failed"))?;
    let _ = app.events.emit(
        if failed.is_some() {
            "operation.failed"
        } else {
            "operation.completed"
        },
        Some(reference),
        json!({"id":id}),
    );
    if let Some(error) = failed {
        return Err(Status::failed_precondition(error));
    }
    Ok(())
}
