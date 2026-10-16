use crate::api::App;
use fd_api::core::v1 as pb;
use fd_connectors::{oauth, rclone::RcloneAdapter};
use serde_json::Value;
use std::sync::Arc;
use tonic::Status;
pub struct Flow {
    pub actor: String,
    pub remote: String,
    pub state: String,
    pub expires: std::time::Instant,
}
fn fail(_: impl std::fmt::Display) -> Status {
    Status::failed_precondition("connection.authorization_failed")
}
fn adapter(app: &App) -> RcloneAdapter {
    RcloneAdapter::with_socket(app.config.socket.parent().unwrap().join("rclone-auth.sock"))
}
pub async fn start(
    app: &Arc<App>,
    actor: &str,
    family: &str,
    config: Value,
) -> Result<pb::OAuthFlow, Status> {
    if !app
        .subsystems
        .get("rclone")
        .map_err(fail)?
        .is_some_and(|r| r.enabled)
    {
        return Err(Status::failed_precondition("connection.family_unserved"));
    }
    crate::connection_runtime::validate(
        app,
        &serde_json::json!({"providerFamily":family,"definition":{"type":"bound","connectorId":"rclone","config":config,"credentials":[]}}),
    )?;
    // Serialize remote creation against orphan cleanup and other authorization starts.
    let mut flows = app.oauth_flows.lock().await;
    let socket = app.config.socket.parent().unwrap().join("rclone-auth.sock");
    if !socket.exists() {
        crate::storage_runtime::enabled()?;
        let mut properties = std::collections::HashMap::new();
        properties.insert("Type".into(), "simple".into());
        properties.insert(
            "ExecStart".into(),
            format!(
                "/usr/bin/rclone rcd --rc-addr unix://{} --config {} --rc-no-auth",
                socket.display(),
                socket.parent().unwrap().join("rclone-auth.conf").display()
            ),
        );
        properties.insert("UMask".into(), "0077".into());
        properties.insert("NoNewPrivileges".into(), "yes".into());
        fd_subsystems::Systemd::default()
            .start_transient("filedock-rclone-auth.service", properties)
            .await
            .map_err(fail)?;
        let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !socket.exists() {
            if std::time::Instant::now() > until {
                return Err(fail("authorization service unavailable"));
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }
    let id = uuid::Uuid::new_v4().to_string();
    let remote = format!("fd-oauth-{id}");
    let output = oauth::start(&adapter(app), &remote, family, config)
        .await
        .map_err(fail)?;
    let state = oauth::state(&output).to_string();
    let question = oauth::question(&output).to_string();
    flows.insert(
        id.clone(),
        Flow {
            actor: actor.into(),
            remote,
            state,
            expires: std::time::Instant::now() + std::time::Duration::from_secs(900),
        },
    );
    drop(flows);
    response(app, &id, question).await
}
async fn response(app: &Arc<App>, id: &str, question: String) -> Result<pb::OAuthFlow, Status> {
    let mut flows = app.oauth_flows.lock().await;
    let flow = flows
        .get(id)
        .ok_or_else(|| Status::not_found("connection.flow_not_found"))?;
    if flow.state.is_empty() {
        let credentials = oauth::credentials(&adapter(app), &flow.remote)
            .await
            .map_err(fail)?;
        let reference = format!("connection/oauth-{id}/credentials");
        app.secrets
            .lock()
            .await
            .put(
                &reference,
                &serde_json::to_vec(&credentials).map_err(fail)?,
                vec!["rclone".into()],
            )
            .map_err(fail)?;
        oauth::discard(&adapter(app), &flow.remote)
            .await
            .map_err(fail)?;
        flows.remove(id);
        return Ok(pb::OAuthFlow {
            flow_id: id.into(),
            complete: true,
            credential_ref: reference,
            ..Default::default()
        });
    }
    let auth_url = question
        .split_whitespace()
        .find(|s| s.starts_with("https://"))
        .unwrap_or("")
        .trim_matches(['"', '\''])
        .to_string();
    Ok(pb::OAuthFlow {
        flow_id: id.into(),
        auth_url,
        question,
        ..Default::default()
    })
}
pub async fn finish(
    app: &Arc<App>,
    actor: &str,
    id: &str,
    input: &str,
) -> Result<pb::OAuthFlow, Status> {
    let mut flows = app.oauth_flows.lock().await;
    let flow = flows
        .get_mut(id)
        .ok_or_else(|| Status::not_found("connection.flow_not_found"))?;
    if flow.actor != actor {
        return Err(Status::permission_denied("connection.flow_owner_mismatch"));
    }
    if flow.expires < std::time::Instant::now() {
        let remote = flow.remote.clone();
        flows.remove(id);
        drop(flows);
        let _ = oauth::discard(&adapter(app), &remote).await;
        return Err(Status::failed_precondition("connection.flow_expired"));
    }
    let output = oauth::finish(&adapter(app), &flow.remote, &flow.state, input)
        .await
        .map_err(fail)?;
    flow.state = oauth::state(&output).to_string();
    let question = oauth::question(&output).to_string();
    drop(flows);
    response(app, id, question).await
}

pub async fn maintenance(app: Arc<App>) {
    loop {
        let mut expired = Vec::new();
        let mut flows = app.oauth_flows.lock().await;
        {
            flows.retain(|_, f| {
                if f.expires < std::time::Instant::now() {
                    expired.push(f.remote.clone());
                    false
                } else {
                    true
                }
            });
        }
        let auth = adapter(&app);
        if app
            .config
            .socket
            .parent()
            .unwrap()
            .join("rclone-auth.sock")
            .exists()
        {
            if let Ok(remotes) = oauth::remotes(&auth).await {
                let active = flows
                    .values()
                    .map(|f| f.remote.clone())
                    .collect::<std::collections::BTreeSet<_>>();
                expired.extend(
                    remotes
                        .into_iter()
                        .filter(|r| r.starts_with("fd-oauth-") && !active.contains(r)),
                );
            }
            for name in expired {
                let _ = oauth::discard(&auth, &name).await;
            }
        }
        drop(flows);
        if let Ok(connections) =
            crate::storage_runtime::rows(&app, fd_state::ResourceKind::CloudConnection)
        {
            for connection in connections {
                let mut expiring = None;
                for reference in connection.spec["definition"]["credentials"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|v| v.as_str())
                {
                    if let Ok(bytes) = app.secrets.lock().await.get(reference)
                        && let Ok(value) = serde_json::from_slice::<Value>(&bytes)
                        && let Some(token) = value["token"].as_str()
                        && let Ok(token) = serde_json::from_str::<Value>(token)
                        && let Some(expiry) = authorization_expiry(&token)
                        && expiry.timestamp() > 1_500_000_000
                        && expiry < chrono::Utc::now() + chrono::Duration::days(7)
                    {
                        expiring = Some(expiry.to_rfc3339());
                    }
                }
                if let Some(expiry) = expiring {
                    let _=app.alerts.raise("connections",Some(connection.resource),"connection.authorization_expiring",fd_alerts::Severity::Warning,&format!("Cloud authorization expires by {expiry}; authorize this connection again"));
                } else {
                    let _ = app.alerts.resolve(
                        "connections",
                        Some(&connection.resource),
                        "connection.authorization_expiring",
                    );
                }
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
    }
}

// An ordinary refreshable access token expiring in an hour does not require reauthorization.
fn authorization_expiry(token: &Value) -> Option<chrono::DateTime<chrono::FixedOffset>> {
    let deadline = token
        .get("refresh_token_expiry")
        .and_then(Value::as_str)
        .or_else(|| {
            if token["refresh_token"]
                .as_str()
                .is_some_and(|s| !s.is_empty())
            {
                None
            } else {
                token["expiry"].as_str()
            }
        })?;
    chrono::DateTime::parse_from_rfc3339(deadline).ok()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn refreshable_access_expiry_does_not_raise_a_reauthorization_warning() {
        let mut token =
            serde_json::json!({"expiry":"2026-10-16T00:00:00Z","refresh_token":"refresh-canary"});
        assert!(authorization_expiry(&token).is_none());
        token["refresh_token_expiry"] = serde_json::json!("2026-10-18T00:00:00Z");
        assert!(authorization_expiry(&token).is_some());
        assert!(
            authorization_expiry(&serde_json::json!({"expiry":"2026-10-16T00:00:00Z"})).is_some()
        );
    }
}
