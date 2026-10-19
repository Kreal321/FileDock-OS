use std::sync::Arc;

use fd_api::core::v1::{
    self as pb, CloudConnector, ConnectionRef, ConnectionTestResult, ListConnectorsResponse,
    RouteChangeImpact, RouteChangeRequest, RouteTable, SetConnectorEnabledRequest,
    connectors_server::Connectors,
};
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
    async fn start_o_auth(
        &self,
        request: Request<pb::OAuthStartRequest>,
    ) -> Result<Response<pb::OAuthFlow>, Status> {
        check_metadata(request.metadata(), self.app.config.interface_version, true)?;
        let actor = super::interceptors::actor_of(request.metadata());
        let r = request.into_inner();
        Ok(Response::new(
            crate::oauth_runtime::start(
                &self.app,
                &actor,
                &r.provider_family,
                r.config
                    .as_ref()
                    .map(crate::proto_conv::struct_to_value)
                    .unwrap_or(serde_json::json!({})),
            )
            .await?,
        ))
    }
    async fn finish_o_auth(
        &self,
        request: Request<pb::OAuthFinishRequest>,
    ) -> Result<Response<pb::OAuthFlow>, Status> {
        check_metadata(request.metadata(), self.app.config.interface_version, true)?;
        let actor = super::interceptors::actor_of(request.metadata());
        let r = request.into_inner();
        Ok(Response::new(
            crate::oauth_runtime::finish(&self.app, &actor, &r.flow_id, &r.redirect_url).await?,
        ))
    }

    async fn list_connectors(
        &self,
        request: Request<pb::Empty>,
    ) -> Result<Response<ListConnectorsResponse>, Status> {
        check_metadata(request.metadata(), self.app.config.interface_version, false)?;
        let _routes = self.app.route_lock.lock().await;
        let rows = self
            .app
            .subsystems
            .connectors()
            .map_err(|e| Status::internal(e.to_string()))?;
        let routes = self.route_table()?;
        Ok(Response::new(ListConnectorsResponse {
            connectors: rows
                .iter()
                .map(|row| {
                    let mut connector = to_connector_with_schema(&self.app, row);
                    connector.routed_by = routes
                        .defaults
                        .iter()
                        .chain(&routes.overrides)
                        .filter(|route| targets_connector(route, &row.id))
                        .filter_map(|route| route.key.clone())
                        .collect();
                    connector
                })
                .collect(),
        }))
    }

    async fn set_connector_enabled(
        &self,
        request: Request<SetConnectorEnabledRequest>,
    ) -> Result<Response<CloudConnector>, Status> {
        check_metadata(request.metadata(), self.app.config.interface_version, true)?;
        let req = request.into_inner();
        let _routes = self.app.route_lock.lock().await;
        if !req.enabled {
            let table = self.route_table()?;
            let used = table
                .defaults
                .iter()
                .chain(&table.overrides)
                .any(|route| targets_connector(route, &req.subsystem_id));
            if used {
                return Err(Status::already_exists("connector.in_use"));
            }
        }
        let row = self
            .app
            .subsystems
            .set_enabled(&req.subsystem_id, req.enabled)
            .map_err(|e| Status::not_found(e.to_string()))?;
        self.app
            .store
            .db()
            .with(|c| {
                fd_state::db::put_setting(
                    c,
                    "config.changed_at",
                    &serde_json::json!(fd_state::db::now_text()),
                )
            })
            .map_err(internal)?;
        Ok(Response::new(to_connector(&row)))
    }

    async fn get_routes(
        &self,
        request: Request<pb::Empty>,
    ) -> Result<Response<RouteTable>, Status> {
        check_metadata(request.metadata(), self.app.config.interface_version, false)?;
        let _routes = self.app.route_lock.lock().await;
        Ok(Response::new(self.route_table()?))
    }

    async fn preview_route_change(
        &self,
        request: Request<RouteChangeRequest>,
    ) -> Result<Response<RouteChangeImpact>, Status> {
        check_metadata(request.metadata(), self.app.config.interface_version, false)?;
        let _routes = self.app.route_lock.lock().await;
        let (_, preview) = self.preview(&request.into_inner())?;
        Ok(Response::new(impact_proto(&preview.impact)))
    }

    async fn apply_route_change(
        &self,
        request: Request<RouteChangeRequest>,
    ) -> Result<Response<RouteTable>, Status> {
        check_metadata(request.metadata(), self.app.config.interface_version, true)?;
        let actor = super::interceptors::actor_of(request.metadata());
        let request_id = super::interceptors::request_id_of(request.metadata());
        let audit_id = request
            .metadata()
            .get("x-fd-audit-id")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned)
            .unwrap_or_else(fd_state::new_id);
        let r = request.into_inner();
        let _routes = self.app.route_lock.lock().await;
        let (route_row, mut preview) = self.preview(&r)?;
        if let Some(issue) = preview.impact.issues.first() {
            return Err(Status::failed_precondition(&issue.code));
        }
        if preview.impact.basic_ack && !r.acknowledged_basic_claim_safety {
            return Err(Status::failed_precondition(
                "route.ack_required:acknowledgedBasicClaimSafety",
            ));
        }
        if preview.impact.third_party_ack && !r.acknowledged_third_party_owned_storage {
            return Err(Status::failed_precondition(
                "route.ack_required:acknowledgedThirdPartyOwnedStorage",
            ));
        }
        use fd_state::resources::{Acknowledgement, AcknowledgementKind};
        for (required, kind) in [
            (
                preview.impact.basic_ack,
                AcknowledgementKind::BasicClaimSafety,
            ),
            (
                preview.impact.third_party_ack,
                AcknowledgementKind::ThirdPartyOwnedStorage,
            ),
        ] {
            if required {
                preview.routes.acknowledgements.push(Acknowledgement {
                    kind,
                    scope: format!("routes:{}", route_row.generation + 1),
                    audit_entry_id: audit_id.clone(),
                });
            }
        }
        let mut changes = vec![fd_state::Change::Upsert {
            resource: route_row.resource,
            expected_generation: route_row.generation,
            spec: serde_json::to_value(&preview.routes).map_err(internal)?,
        }];
        for (id, spec) in &preview.connections {
            let row = self
                .app
                .store
                .get(&ResourceRef::new(ResourceKind::CloudConnection, id))
                .map_err(internal)?
                .ok_or_else(|| Status::aborted("route.connection_changed"))?;
            let value = serde_json::to_value(spec).map_err(internal)?;
            if value != row.spec {
                changes.push(fd_state::Change::Upsert {
                    resource: row.resource,
                    expected_generation: row.generation,
                    spec: value,
                });
            }
        }
        self.app
            .store
            .apply(
                fd_state::ChangeOrigin::Management { actor, request_id },
                &changes,
            )
            .map_err(|e| match e {
                fd_state::ApplyError::Conflict(_) => Status::aborted("route.generation_mismatch"),
                _ => Status::failed_precondition(e.to_string()),
            })?;
        crate::state_runtime::recompute_claim_safety(&self.app)?;
        let _=self.app.events.emit("connector.routes_changed",None,serde_json::json!({"generation":route_row.generation+1,"auditEntryId":audit_id,"basicAcknowledged":r.acknowledged_basic_claim_safety,"thirdPartyAcknowledged":r.acknowledged_third_party_owned_storage}));
        if crate::storage_runtime::enabled().is_ok()
            && let Err(e) = crate::runtime::configure(&self.app).await
        {
            tracing::warn!(code=%e.message(),"route configuration pending");
        }
        for id in &preview.impact.mounts {
            self.app
                .reconciler
                .enqueue_event(ResourceRef::new(ResourceKind::ManagedFolder, id))
                .await;
        }
        Ok(Response::new(self.route_table()?))
    }

    async fn test_connection(
        &self,
        request: Request<ConnectionRef>,
    ) -> Result<Response<ConnectionTestResult>, Status> {
        check_metadata(request.metadata(), self.app.config.interface_version, true)?;
        let id = request.into_inner().connection_id;
        Ok(Response::new(
            crate::runtime::test_connection(&self.app, &id).await?,
        ))
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
            overrides: self
                .app
                .store
                .list(ResourceKind::CloudConnection, None, usize::MAX / 2)
                .map_err(internal)?
                .iter()
                .flat_map(|r| {
                    let Ok(ResourceSpec::CloudConnection(c)) = r.parsed_spec() else {
                        return Vec::new();
                    };
                    c.route_overrides
                        .iter()
                        .map(|(usage, t)| pb::Route {
                            key: Some(pb::RouteKey {
                                family: c.provider_family.clone(),
                                r#use: route_use(*usage) as i32,
                                connection_id: r.resource.id.clone(),
                            }),
                            target: Some(route_target(t)),
                        })
                        .collect::<Vec<_>>()
                })
                .collect(),
            generation: row.generation,
        })
    }
}

fn targets_connector(route: &pb::Route, id: &str) -> bool {
    route
        .target
        .as_ref()
        .is_some_and(|target| match &target.target {
            Some(pb::route_target::Target::ConnectorId(connector)) => connector == id,
            Some(pb::route_target::Target::BackupEngine(true)) => id == "kopia",
            _ => false,
        })
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
                        connection_schema: None,
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

fn to_connector_with_schema(app: &App, row: &fd_subsystems::SubsystemRecord) -> CloudConnector {
    let mut connector = to_connector(row);
    for family in &mut connector.families {
        family.connection_schema = crate::connection_runtime::schema(app, row, &family.family)
            .map(|v| crate::proto_conv::value_to_struct(&v));
    }
    connector
}

fn internal(e: impl std::fmt::Display) -> Status {
    Status::internal(e.to_string())
}
impl ConnectorsSvc {
    fn preview(
        &self,
        r: &RouteChangeRequest,
    ) -> Result<
        (
            fd_state::StoredResource,
            fd_connectors::route_change::Preview,
        ),
        Status,
    > {
        let row = self
            .app
            .store
            .get(&ResourceRef::singleton(ResourceKind::ConnectorRoutes))
            .map_err(internal)?
            .ok_or_else(|| Status::not_found("route.missing"))?;
        if r.expected_generation != row.generation {
            return Err(Status::aborted("route.generation_mismatch"));
        }
        if r.changes.is_empty() {
            return Err(Status::invalid_argument("route.change_required"));
        }
        let ResourceSpec::ConnectorRoutes(spec) =
            row.parsed_spec().map_err(|e| internal(e.code))?
        else {
            return Err(Status::internal("route.invalid"));
        };
        let changes = r
            .changes
            .iter()
            .map(|c| {
                let key = c
                    .key
                    .as_ref()
                    .ok_or_else(|| Status::invalid_argument("route.key_required"))?;
                let use_kind = match pb::RouteUse::try_from(key.r#use) {
                    Ok(pb::RouteUse::OwnedObjectStorage) => RouteUse::OwnedObjectStorage,
                    Ok(pb::RouteUse::BackupRepository) => RouteUse::BackupRepository,
                    Ok(pb::RouteUse::CloudMount) => RouteUse::CloudMount,
                    _ => return Err(Status::invalid_argument("route.use_not_allowed")),
                };
                let target = c
                    .target
                    .as_ref()
                    .map(|t| match &t.target {
                        Some(pb::route_target::Target::ConnectorId(id)) => {
                            Ok(fd_state::resources::RouteTarget::Connector {
                                subsystem_id: id.clone(),
                            })
                        }
                        Some(pb::route_target::Target::BackupEngine(true)) => {
                            Ok(fd_state::resources::RouteTarget::BackupEngine)
                        }
                        _ => Err(Status::invalid_argument("route.target_invalid")),
                    })
                    .transpose()?;
                Ok(fd_connectors::route_change::Change {
                    family: key.family.clone(),
                    use_kind,
                    connection_id: key.connection_id.clone(),
                    target,
                })
            })
            .collect::<Result<Vec<_>, Status>>()?;
        let preview = fd_connectors::route_change::preview(
            &spec,
            &self
                .app
                .store
                .list(ResourceKind::CloudConnection, None, usize::MAX / 2)
                .map_err(internal)?,
            &self
                .app
                .store
                .list(ResourceKind::ManagedFolder, None, usize::MAX / 2)
                .map_err(internal)?,
            &crate::state_runtime::locations(&self.app)?,
            &self.app.subsystems.list().map_err(internal)?,
            &changes,
        );
        Ok((row, preview))
    }
}
fn impact_proto(i: &fd_connectors::route_change::Impact) -> RouteChangeImpact {
    RouteChangeImpact {
        connection_ids: i.connections.iter().cloned().collect(),
        folder_ids: i.folders.iter().cloned().collect(),
        owned_locations: i
            .locations
            .iter()
            .map(|l| pb::OwnedLocation {
                connection_id: l.connection_id.clone(),
                bucket: l.bucket.clone(),
                prefix: l.prefix.clone(),
                purpose: l.purpose.clone(),
                owner_appliance_id: l.owner_appliance_id.clone(),
                generation: l.generation,
                last_write_at: None,
                role: match l.role {
                    fd_connectors::ownership::Role::Writer => 1,
                    fd_connectors::ownership::Role::RecoveryReader => 2,
                    fd_connectors::ownership::Role::OwnershipLost => 3,
                },
                served_by_connector: l.served_by_connector.clone(),
                served_by_third_party: l.served_by_third_party,
                claim_safety: if matches!(
                    l.claim_safety,
                    fd_connectors::ownership::ClaimSafety::Conditional
                ) {
                    1
                } else {
                    2
                },
            })
            .collect(),
        lost_features: i.lost_features.iter().cloned().collect(),
        credential_recipients: i
            .recipients
            .iter()
            .map(|(k, v)| {
                (
                    k.clone(),
                    pb::ConnectionIdList {
                        connection_ids: v.iter().cloned().collect(),
                    },
                )
            })
            .collect(),
        mounts_restarting: i.mounts.iter().cloned().collect(),
        requires_basic_claim_ack: i.basic_ack,
        requires_third_party_owned_storage_ack: i.third_party_ack,
        issues: crate::proto_conv::issues_to_proto(&i.issues).issues,
    }
}
