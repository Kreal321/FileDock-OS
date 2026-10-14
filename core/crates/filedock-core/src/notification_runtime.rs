//! Notification configuration is authoritative Settings state; delivery attempts are disposable.
use crate::api::App;
use fd_api::{core::v1 as pb, extension::v1 as ep};
use fd_state::{
    Change, ChangeOrigin, ResourceKind, ResourceRef,
    resources::{NotificationChannel, ResourceSpec},
};
use serde_json::json;
use std::sync::Arc;
use tonic::Status;
fn error(e: impl std::fmt::Display) -> Status {
    Status::failed_precondition(e.to_string())
}
pub fn channels(app: &App) -> Result<Vec<NotificationChannel>, Status> {
    let row = app
        .store
        .get(&ResourceRef::new(ResourceKind::Settings, "default"))
        .map_err(error)?
        .ok_or_else(|| error("settings.missing"))?;
    match row.parsed_spec().map_err(|i| error(i.message))? {
        ResourceSpec::Settings(s) => Ok(s.notification_channels),
        _ => unreachable!(),
    }
}
pub fn to_proto(c: NotificationChannel) -> pb::NotificationChannel {
    pb::NotificationChannel {
        id: c.id,
        subsystem_id: c.subsystem_id,
        name: c.name,
        min_severity: c.min_severity,
        config: Some(fd_subsystems::remote::to_struct(&c.config)),
        secret_refs: c.secret_refs,
    }
}
pub fn save(
    app: &App,
    channel: Option<NotificationChannel>,
    id: &str,
    actor: String,
) -> Result<(), Status> {
    let reference = ResourceRef::new(ResourceKind::Settings, "default");
    let current = app
        .store
        .get(&reference)
        .map_err(error)?
        .ok_or_else(|| error("settings.missing"))?;
    let mut rows = channels(app)?;
    rows.retain(|c| c.id != id);
    if let Some(channel) = channel {
        rows.push(channel)
    }
    let mut spec = current.spec;
    spec["notificationChannels"] = json!(rows);
    app.store
        .apply(
            ChangeOrigin::Management {
                actor,
                request_id: fd_state::new_id(),
            },
            &[Change::Upsert {
                resource: reference,
                expected_generation: current.generation,
                spec,
            }],
        )
        .map_err(error)?;
    Ok(())
}
pub async fn validate(app: &App, c: &NotificationChannel) -> Result<(), Status> {
    if uuid::Uuid::parse_str(&c.id).is_err()
        || c.name.trim().is_empty()
        || c.name.len() > 128
        || fd_alerts::Severity::parse(&c.min_severity).is_none()
        || !c.config.is_object()
    {
        return Err(Status::invalid_argument("notification.channel_invalid"));
    }
    if fd_secrets::redact::redact_value(c.config.clone()) != c.config {
        return Err(Status::invalid_argument(
            "notification.use_secret_references",
        ));
    }
    let provider = app
        .subsystems
        .get(&c.subsystem_id)
        .map_err(error)?
        .ok_or_else(|| error("notification.provider_missing"))?;
    if !provider.enabled
        || provider.state != "Running"
        || !app
            .subsystems
            .effective_capabilities(&provider)
            .map_err(error)?
            .iter()
            .any(|v| v == "notify.channel")
    {
        return Err(error("notification.capability_missing"));
    }
    let spec = crate::extension_runtime::find(app, &c.subsystem_id)?
        .ok_or_else(|| error("notification.extension_required"))?;
    let runtime = crate::extension_runtime::plan(app, &spec.spec, false).await?;
    if c.secret_refs
        .iter()
        .any(|r| !runtime.secrets.contains_key(r))
    {
        return Err(error("notification.secret_not_granted"));
    }
    Ok(())
}
async fn client(
    app: &App,
    c: &NotificationChannel,
) -> Result<ep::notify_channel_client::NotifyChannelClient<tonic::transport::Channel>, Status> {
    tokio::time::timeout(std::time::Duration::from_secs(10), client_inner(app, c))
        .await
        .map_err(|_| Status::deadline_exceeded("notification.provider_timeout"))?
}
async fn client_inner(
    app: &App,
    c: &NotificationChannel,
) -> Result<ep::notify_channel_client::NotifyChannelClient<tonic::transport::Channel>, Status> {
    validate(app, c).await?;
    let row = crate::extension_runtime::find(app, &c.subsystem_id)?
        .ok_or_else(|| error("notification.provider_missing"))?;
    let runtime = crate::extension_runtime::plan(app, &row.spec, false).await?;
    let mut config = row.spec["config"].clone();
    if !config.is_object() {
        config = json!({})
    }
    config["notificationChannels"] = json!(
        channels(app)?
            .into_iter()
            .filter(|v| v.subsystem_id == c.subsystem_id)
            .collect::<Vec<_>>()
    );
    let secrets = runtime
        .secrets
        .iter()
        .map(|(r, p)| {
            (
                r.clone(),
                format!(
                    "{}/secrets/{}",
                    runtime.runtime.display(),
                    p.file_name().unwrap().to_string_lossy()
                ),
            )
        })
        .collect();
    let paths = runtime
        .sandbox
        .bindings
        .iter()
        .map(|m| (m.target.clone(), m.target.clone()))
        .collect();
    runtime
        .remote()
        .configure_full(config, secrets, paths)
        .await
        .map_err(error)?;
    Ok(ep::notify_channel_client::NotifyChannelClient::new(
        crate::runtime::channel(runtime.remote().socket).await?,
    ))
}
pub async fn test(app: &App, id: &str) -> Result<(), Status> {
    let c = channels(app)?
        .into_iter()
        .find(|c| c.id == id)
        .ok_or_else(|| Status::not_found("notification.channel_missing"))?;
    let gate = crate::extension_runtime::lock(app, &c.subsystem_id);
    let _guard = gate.lock().await;
    let mut request = tonic::Request::new(ep::SendTestRequest {
        channel_id: id.into(),
    });
    request.set_timeout(std::time::Duration::from_secs(5));
    let result = client(app, &c)
        .await?
        .send_test(request)
        .await?
        .into_inner();
    if !result.delivered {
        return Err(error("notification.test_failed"));
    }
    Ok(())
}
async fn deliver_channel(app: Arc<App>, c: NotificationChannel) {
    let severity =
        fd_alerts::Severity::parse(&c.min_severity).unwrap_or(fd_alerts::Severity::Critical);
    if let Ok(pending) = fd_alerts::notify::pending(app.store.db(), &c.id, severity) {
        for item in pending {
            let delivery = async {
                let gate = crate::extension_runtime::lock(&app, &c.subsystem_id);
                let _guard = gate.lock().await;
                let mut client = client(&app, &c).await?;
                let alert = &item.alert;
                let payload = ep::AlertPayload {
                    alert_id: alert.id.clone(),
                    kind: alert.kind.clone(),
                    severity: alert.severity.as_str().into(),
                    message: app
                        .secrets
                        .lock()
                        .await
                        .redact_observed(json!(alert.message))
                        .as_str()
                        .unwrap_or("redacted")
                        .into(),
                    object_ref: alert
                        .object
                        .as_ref()
                        .map(ToString::to_string)
                        .unwrap_or_default(),
                    state: alert.state.as_str().into(),
                    first_seen_at: timestamp(&alert.first_seen_at),
                    last_seen_at: timestamp(&alert.last_seen_at),
                    appliance_name: crate::state_runtime::appliance_id(&app),
                    ui_link: format!("/#/settings/alerts?alert={}", alert.id),
                };
                let mut r = tonic::Request::new(ep::DeliverRequest {
                    channel_id: c.id.clone(),
                    transition: item.transition.clone(),
                    attempt_key: item.key.clone(),
                    alert: Some(payload),
                });
                r.set_timeout(std::time::Duration::from_secs(5));
                Ok::<_, Status>(client.deliver(r).await?.into_inner())
            }
            .await;
            let (done, retry) = match delivery {
                Ok(r) => (r.delivered, r.retryable),
                Err(_) => (false, true),
            };
            if !done {
                tracing::warn!(channel=%c.id,attempt=item.attempt,"Notification delivery failed; alert is retained");
            }
            if let Err(e) = fd_alerts::notify::finish(app.store.db(), &c.id, &item, done, retry) {
                tracing::error!(%e,"Notification retry persistence failed")
            }
        }
    }
}
pub async fn run(app: Arc<App>) {
    let mut workers = std::collections::BTreeMap::<String, tokio::task::JoinHandle<()>>::new();
    loop {
        if let Ok(channels) = channels(&app) {
            workers.retain(|id, worker| {
                if !channels.iter().any(|c| &c.id == id) {
                    worker.abort();
                    return false;
                }
                !worker.is_finished()
            });
            for c in channels {
                if !workers.contains_key(&c.id) {
                    workers.insert(c.id.clone(), tokio::spawn(deliver_channel(app.clone(), c)));
                }
            }
        }
        let _ = fd_alerts::notify::prune(app.store.db());
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
}
fn timestamp(value: &str) -> Option<prost_types::Timestamp> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|v| prost_types::Timestamp {
            seconds: v.timestamp(),
            nanos: v.timestamp_subsec_nanos() as i32,
        })
}
