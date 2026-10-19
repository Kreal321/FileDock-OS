//! Per-operation configuration leases. A route change cannot retarget an existing session.
use crate::{api::App, runtime};
use fd_connectors::object_io::{RcloneObjects, S3Objects};
use fd_state::{
    StoredResource,
    resources::{CloudConnectionSpec, ConnectionDefinition},
};
use serde_json::json;
use tonic::Status;
pub async fn native(app: &App, row: &StoredResource, connector: &str) -> Result<S3Objects, Status> {
    let id = format!("session-{}", uuid::Uuid::new_v4());
    let parent = app.config.secrets_run.join(connector);
    std::fs::create_dir_all(&parent).map_err(internal)?;
    let directory = tempfile::Builder::new()
        .prefix("session-")
        .tempdir_in(parent)
        .map_err(internal)?;
    let root = directory.path().to_path_buf();
    let mut files = std::collections::HashMap::new();
    let value = if connector == "fd-s3" {
        let fd_state::resources::ResourceSpec::CloudConnection(connection) =
            row.parsed_spec().map_err(|e| internal(e.code))?
        else {
            return Err(internal("connection.invalid"));
        };
        let ConnectionDefinition::S3(def) = connection.definition else {
            return Err(internal("connection.invalid"));
        };
        let vault = app.secrets.lock().await;
        for (name, reference) in [
            ("access", &def.access_key_ref),
            ("secret", &def.secret_key_ref),
        ] {
            let path = root.join(name);
            runtime::write_private(&path, &vault.get(reference).map_err(internal)?)
                .map_err(internal)?;
            files.insert(format!("{id}/{name}"), path.to_string_lossy().into_owned());
        }
        json!({"id":id,"connector_id":connector,"source_connection":row.resource.id,"endpoint":def.endpoint,"region":def.region,"bucket":def.bucket,"virtual_hosted":def.addressing_style==fd_state::resources::AddressingStyle::Virtual,"access_key_ref":format!("{id}/access"),"secret_key_ref":format!("{id}/secret"),"storage_class":def.storage_class,"kms_key_id":match def.server_side_encryption {Some(fd_state::resources::ServerSideEncryption::Kms{key_id})=>Some(key_id),_=>None},"bandwidth_limit":def.bandwidth_limit,"checksums":crate::runtime::proven_checksums(row,connector)})
    } else {
        let marker = root.join("lease");
        runtime::write_private(&marker, b"active").map_err(internal)?;
        files.insert("lease".into(), marker.to_string_lossy().into_owned());
        json!({"id":id,"connector_id":connector,"spec":row.spec})
    };
    app.temporary_s3
        .lock()
        .await
        .insert(id.clone(), (value, files));
    let socket = if connector == "fd-s3" {
        runtime::s3_socket(app)
    } else {
        crate::extension_runtime::socket(app, connector)
    };
    let result = async {
        runtime::configure(app).await?;
        runtime::channel(socket).await
    }
    .await;
    match result {
        Ok(channel) => Ok(S3Objects {
            channel,
            connection_id: id,
            cleanup_directory: Some(directory.keep()),
            prefix: String::new(),
        }),
        Err(e) => {
            app.temporary_s3.lock().await.remove(&id);
            let _ = std::fs::remove_dir_all(root);
            Err(e)
        }
    }
}
pub async fn rclone(app: &App, connection: &CloudConnectionSpec) -> Result<RcloneObjects, Status> {
    let name = format!("session-{}", uuid::Uuid::new_v4());
    let mut remote =
        fd_connectors::rclone::conf::remotes_for(&[(name.clone(), connection.clone())]).remove(0);
    let vault = app.secrets.lock().await;
    let bucket = match &connection.definition {
        ConnectionDefinition::S3(d) => {
            remote.lines.extend([
                (
                    "access_key_id".into(),
                    String::from_utf8(vault.get(&d.access_key_ref).map_err(internal)?.to_vec())
                        .map_err(internal)?,
                ),
                (
                    "secret_access_key".into(),
                    String::from_utf8(vault.get(&d.secret_key_ref).map_err(internal)?.to_vec())
                        .map_err(internal)?,
                ),
            ]);
            d.bucket.clone()
        }
        ConnectionDefinition::Bound(d) => {
            for reference in &d.credentials {
                let credentials: serde_json::Value =
                    serde_json::from_slice(&vault.get(reference).map_err(internal)?)
                        .map_err(internal)?;
                for (k, v) in credentials
                    .as_object()
                    .ok_or_else(|| internal("connection.credentials_invalid"))?
                {
                    if k != "type"
                        && let Some(value) = v.as_str()
                    {
                        remote.lines.push((k.clone(), value.into()));
                    }
                }
            }
            String::new()
        }
    };
    drop(vault);
    let kind = remote
        .lines
        .iter()
        .find(|(k, _)| k == "type")
        .map(|(_, v)| v.as_str())
        .ok_or_else(|| internal("connection.config_invalid"))?;
    let parameters = serde_json::Value::Object(
        remote
            .lines
            .iter()
            .filter(|(k, _)| k != "type")
            .map(|(k, v)| (k.clone(), json!(v)))
            .collect(),
    );
    let adapter = fd_connectors::rclone::RcloneAdapter::with_socket(
        app.config.socket.parent().unwrap().join("rclone.sock"),
    );
    adapter
        .configure_remote(&name, kind, parameters)
        .await
        .map_err(internal)?;
    Ok(RcloneObjects {
        adapter,
        remote: format!("{name}:{bucket}"),
        prefix: String::new(),
        cleanup_name: Some(name),
    })
}
pub async fn persist_token(
    app: &App,
    connection: &CloudConnectionSpec,
    lease: &RcloneObjects,
) -> Result<(), Status> {
    let ConnectionDefinition::Bound(bound) = &connection.definition else {
        return Ok(());
    };
    let Some(name) = &lease.cleanup_name else {
        return Ok(());
    };
    let values = fd_connectors::oauth::credentials(&lease.adapter, name)
        .await
        .map_err(internal)?;
    let Some(token) = values["token"].as_str() else {
        return Ok(());
    };
    let mut vault = app.secrets.lock().await;
    let mut changed = false;
    for reference in &bound.credentials {
        if let Ok(bytes) = vault.get(reference)
            && let Ok(mut saved) = serde_json::from_slice::<serde_json::Value>(&bytes)
            && saved.get("token").is_some()
            && saved["token"] != token
        {
            saved["token"] = json!(token);
            vault
                .put(
                    reference,
                    &serde_json::to_vec(&saved).map_err(internal)?,
                    vec!["rclone".into()],
                )
                .map_err(internal)?;
            changed = true;
        }
    }
    if changed {
        app.store
            .db()
            .with(|c| {
                fd_state::db::put_setting(c, "config.changed_at", &json!(fd_state::db::now_text()))
            })
            .map_err(internal)?;
    }
    Ok(())
}

fn internal(e: impl std::fmt::Display) -> Status {
    Status::failed_precondition(e.to_string())
}
