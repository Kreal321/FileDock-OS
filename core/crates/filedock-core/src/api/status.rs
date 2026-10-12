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

    let s3_health = async {
        let channel = crate::runtime::channel(crate::runtime::s3_socket(app)).await?;
        fd_api::extension::v1::subsystem_client::SubsystemClient::new(channel)
            .health(fd_api::extension::v1::Empty {})
            .await
    }
    .await;
    let s3_ok = s3_health.as_ref().is_ok_and(|r| {
        r.get_ref().status == fd_api::extension::v1::health_report::Status::Healthy as i32
    });
    components.push(ComponentHealth {
        id: "fd-s3".into(),
        name: "Native S3 connector".into(),
        state: if s3_ok {
            State::Healthy.into()
        } else {
            State::Failed.into()
        },
        detail: if s3_ok {
            "serving".into()
        } else {
            "connector is not responding".into()
        },
    });
    let rclone_ok = fd_connectors::rclone::adapter::RcloneAdapter::new()
        .health()
        .await
        .is_ok();
    components.push(ComponentHealth {
        id: "rclone".into(),
        name: "rclone".into(),
        state: if rclone_ok {
            State::Healthy.into()
        } else {
            State::Failed.into()
        },
        detail: if rclone_ok {
            "remote-control service responding".into()
        } else {
            "remote-control service unavailable".into()
        },
    });

    Ok(Response::new(SystemStatus {
        interface_version: app.config.interface_version.into(),
        release_version: app.config.release_version.into(),
        appliance_id: app.appliance_id.clone(),
        appliance_name,
        components,
        last_state_snapshot_at: None,
        state_protection_warning: false,
        notification_channel_missing: true,
    }))
}
