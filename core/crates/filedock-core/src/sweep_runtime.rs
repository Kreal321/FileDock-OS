//! Startup/daily cleanup. Every owned prefix is rechecked through a pinned Writer session.
use crate::{api::App, state_runtime};
use fd_connectors::ownership::{ObjectIo, Role, WriterSession};
use fd_state::{
    ResourceKind, ResourceRef,
    resources::{ResourceSpec, RouteTarget, RouteUse},
};
use tonic::Status;
pub async fn run(app: &App) -> Result<(), Status> {
    let locations = state_runtime::locations(app)?;
    let rows = app
        .store
        .list(ResourceKind::CloudConnection, None, usize::MAX / 2)
        .map_err(internal)?;
    let Some(row) = app
        .store
        .get(&ResourceRef::singleton(ResourceKind::ConnectorRoutes))
        .map_err(internal)?
    else {
        return Ok(());
    };
    let Ok(ResourceSpec::ConnectorRoutes(routes)) = row.parsed_spec() else {
        return Ok(());
    };
    for row in rows {
        let Ok(ResourceSpec::CloudConnection(connection)) = row.parsed_spec() else {
            continue;
        };
        if !matches!(fd_connectors::routes::resolve(&routes,Some(&connection),&connection.provider_family,RouteUse::OwnedObjectStorage),Ok(RouteTarget::Connector{subsystem_id}) if subsystem_id=="fd-s3")
        {
            continue;
        }
        let own = locations
            .iter()
            .filter(|l| l.connection_id == row.resource.id)
            .collect::<Vec<_>>();
        if own.iter().any(|l| l.role != Role::Writer) {
            continue;
        }
        for location in &own {
            let result = async {
                let (io, _, _) = state_runtime::object_io(
                    app,
                    &location.connection_id,
                    &location.bucket,
                    &location.prefix,
                    false,
                )
                .await?;
                let session = WriterSession {
                    io,
                    location: (*location).clone(),
                };
                fd_connectors::sweep::owned(&session)
                    .await
                    .map_err(internal)
            }
            .await;
            if let Err(e) = result {
                tracing::warn!(connection=%row.resource.id,code=%e.message(),"incomplete upload sweep refused");
            }
        }
        // For unregistered connections, only this appliance's narrow probe namespace is allowed.
        if own.is_empty() {
            let discovery = crate::session_runtime::native(app, &row, "fd-s3").await?;
            let owner = discovery
                .read(fd_connectors::ownership::OWNER_KEY)
                .await
                .map_err(internal)?;
            if let Some(owner) = owner {
                let value: fd_connectors::ownership::Owner =
                    serde_json::from_slice(&owner.bytes).map_err(internal)?;
                if value.appliance_id != state_runtime::appliance_id(app) {
                    continue;
                }
            }
            if let Some(prefix) =
                fd_connectors::sweep::probe_prefix(&state_runtime::appliance_id(app))
                && let Err(e) = discovery.abort_incomplete(&prefix, 24, false).await
            {
                tracing::warn!(connection=%row.resource.id,code=%e,"probe upload sweep failed");
            }
        }
    }
    Ok(())
}
fn internal(e: impl std::fmt::Display) -> Status {
    Status::failed_precondition(e.to_string())
}
