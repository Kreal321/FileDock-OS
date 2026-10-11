use std::sync::Arc;

use fd_alerts::AlertState;
use fd_api::core::v1::{
    self as pb, ListAlertsRequest, ListAlertsResponse, QueryLogsRequest, QueryLogsResponse,
    observability_server::Observability,
};
use tonic::{Request, Response, Status};

use super::App;
use super::interceptors::{actor_of, check_metadata};
use crate::proto_conv;

pub struct ObservabilitySvc {
    pub app: Arc<App>,
}

#[tonic::async_trait]
impl Observability for ObservabilitySvc {
    async fn query_logs(
        &self,
        request: Request<QueryLogsRequest>,
    ) -> Result<Response<QueryLogsResponse>, Status> {
        check_metadata(request.metadata(), self.app.config.interface_version, false)?;
        let _ = request;
        Ok(Response::new(QueryLogsResponse {
            entries: Vec::new(),
            next_page_token: String::new(),
        }))
    }

    type CreateDiagnosticBundleStream =
        tokio_stream::wrappers::ReceiverStream<Result<pb::BundleChunk, Status>>;

    async fn create_diagnostic_bundle(
        &self,
        request: Request<pb::Empty>,
    ) -> Result<Response<Self::CreateDiagnosticBundleStream>, Status> {
        check_metadata(request.metadata(), self.app.config.interface_version, true)?;
        let (tx, rx) = tokio::sync::mpsc::channel(1);
        let _ = tx;
        Ok(Response::new(tokio_stream::wrappers::ReceiverStream::new(
            rx,
        )))
    }

    async fn list_alerts(
        &self,
        request: Request<ListAlertsRequest>,
    ) -> Result<Response<ListAlertsResponse>, Status> {
        check_metadata(request.metadata(), self.app.config.interface_version, false)?;
        let inner = request.into_inner();
        let states: Vec<AlertState> = inner
            .states
            .iter()
            .filter_map(|s| AlertState::parse(s))
            .collect();
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
        let alerts = self
            .app
            .alerts
            .list(&states, after, limit)
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(ListAlertsResponse {
            alerts: alerts.iter().map(alert_to_proto).collect(),
            next_page_token: alerts.last().map(|a| a.id.clone()).unwrap_or_default(),
        }))
    }

    async fn acknowledge_alert(
        &self,
        request: Request<pb::AlertRef>,
    ) -> Result<Response<pb::Alert>, Status> {
        check_metadata(request.metadata(), self.app.config.interface_version, true)?;
        let actor = actor_of(request.metadata());
        let alert = self
            .app
            .alerts
            .acknowledge(&request.get_ref().id, &actor)
            .map_err(|e| Status::not_found(e.to_string()))?;
        Ok(Response::new(alert_to_proto(&alert)))
    }
}

fn alert_to_proto(alert: &fd_alerts::Alert) -> pb::Alert {
    pb::Alert {
        id: alert.id.clone(),
        severity: alert.severity.as_str().into(),
        kind: alert.kind.clone(),
        source: alert.source.clone(),
        object: alert.object.as_ref().map(proto_conv::ref_to_proto),
        message: alert.message.clone(),
        first_seen_at: None,
        last_seen_at: None,
        state: alert.state.as_str().into(),
        acknowledged_by: alert.acknowledged_by.clone().unwrap_or_default(),
    }
}
