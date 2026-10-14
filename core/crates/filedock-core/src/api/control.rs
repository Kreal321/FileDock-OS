use std::pin::Pin;
use std::sync::Arc;

use fd_api::core::v1::{
    self as pb, ApplyChangeRequest, ApplyChangeResponse, GetSystemStatusRequest,
    ListOperationsRequest, ListOperationsResponse, ListResourcesRequest, ListResourcesResponse,
    OperationRef, StartOperationRequest, StreamEventsRequest, SystemStatus, ValidationReport,
    core_control_server::CoreControl,
};
use fd_journal::OperationState;
use fd_state::{ApplyError, Change, ChangeOrigin};
use tokio_stream::Stream;
use tonic::{Request, Response, Status};

use super::App;
use super::interceptors::{actor_of, check_metadata, request_id_of};
use crate::proto_conv;

pub struct CoreControlSvc {
    pub app: Arc<App>,
}

#[tonic::async_trait]
impl CoreControl for CoreControlSvc {
    async fn get_system_status(
        &self,
        request: Request<GetSystemStatusRequest>,
    ) -> Result<Response<SystemStatus>, Status> {
        super::status::get_system_status(&self.app, request).await
    }

    async fn apply_change(
        &self,
        request: Request<ApplyChangeRequest>,
    ) -> Result<Response<ApplyChangeResponse>, Status> {
        check_metadata(request.metadata(), self.app.config.interface_version, true)?;
        let actor = actor_of(request.metadata());
        let request_id = request_id_of(request.metadata());
        let changes = proto_changes(request.into_inner())?;
        if changes
            .iter()
            .any(|c| c.resource().kind == fd_state::ResourceKind::Extension)
        {
            return Err(Status::failed_precondition("extension.use_lifecycle_api"));
        }
        match self
            .app
            .store
            .apply(ChangeOrigin::Management { actor, request_id }, &changes)
        {
            Ok(outcome) => Ok(Response::new(ApplyChangeResponse {
                committed: outcome
                    .committed
                    .iter()
                    .map(proto_conv::stored_to_proto)
                    .collect(),
                warnings: Some(proto_conv::issues_to_proto(&outcome.warnings)),
            })),
            Err(ApplyError::Invalid(issues)) => {
                let report = proto_conv::issues_to_proto(&issues);
                let first = issues
                    .first()
                    .map(|i| i.message.clone())
                    .unwrap_or_else(|| "invalid".into());
                let _ = report;
                Err(Status::failed_precondition(first))
            }
            Err(ApplyError::Conflict(resource)) => Err(Status::aborted(format!(
                "generation mismatch for {resource}"
            ))),
            Err(ApplyError::Db(error)) => Err(Status::internal(error.to_string())),
        }
    }

    async fn validate_change(
        &self,
        request: Request<ApplyChangeRequest>,
    ) -> Result<Response<ValidationReport>, Status> {
        check_metadata(request.metadata(), self.app.config.interface_version, false)?;
        let changes = proto_changes(request.into_inner())?;
        match self.app.store.validate(&changes) {
            Ok(issues) => Ok(Response::new(proto_conv::issues_to_proto(&issues))),
            Err(ApplyError::Invalid(issues)) => {
                Ok(Response::new(proto_conv::issues_to_proto(&issues)))
            }
            Err(ApplyError::Conflict(resource)) => Err(Status::aborted(resource.to_string())),
            Err(ApplyError::Db(error)) => Err(Status::internal(error.to_string())),
        }
    }

    async fn get_resource(
        &self,
        request: Request<pb::ResourceRef>,
    ) -> Result<Response<pb::Resource>, Status> {
        check_metadata(request.metadata(), self.app.config.interface_version, false)?;
        let resource = proto_conv::ref_from_proto(request.get_ref())?;
        self.app
            .store
            .get(&resource)
            .map_err(|e| Status::internal(e.to_string()))?
            .map(|row| Response::new(proto_conv::stored_to_proto(&row)))
            .ok_or_else(|| Status::not_found(resource.to_string()))
    }

    async fn list_resources(
        &self,
        request: Request<ListResourcesRequest>,
    ) -> Result<Response<ListResourcesResponse>, Status> {
        check_metadata(request.metadata(), self.app.config.interface_version, false)?;
        let inner = request.into_inner();
        let kind = proto_conv::kind_from_proto(inner.kind)?;
        let after = inner
            .page
            .as_ref()
            .and_then(|p| (!p.page_token.is_empty()).then_some(p.page_token.as_str()));
        let limit = inner
            .page
            .as_ref()
            .map(|p| p.page_size as usize)
            .filter(|n| *n > 0)
            .unwrap_or(100);
        let mut rows = self
            .app
            .store
            .list(kind, after, limit + 1)
            .map_err(|e| Status::internal(e.to_string()))?;
        let next = if rows.len() > limit {
            rows.pop();
            rows.last()
                .map(|r| r.resource.id.clone())
                .unwrap_or_default()
        } else {
            String::new()
        };
        Ok(Response::new(ListResourcesResponse {
            resources: rows.iter().map(proto_conv::stored_to_proto).collect(),
            next_page_token: next,
        }))
    }

    async fn start_operation(
        &self,
        request: Request<StartOperationRequest>,
    ) -> Result<Response<pb::Operation>, Status> {
        check_metadata(request.metadata(), self.app.config.interface_version, true)?;
        let actor = actor_of(request.metadata());
        let req = request.into_inner();
        if req.kind == "RestartExtension" {
            let params = req
                .params
                .as_ref()
                .map(proto_conv::struct_to_value)
                .unwrap_or(serde_json::json!({}));
            let subsystem = params["subsystemId"]
                .as_str()
                .ok_or_else(|| Status::invalid_argument("extension.id_required"))?;
            let row = crate::extension_runtime::find(&self.app, subsystem)?
                .ok_or_else(|| Status::not_found("extension.not_found"))?;
            let op = self
                .app
                .journal
                .start(
                    "RestartExtension",
                    serde_json::json!({"spec":row.spec,"previous":row.spec}),
                    Some(row.resource),
                    &actor,
                    &["activate"],
                )
                .map_err(|_| Status::internal("extension.journal_failed"))?;
            let app = self.app.clone();
            let id = op.id.clone();
            tokio::spawn(async move {
                let _ = crate::extension_runtime::resume(&app, &id).await;
            });
            return Ok(Response::new(operation_to_proto(&op)));
        }
        if req.kind == "StateSnapshot" {
            let op = self
                .app
                .journal
                .start(
                    "StateSnapshot",
                    serde_json::json!({}),
                    None,
                    &actor,
                    &["snapshot"],
                )
                .map_err(|e| Status::internal(e.to_string()))?;
            let app = self.app.clone();
            let id = op.id.clone();
            tokio::spawn(async move {
                crate::restore_runtime::snapshot_operation(&app, &id).await;
            });
            return Ok(Response::new(operation_to_proto(&op)));
        }
        if req.kind != "ReconcileAll" {
            return Err(Status::invalid_argument("unsupported operation kind"));
        }
        let op = self
            .app
            .journal
            .start(
                &req.kind,
                req.params
                    .as_ref()
                    .map(proto_conv::struct_to_value)
                    .unwrap_or(serde_json::json!({})),
                None,
                &actor,
                &["reconcile"],
            )
            .map_err(|e| Status::internal(e.to_string()))?;
        self.app
            .events
            .emit(
                "operation.started",
                None,
                serde_json::json!({"id":op.id,"kind":op.kind}),
            )
            .map_err(|e| Status::internal(e.to_string()))?;
        let app = self.app.clone();
        let id = op.id.clone();
        tokio::spawn(async move {
            if let Err(error) =
                crate::startup::reconcile_operation(&app.journal, &app.reconciler, &app.events, &id)
                    .await
            {
                tracing::error!(%error,"operation reconciliation failed");
            }
        });
        Ok(Response::new(operation_to_proto(&op)))
    }

    async fn get_operation(
        &self,
        request: Request<OperationRef>,
    ) -> Result<Response<pb::Operation>, Status> {
        check_metadata(request.metadata(), self.app.config.interface_version, false)?;
        let op = self
            .app
            .journal
            .get(&request.get_ref().id)
            .map_err(|e| Status::not_found(e.to_string()))?;
        Ok(Response::new(operation_to_proto(&op)))
    }

    async fn list_operations(
        &self,
        request: Request<ListOperationsRequest>,
    ) -> Result<Response<ListOperationsResponse>, Status> {
        check_metadata(request.metadata(), self.app.config.interface_version, false)?;
        let inner = request.into_inner();
        let after = inner
            .page
            .as_ref()
            .and_then(|p| (!p.page_token.is_empty()).then_some(p.page_token.as_str()));
        let limit = inner
            .page
            .as_ref()
            .map(|p| p.page_size as usize)
            .filter(|n| *n > 0)
            .unwrap_or(100);
        let mut ops = self
            .app
            .journal
            .list(inner.active_only, after, limit + 1)
            .map_err(|e| Status::internal(e.to_string()))?;
        let next = if ops.len() > limit {
            ops.pop();
            ops.last().map(|o| o.id.clone()).unwrap_or_default()
        } else {
            String::new()
        };
        Ok(Response::new(ListOperationsResponse {
            operations: ops.iter().map(operation_to_proto).collect(),
            next_page_token: next,
        }))
    }

    type StreamEventsStream = Pin<Box<dyn Stream<Item = Result<pb::Event, Status>> + Send>>;

    async fn stream_events(
        &self,
        request: Request<StreamEventsRequest>,
    ) -> Result<Response<Self::StreamEventsStream>, Status> {
        check_metadata(request.metadata(), self.app.config.interface_version, false)?;
        let stream =
            super::events::stream(self.app.events.clone(), request.into_inner().after_seq)?;
        Ok(Response::new(Box::pin(stream)))
    }
}

fn proto_changes(request: ApplyChangeRequest) -> Result<Vec<Change>, Status> {
    request
        .changes
        .into_iter()
        .map(|change| match change.change {
            Some(pb::resource_change::Change::Upsert(u)) => {
                let resource = proto_conv::ref_from_proto(
                    u.r#ref
                        .as_ref()
                        .ok_or_else(|| Status::invalid_argument("upsert.ref is required"))?,
                )?;
                Ok(Change::Upsert {
                    resource,
                    expected_generation: u.expected_generation,
                    spec: u
                        .spec
                        .as_ref()
                        .map(proto_conv::struct_to_value)
                        .unwrap_or(serde_json::json!({})),
                })
            }
            Some(pb::resource_change::Change::Delete(d)) => {
                let resource = proto_conv::ref_from_proto(
                    d.r#ref
                        .as_ref()
                        .ok_or_else(|| Status::invalid_argument("delete.ref is required"))?,
                )?;
                Ok(Change::Delete {
                    resource,
                    expected_generation: d.expected_generation,
                })
            }
            None => Err(Status::invalid_argument("empty change")),
        })
        .collect()
}

pub fn operation_to_proto(op: &fd_journal::Operation) -> pb::Operation {
    pb::Operation {
        id: op.id.clone(),
        kind: op.kind.clone(),
        state: match op.state {
            OperationState::Pending => pb::operation::State::Pending,
            OperationState::Running => pb::operation::State::Running,
            OperationState::Succeeded => pb::operation::State::Succeeded,
            OperationState::Failed => pb::operation::State::Failed,
            OperationState::RollingBack => pb::operation::State::RollingBack,
            OperationState::RolledBack => pb::operation::State::RolledBack,
            OperationState::AwaitingConfirmation => pb::operation::State::AwaitingConfirmation,
        } as i32,
        steps: op
            .steps
            .iter()
            .map(|s| pb::OperationStep {
                name: s.name.clone(),
                state: s.state.clone(),
                attempts: s.attempts,
                detail: s.detail.clone(),
            })
            .collect(),
        resource: op.resource.as_ref().map(proto_conv::ref_to_proto),
        started_by: op.started_by.clone(),
        started_at: super::events::timestamp(&op.started_at),
        finished_at: op.finished_at.as_deref().and_then(super::events::timestamp),
        error: op.error.clone().unwrap_or_default(),
    }
}
