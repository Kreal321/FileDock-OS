use std::sync::Arc;

use fd_api::core::v1::{
    self as pb, CloudConnector, ConnectionRef, ConnectionTestResult, ListConnectorsResponse,
    RouteChangeImpact, RouteChangeRequest, RouteTable, SetConnectorEnabledRequest,
    connectors_server::Connectors,
};
use fd_connectors::test::ConnectionTester;
use fd_state::resources::{ConnectorRoutesSpec, ResourceSpec, RouteUse};
use fd_state::{ResourceKind, ResourceRef};
use tonic::{Request, Response, Status};

use super::App;
use super::interceptors::check_metadata;

pub struct ConnectorsSvc {
    pub app: Arc<App>,
}

#[tonic::async_trait]
impl Connectors for ConnectorsSvc {
    async fn list_connectors(
        &self,
        request: Request<pb::Empty>,
    ) -> Result<Response<ListConnectorsResponse>, Status> {
        check_metadata(request.metadata(), self.app.config.interface_version, false)?;
        let rows = self
            .app
            .subsystems
            .connectors()
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(ListConnectorsResponse {
            connectors: rows.iter().map(to_connector).collect(),
        }))
    }

    async fn set_connector_enabled(
        &self,
        request: Request<SetConnectorEnabledRequest>,
    ) -> Result<Response<CloudConnector>, Status> {
        check_metadata(request.metadata(), self.app.config.interface_version, true)?;
        let req = request.into_inner();
        let row = self
            .app
            .subsystems
            .set_enabled(&req.subsystem_id, req.enabled)
            .map_err(|e| Status::not_found(e.to_string()))?;
        Ok(Response::new(to_connector(&row)))
    }

    async fn get_routes(
        &self,
        request: Request<pb::Empty>,
    ) -> Result<Response<RouteTable>, Status> {
        check_metadata(request.metadata(), self.app.config.interface_version, false)?;
        Ok(Response::new(self.route_table()?))
    }

    async fn preview_route_change(
        &self,
        request: Request<RouteChangeRequest>,
    ) -> Result<Response<RouteChangeImpact>, Status> {
        check_metadata(request.metadata(), self.app.config.interface_version, false)?;
        let _ = request;
        Ok(Response::new(RouteChangeImpact::default()))
    }

    async fn apply_route_change(
        &self,
        request: Request<RouteChangeRequest>,
    ) -> Result<Response<RouteTable>, Status> {
        check_metadata(request.metadata(), self.app.config.interface_version, true)?;
        let _ = request;
        Ok(Response::new(self.route_table()?))
    }

    async fn test_connection(
        &self,
        request: Request<ConnectionRef>,
    ) -> Result<Response<ConnectionTestResult>, Status> {
        check_metadata(request.metadata(), self.app.config.interface_version, true)?;
        let _ = (
            request.into_inner(),
            ConnectionTester::probe_prefix("appliance", "rand"),
            RouteUse::OwnedObjectStorage,
        );
        Ok(Response::new(ConnectionTestResult::default()))
    }
}

impl ConnectorsSvc {
    fn route_table(&self) -> Result<RouteTable, Status> {
        let row = self
            .app
            .store
            .get(&ResourceRef::singleton(ResourceKind::ConnectorRoutes))
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found("ConnectorRoutes"))?;
        let spec = match row.parsed_spec() {
            Ok(ResourceSpec::ConnectorRoutes(spec)) => spec,
            _ => ConnectorRoutesSpec::seeded(),
        };
        Ok(RouteTable {
            defaults: spec
                .defaults
                .iter()
                .map(|d| pb::Route {
                    key: Some(pb::RouteKey {
                        family: d.family.clone(),
                        r#use: route_use(d.use_kind) as i32,
                        connection_id: String::new(),
                    }),
                    target: Some(route_target(&d.target)),
                })
                .collect(),
            overrides: Vec::new(),
            generation: row.generation,
        })
    }
}

fn to_connector(row: &fd_subsystems::SubsystemRecord) -> CloudConnector {
    CloudConnector {
        subsystem_id: row.id.clone(),
        class: row.class.clone(),
        enabled: row.enabled,
        families: row
            .manifest
            .get("cloud")
            .and_then(serde_json::Value::as_array)
            .map(|rows| {
                rows.iter()
                    .map(|entry| pb::ConnectorFamily {
                        family: entry
                            .get("family")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .into(),
                        capabilities: entry
                            .get("capabilities")
                            .and_then(|v| v.as_array())
                            .map(|caps| {
                                caps.iter()
                                    .filter_map(|c| c.as_str().map(str::to_owned))
                                    .collect()
                            })
                            .unwrap_or_default(),
                        conditional_writes: entry
                            .get("conditionalWrites")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false),
                        provider_checksums: entry
                            .get("providerChecksums")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false),
                    })
                    .collect()
            })
            .unwrap_or_default(),
        state: row.state.clone(),
        routed_by: Vec::new(),
    }
}

fn route_use(use_kind: RouteUse) -> pb::RouteUse {
    match use_kind {
        RouteUse::OwnedObjectStorage => pb::RouteUse::OwnedObjectStorage,
        RouteUse::BackupRepository => pb::RouteUse::BackupRepository,
        RouteUse::CloudMount => pb::RouteUse::CloudMount,
    }
}

fn route_target(target: &fd_state::resources::RouteTarget) -> pb::RouteTarget {
    use fd_state::resources::RouteTarget::*;
    pb::RouteTarget {
        target: Some(match target {
            Connector { subsystem_id } => {
                pb::route_target::Target::ConnectorId(subsystem_id.clone())
            }
            BackupEngine => pb::route_target::Target::BackupEngine(true),
        }),
    }
}
