use std::pin::Pin;
use std::sync::Arc;

use fd_api::core::v1::{
    self as pb, ApplyChangeRequest, ApplyChangeResponse, GetSystemStatusRequest,
    ListOperationsRequest, ListOperationsResponse, ListResourcesRequest, ListResourcesResponse,
    OperationRef, StartOperationRequest, StreamEventsRequest, SystemStatus, ValidationReport,
    core_control_server::CoreControl,
};
use fd_journal::OperationState;
use fd_state::{ApplyError, Change, ChangeOrigin, ResourceKind};
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
        check_metadata(request.metadata(), self.app.config.interface_version, false)?;
        Ok(Response::new(SystemStatus {
            interface_version: self.app.config.interface_version.into(),
            release_version: self.app.config.release_version.into(),
            appliance_id: String::new(),
            appliance_name: self
                .app
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
                .unwrap_or_default(),
            components: Vec::new(),
            last_state_snapshot_at: None,
            state_protection_warning: false,
            notification_channel_missing: false,
        }))
    }

    async fn apply_change(
        &self,
        request: Request<ApplyChangeRequest>,
    ) -> Result<Response<ApplyChangeResponse>, Status> {
        check_metadata(request.metadata(), self.app.config.interface_version, true)?;
        let actor = actor_of(request.metadata());
        let request_id = request_id_of(request.metadata());
        let changes = proto_changes(request.into_inner())?;
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
        let op = self
            .app
            .journal
            .start(&req.kind, serde_json::json!({}), None, &actor, &[])
            .map_err(|e| Status::internal(e.to_string()))?;
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
        let ops = self
            .app
            .journal
            .list(inner.active_only, after, limit)
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(ListOperationsResponse {
            operations: ops.iter().map(operation_to_proto).collect(),
            next_page_token: ops.last().map(|o| o.id.clone()).unwrap_or_default(),
        }))
    }

    type StreamEventsStream = Pin<Box<dyn Stream<Item = Result<pb::Event, Status>> + Send>>;

    async fn stream_events(
        &self,
        request: Request<StreamEventsRequest>,
    ) -> Result<Response<Self::StreamEventsStream>, Status> {
        check_metadata(request.metadata(), self.app.config.interface_version, false)?;
        let after = request.into_inner().after_seq;
        let backlog = self
            .app
            .events
            .after(after, 1000)
            .map_err(|e| Status::internal(e.to_string()))?;
        let mut rx = self.app.events.subscribe();
        let stream = async_stream(backlog, rx);
        Ok(Response::new(Box::pin(stream)))
    }
}

fn async_stream(
    backlog: Vec<fd_observe::Event>,
    mut rx: tokio::sync::broadcast::Receiver<fd_observe::Event>,
) -> impl Stream<Item = Result<pb::Event, Status>> {
    async_stream_impl(backlog, rx)
}

fn async_stream_impl(
    backlog: Vec<fd_observe::Event>,
    mut rx: tokio::sync::broadcast::Receiver<fd_observe::Event>,
) -> impl Stream<Item = Result<pb::Event, Status>> {
    tokio_stream::wrappers::ReceiverStream::new({
        let (tx, rx_out) = tokio::sync::mpsc::channel(64);
        tokio::spawn(async move {
            for event in backlog {
                if tx.send(Ok(event_to_proto(&event))).await.is_err() {
                    return;
                }
            }
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        if tx.send(Ok(event_to_proto(&event))).await.is_err() {
                            return;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => return,
                }
            }
        });
        rx_out
    })
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

fn operation_to_proto(op: &fd_journal::Operation) -> pb::Operation {
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
        started_at: None,
        finished_at: None,
        error: op.error.clone().unwrap_or_default(),
    }
}

fn event_to_proto(event: &fd_observe::Event) -> pb::Event {
    pb::Event {
        seq: event.seq,
        at: None,
        r#type: event.r#type.clone(),
        subject: event.subject.as_ref().map(proto_conv::ref_to_proto),
        payload: Some(proto_conv::value_to_struct(&event.payload)),
    }
}
