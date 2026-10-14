//! Per-instance authentication and manifest permission checks on Core callbacks.
use fd_api::extension::v1::{self as pb, core_bridge_server::CoreBridge};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    pin::Pin,
    sync::{Arc, Mutex},
};
use tonic::{Request, Response, Status};
#[derive(Clone, Debug)]
pub struct Instance {
    pub id: String,
    pub version: String,
    pub socket: String,
    pub capabilities: BTreeSet<String>,
    pub permissions: BTreeSet<String>,
    pub registered: bool,
}
#[derive(Default)]
pub struct Instances(Mutex<BTreeMap<String, Instance>>);
impl Instances {
    pub fn issue(&self, token: &str, instance: Instance) {
        let mut entries = self.0.lock().unwrap();
        entries.retain(|_, v| v.id != instance.id);
        entries.insert(hex::encode(Sha256::digest(token.as_bytes())), instance);
    }
    pub fn revoke(&self, id: &str) {
        self.0.lock().unwrap().retain(|_, v| v.id != id);
    }
    pub fn authenticate(&self, token: &str, permission: Option<&str>) -> Result<Instance, Status> {
        let instance = self
            .0
            .lock()
            .unwrap()
            .get(&hex::encode(Sha256::digest(token.as_bytes())))
            .cloned()
            .ok_or_else(|| Status::unauthenticated("extension.token_invalid"))?;
        if permission.is_some_and(|p| !instance.permissions.contains(p)) {
            tracing::warn!(subsystem=%instance.id,"extension permission denied");
            return Err(Status::permission_denied("extension.permission_denied"));
        }
        Ok(instance)
    }
    pub fn register(&self, token: &str, body: &pb::RegisterRequest) -> Result<Instance, Status> {
        let mut entries = self.0.lock().unwrap();
        let instance = entries
            .get_mut(&hex::encode(Sha256::digest(token.as_bytes())))
            .ok_or_else(|| Status::unauthenticated("extension.token_invalid"))?;
        if body.subsystem_id != instance.id
            || body.version != instance.version
            || body.socket_path != instance.socket
            || body
                .capabilities
                .iter()
                .any(|c| !instance.capabilities.contains(c))
        {
            return Err(Status::permission_denied("extension.registration_mismatch"));
        }
        instance.registered = true;
        Ok(instance.clone())
    }
    pub fn registered(&self, id: &str) -> bool {
        self.0
            .lock()
            .unwrap()
            .values()
            .any(|v| v.id == id && v.registered)
    }
}
pub struct Bridge {
    pub instances: Arc<Instances>,
    pub events: Arc<fd_observe::EventBus>,
    pub alerts: Arc<fd_alerts::AlertStore>,
    pub interface_version: String,
    pub registry: crate::SubsystemRegistry,
    pub secrets: Arc<tokio::sync::Mutex<fd_secrets::SecretStore>>,
}
impl Bridge {
    fn identity<T>(&self, r: &Request<T>, permission: Option<&str>) -> Result<Instance, Status> {
        let token = r
            .metadata()
            .get("fd-extension-token")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| Status::unauthenticated("extension.token_missing"))?;
        self.instances.authenticate(token, permission)
    }
}
#[tonic::async_trait]
impl CoreBridge for Bridge {
    async fn register(
        &self,
        r: Request<pb::RegisterRequest>,
    ) -> Result<Response<pb::RegisterResponse>, Status> {
        let token = r
            .metadata()
            .get("fd-extension-token")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| Status::unauthenticated("extension.token_missing"))?
            .to_owned();
        let body = r.into_inner();
        let instance = self.instances.register(&token, &body)?;
        self.registry
            .record_capabilities(&instance.id, &body.capabilities)
            .map_err(|_| Status::internal("extension.registration_failed"))?;
        Ok(Response::new(pb::RegisterResponse {
            interface_version: self.interface_version.clone(),
            granted_permissions: instance.permissions.into_iter().collect(),
        }))
    }
    type StreamEventsStream =
        Pin<Box<dyn tokio_stream::Stream<Item = Result<pb::Event, Status>> + Send>>;
    async fn stream_events(
        &self,
        r: Request<pb::StreamEventsRequest>,
    ) -> Result<Response<Self::StreamEventsStream>, Status> {
        self.identity(&r, Some("core.events.read"))?;
        let token = r
            .metadata()
            .get("fd-extension-token")
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        let instances = self.instances.clone();
        let body = r.into_inner();
        let (tx, rx) = tokio::sync::mpsc::channel(64);
        let events = self.events.clone();
        let mut live = events.subscribe();
        tokio::spawn(async move {
            let mut seq = body.after_seq;
            loop {
                if instances
                    .authenticate(&token, Some("core.events.read"))
                    .is_err()
                {
                    return;
                }
                match events.after(seq, 256) {
                    Ok(batch) => {
                        let more = batch.len() == 256;
                        for e in batch {
                            seq = e.seq;
                            if !body.types.is_empty() && !body.types.contains(&e.r#type) {
                                continue;
                            }
                            let at = chrono::DateTime::parse_from_rfc3339(&e.at).ok().map(|d| {
                                prost_types::Timestamp {
                                    seconds: d.timestamp(),
                                    nanos: d.timestamp_subsec_nanos() as i32,
                                }
                            });
                            let event = pb::Event {
                                seq: e.seq,
                                at,
                                r#type: e.r#type,
                                subject_kind: e
                                    .subject
                                    .as_ref()
                                    .map(|s| s.kind.as_str().into())
                                    .unwrap_or_default(),
                                subject_id: e.subject.map(|s| s.id).unwrap_or_default(),
                                payload: Some(crate::remote::to_struct(&e.payload)),
                            };
                            if tx.send(Ok(event)).await.is_err() {
                                return;
                            }
                        }
                        if more {
                            continue;
                        }
                    }
                    Err(_) => return,
                }
                tokio::select! {_=tx.closed()=>return,_=live.recv()=>{},_=tokio::time::sleep(std::time::Duration::from_secs(1))=>{}}
            }
        });
        Ok(Response::new(Box::pin(
            tokio_stream::wrappers::ReceiverStream::new(rx),
        )))
    }
    async fn raise_alert(
        &self,
        r: Request<pb::RaiseAlertRequest>,
    ) -> Result<Response<pb::Empty>, Status> {
        let instance = self.identity(&r, Some("core.alerts.write"))?;
        let token = r
            .metadata()
            .get("fd-extension-token")
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        let body = r.into_inner();
        let message = self
            .secrets
            .lock()
            .await
            .redact_observed(serde_json::json!(body.message.replace(&token, "••••")))
            .as_str()
            .unwrap_or("[redacted]")
            .to_owned();
        if body.kind.len() > 128 || body.message.len() > 4096 {
            return Err(Status::invalid_argument("extension.alert_invalid"));
        }
        let severity = fd_alerts::Severity::parse(&body.severity)
            .ok_or_else(|| Status::invalid_argument("extension.alert_invalid"))?;
        let source = format!("extension:{}", instance.id);
        if body.resolved {
            self.alerts
                .resolve(&source, None, &body.kind)
                .map_err(|_| Status::internal("extension.alert_failed"))?;
        } else {
            self.alerts
                .raise(&source, None, &body.kind, severity, &message)
                .map_err(|_| Status::internal("extension.alert_failed"))?;
        }
        self.events
            .emit(
                "alert.changed",
                None,
                serde_json::json!({"source":source,"kind":body.kind,"resolved":body.resolved}),
            )
            .map_err(|_| Status::internal("extension.alert_event_failed"))?;
        Ok(Response::new(pb::Empty {}))
    }
    async fn report_health(
        &self,
        r: Request<pb::HealthReport>,
    ) -> Result<Response<pb::Empty>, Status> {
        let instance = self.identity(&r, None)?;
        let token = r
            .metadata()
            .get("fd-extension-token")
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        let report = r.into_inner();
        if report.detail.len() > 4096 {
            return Err(Status::invalid_argument(
                "extension.health_detail_too_large",
            ));
        }
        let detail = self
            .secrets
            .lock()
            .await
            .redact_observed(serde_json::json!(report.detail.replace(&token, "••••")));
        self.events
            .emit(
                "subsystem.health",
                None,
                serde_json::json!({"id":instance.id,"status":report.status,"detail":detail}),
            )
            .map_err(|_| Status::internal("extension.health_failed"))?;
        Ok(Response::new(pb::Empty {}))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tokens_are_instance_scoped_and_revoked() {
        let registry = Instances::default();
        let instance = Instance {
            id: "sample".into(),
            version: "1".into(),
            socket: "/run/example.sock".into(),
            capabilities: BTreeSet::new(),
            permissions: BTreeSet::from(["core.events.read".into()]),
            registered: false,
        };
        registry.issue("one", instance.clone());
        assert!(
            registry
                .authenticate("one", Some("core.alerts.write"))
                .is_err()
        );
        assert!(
            registry
                .authenticate("one", Some("core.events.read"))
                .is_ok()
        );
        registry.issue("two", instance);
        assert!(registry.authenticate("one", None).is_err());
        registry.revoke("sample");
        assert!(registry.authenticate("two", None).is_err());
    }
}
