//! GetSystemStatus: Core, container runtime, and built-in subsystem health (FR-019).

use fd_api::core::v1::{
    ComponentHealth, GetSystemStatusRequest, SystemStatus, component_health::State,
};
use fd_state::ResourceKind;
use fd_subsystems::{DockerAdapter, KopiaAdapter, NfsAdapter, SambaAdapter, SubsystemAdapter};
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

    let adapters: Vec<Box<dyn SubsystemAdapter>> = vec![
        Box::new(SambaAdapter),
        Box::new(NfsAdapter),
        Box::new(DockerAdapter),
        Box::new(KopiaAdapter),
    ];
    for adapter in adapters {
        match adapter.health().await {
            Ok(health) => components.push(ComponentHealth {
                id: adapter.id().into(),
                name: adapter
                    .describe()
                    .await
                    .map(|d| d.name)
                    .unwrap_or_else(|_| adapter.id().into()),
                state: if health.healthy {
                    State::Healthy.into()
                } else {
                    State::Stopped.into()
                },
                detail: health.detail,
            }),
            Err(error) => components.push(ComponentHealth {
                id: adapter.id().into(),
                name: adapter.id().into(),
                state: State::Failed.into(),
                detail: error.to_string(),
            }),
        }
    }

    for (id, name, present) in [
        ("rclone", "rclone", command_exists("rclone")),
        ("fd-s3", "Native S3 connector", command_exists("fd-s3")),
    ] {
        components.push(ComponentHealth {
            id: id.into(),
            name: name.into(),
            state: if present {
                State::Healthy.into()
            } else {
                State::Stopped.into()
            },
            detail: if present {
                "installed".into()
            } else {
                "not installed".into()
            },
        });
    }

    Ok(Response::new(SystemStatus {
        interface_version: app.config.interface_version.into(),
        release_version: app.config.release_version.into(),
        appliance_id: String::new(),
        appliance_name,
        components,
        last_state_snapshot_at: None,
        state_protection_warning: false,
        notification_channel_missing: true,
    }))
}

fn command_exists(name: &str) -> bool {
    std::process::Command::new("sh")
        .args(["-c", &format!("command -v {name} >/dev/null")])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
