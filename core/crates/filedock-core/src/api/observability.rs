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
    async fn query_activity(
        &self,
        r: Request<pb::PageRequest>,
    ) -> Result<Response<pb::ActivityPage>, Status> {
        check_metadata(r.metadata(), self.app.config.interface_version, false)?;
        let request = r.into_inner();
        let before = if request.page_token.is_empty() {
            i64::MAX
        } else {
            request
                .page_token
                .parse::<i64>()
                .map_err(|_| Status::invalid_argument("activity.cursor_invalid"))?
        };
        let events=self.app.store.db().with(|c|{let mut q=c.prepare("SELECT seq,at,type,subject_kind,subject_id,payload FROM event WHERE seq<?1 ORDER BY seq DESC LIMIT ?2")?;Ok::<_,fd_state::DbError>(q.query_map(rusqlite::params![before,request.page_size.clamp(1,500)],|r|Ok((r.get::<_,i64>(0)? as u64,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,Option<String>>(3)?,r.get::<_,Option<String>>(4)?,r.get::<_,String>(5)?)))?.collect::<Result<Vec<_>,_>>()?) }).map_err(|_|Status::internal("activity.query_failed"))?;
        let vault = self.app.secrets.lock().await;
        let entries = events
            .into_iter()
            .map(|(seq, at, kind, subject, id, payload)| pb::Event {
                seq,
                at: super::events::timestamp(&at),
                r#type: kind,
                subject: subject
                    .and_then(|k| fd_state::ResourceKind::parse(&k))
                    .map(|kind| {
                        proto_conv::ref_to_proto(&fd_state::ResourceRef::new(
                            kind,
                            id.as_deref().unwrap_or(""),
                        ))
                    }),
                payload: Some(fd_subsystems::remote::to_struct(
                    &vault.redact_observed(serde_json::from_str(&payload).unwrap_or_default()),
                )),
            })
            .collect::<Vec<_>>();
        Ok(Response::new(pb::ActivityPage {
            next_page_token: if entries.len() == request.page_size.clamp(1, 500) as usize {
                entries.last().unwrap().seq.to_string()
            } else {
                String::new()
            },
            events: entries,
        }))
    }
    async fn query_logs(
        &self,
        request: Request<QueryLogsRequest>,
    ) -> Result<Response<QueryLogsResponse>, Status> {
        check_metadata(request.metadata(), self.app.config.interface_version, false)?;
        let r = request.into_inner();
        let filter = fd_observe::logs::Filter {
            sources: r.sources,
            min_severity: r.min_severity,
            since: r
                .since
                .and_then(|t| chrono::DateTime::from_timestamp(t.seconds, t.nanos as u32))
                .map(|t| t.to_rfc3339()),
            until: r
                .until
                .and_then(|t| chrono::DateTime::from_timestamp(t.seconds, t.nanos as u32))
                .map(|t| t.to_rfc3339()),
            text: r.text,
            cursor: r
                .page
                .as_ref()
                .map(|p| p.page_token.clone())
                .unwrap_or_default(),
            limit: r.page.map(|p| p.page_size as usize).unwrap_or(100),
        };
        let mut result = fd_observe::logs::query(&filter).await.map_err(|e| {
            if e == "logs.unavailable" {
                Status::unavailable(e)
            } else {
                Status::invalid_argument(e)
            }
        })?;
        let vault = self.app.secrets.lock().await;
        for entry in &mut result.entries {
            entry.message = vault
                .redact_observed(serde_json::json!(entry.message))
                .as_str()
                .unwrap_or("redacted")
                .into();
        }
        Ok(Response::new(QueryLogsResponse {
            entries: result
                .entries
                .into_iter()
                .map(|e| pb::LogEntry {
                    at: super::events::timestamp(&e.at),
                    source: e.source,
                    severity: e.severity,
                    message: e.message,
                })
                .collect(),
            next_page_token: result.next_page_token,
        }))
    }

    type CreateDiagnosticBundleStream =
        tokio_stream::wrappers::ReceiverStream<Result<pb::BundleChunk, Status>>;

    async fn create_diagnostic_bundle(
        &self,
        request: Request<pb::Empty>,
    ) -> Result<Response<Self::CreateDiagnosticBundleStream>, Status> {
        check_metadata(request.metadata(), self.app.config.interface_version, true)?;
        let mut configuration = serde_json::Map::new();
        for kind in fd_state::ResourceKind::ALL {
            let rows = self
                .app
                .store
                .list(kind, None, 10000)
                .map_err(|_| Status::internal("diagnostics.configuration_unavailable"))?;
            configuration.insert(kind.as_str().into(),serde_json::json!(rows.into_iter().map(|r|serde_json::json!({"id":r.resource.id,"spec":r.spec,"status":r.status})).collect::<Vec<_>>()));
        }
        let journal = fd_observe::logs::query(&fd_observe::logs::Filter {
            limit: 500,
            ..Default::default()
        })
        .await
        .map(|p| serde_json::json!(p.entries))
        .unwrap_or_else(|e| serde_json::json!({"unavailable":e}));
        let subsystems = self
            .app
            .subsystems
            .list()
            .map_err(|_| Status::internal("diagnostics.subsystems_unavailable"))?;
        let services = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            tokio::process::Command::new("systemctl")
                .args([
                    "show",
                    "filedock-core.service",
                    "filedock-management.service",
                    "docker.service",
                    "--property=Id,ActiveState,SubState,Result,MainPID,NRestarts",
                ])
                .kill_on_drop(true)
                .output(),
        )
        .await
        .ok()
        .and_then(Result::ok)
        .map(|r| String::from_utf8_lossy(&r.stdout).into_owned())
        .unwrap_or_else(|| "Service status unavailable".into());
        let vault = self.app.secrets.lock().await;
        let files = vec![
            (
                "configuration.json".into(),
                serde_json::Value::Object(configuration),
            ),
            (
                "subsystem-health.json".into(),
                serde_json::json!(
                    subsystems
                        .iter()
                        .map(|s| serde_json::json!({"id":s.id,"state":s.state,"version":s.version}))
                        .collect::<Vec<_>>()
                ),
            ),
            (
                "services.json".into(),
                serde_json::json!({"status":services}),
            ),
            ("logs.json".into(), journal),
        ]
        .into_iter()
        .map(|(n, v)| (n, vault.redact_observed(v)))
        .collect();
        let bytes = fd_observe::bundle::create(files)
            .map_err(|_| Status::internal("diagnostics.bundle_failed"))?;
        drop(vault);
        let (tx, rx) = tokio::sync::mpsc::channel(2);
        tokio::spawn(async move {
            for data in bytes.chunks(64 * 1024) {
                if tx
                    .send(Ok(pb::BundleChunk {
                        data: data.to_vec(),
                    }))
                    .await
                    .is_err()
                {
                    break;
                }
            }
        });
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
        if inner.states.iter().any(|s| AlertState::parse(s).is_none()) {
            return Err(Status::invalid_argument("alerts.state_invalid"));
        }
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
        if !inner.min_severity.is_empty()
            && fd_alerts::Severity::parse(&inner.min_severity).is_none()
        {
            return Err(Status::invalid_argument("alerts.severity_invalid"));
        }
        let alerts = self
            .app
            .alerts
            .list_minimum(
                &states,
                after,
                limit.clamp(1, 500),
                fd_alerts::Severity::parse(&inner.min_severity),
            )
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(ListAlertsResponse {
            alerts: alerts
                .iter()
                .filter(|a| {
                    fd_alerts::Severity::parse(&inner.min_severity).is_none_or(|s| a.severity >= s)
                })
                .map(alert_to_proto)
                .collect(),
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
        self.app
            .events
            .emit(
                "alert.changed",
                alert.object.clone(),
                serde_json::json!({"id":alert.id,"state":alert.state.as_str()}),
            )
            .map_err(|_| Status::internal("alert.event_failed"))?;
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
        first_seen_at: super::events::timestamp(&alert.first_seen_at),
        last_seen_at: super::events::timestamp(&alert.last_seen_at),
        state: alert.state.as_str().into(),
        acknowledged_by: alert.acknowledged_by.clone().unwrap_or_default(),
    }
}
