//! GetSystemStatus: Core, container runtime, and built-in subsystem health (FR-019).

use fd_api::core::v1::{
    ComponentHealth, GetSystemStatusRequest, SystemStatus, component_health::State,
};
use fd_state::ResourceKind;
use tonic::{Request, Response, Status};

use super::App;
use super::interceptors::check_metadata;

pub async fn get_system_status(
    app: &App,
    request: Request<GetSystemStatusRequest>,
) -> Result<Response<SystemStatus>, Status> {
    check_metadata(request.metadata(), app.config.interface_version, false)?;
    let appliance_name = app
        .store
        .get(&fd_state::ResourceRef::singleton(ResourceKind::Settings))
        .ok()
        .flatten()
        .and_then(|row| {
            row.spec
                .get("applianceName")
                .and_then(|v| v.as_str())
                .map(str::to_owned)
        })
        .unwrap_or_default();

    let mut components = vec![ComponentHealth {
        id: "core".into(),
        name: "FileDock Core".into(),
        state: State::Healthy.into(),
        detail: "serving".into(),
    }];

    for record in app
        .subsystems
        .list()
        .map_err(|e| Status::internal(e.to_string()))?
    {
        let status = app
            .supervisor
            .status(&record.id)
            .map_err(|e| Status::internal(e.to_string()))?;
        components.push(ComponentHealth {
            id: record.id.clone(),
            name: record.manifest["name"]
                .as_str()
                .unwrap_or(&record.id)
                .into(),
            state: match status.state.as_str() {
                "Running" => State::Healthy,
                "Failed" => State::Failed,
                "Stopped" => State::Stopped,
                _ => State::Degraded,
            } as i32,
            detail: if status.detail.is_empty() {
                "awaiting health inspection".into()
            } else {
                status.detail
            },
        });
    }
    let ready = app
        .store
        .db()
        .with(|c| fd_state::db::get_setting(c, "startup.ready"))
        .map_err(|e| Status::internal(e.to_string()))?
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if !ready {
        components[0].state = State::Degraded as i32;
        components[0].detail = "startup reconciliation pending".into();
    }

    Ok(Response::new(SystemStatus {
        interface_version: app.config.interface_version.into(),
        release_version: app.config.release_version.into(),
        appliance_id: crate::state_runtime::appliance_id(app),
        appliance_name,
        components,
        last_state_snapshot_at: fd_appstate::engine::Engine::records(app.store.db())
            .ok()
            .and_then(|r| r.into_iter().filter_map(|r| r.last_snapshot_at).max())
            .as_deref()
            .and_then(super::events::timestamp),
        state_protection_warning: fd_appstate::engine::Engine::protection(app.store.db())
            .ok()
            .and_then(|v| v["warning"].as_bool())
            .unwrap_or(true),
        notification_channel_missing: crate::notification_runtime::channels(app)
            .map(|c| c.is_empty())
            .unwrap_or(true),
    }))
}
