//! Durable restore journal. No key material or source credentials enter the operation journal.
use crate::{
    api::{App, events::timestamp},
    state_runtime::{self as runtime, error},
};
use fd_api::core::v1 as pb;
use fd_appstate::{
    atomic_write,
    format::fdstate,
    restore::{apply::import_config, report::build_report},
};
use fd_connectors::ownership::{self, ObjectIo, Role};
use fd_secrets::{RecoveryKeyStore, SecretStore};
use fd_state::db;
use serde_json::{Value, json};
use std::{collections::HashMap, path::PathBuf, sync::Arc};
use tonic::Status;
const STEPS: &[&str] = &[
    "verify",
    "install-secrets",
    "import-core",
    "import-management",
    "provision",
    "report",
];
pub fn throttle(app: &App) -> Result<(), Status> {
    let now = chrono::Utc::now().timestamp();
    let permitted = app
        .store
        .db()
        .with(|c| {
            let mut attempts = db::get_setting(c, "restore.key_attempts")?
                .and_then(|v| v.as_array().cloned())
                .unwrap_or_default();
            attempts.retain(|v| v.as_i64().is_some_and(|t| now - t < 900));
            if attempts.len() >= 5 {
                return Ok::<_, fd_state::DbError>(false);
            }
            attempts.push(json!(now));
            db::put_setting(c, "restore.key_attempts", &json!(attempts))?;
            Ok(true)
        })
        .map_err(error)?;
    if permitted {
        Ok(())
    } else {
        Err(Status::resource_exhausted("restore.key_throttled"))
    }
}
fn stage(app: &App, id: &str) -> Result<PathBuf, Status> {
    uuid::Uuid::parse_str(id).map_err(|_| Status::invalid_argument("operation.id_invalid"))?;
    Ok(app.config.state_spool.join("restore").join(id))
}
pub async fn begin(
    app: &Arc<App>,
    source_id: &str,
    snapshot_id: &str,
) -> Result<fd_journal::Operation, Status> {
    let _guard = app.restore_lock.lock().await;
    let sources = app.restore_sources.lock().await;
    let primary = sources
        .get(source_id)
        .ok_or_else(|| error("restore.source_not_found"))?;
    let mut selected = None;
    for source in std::iter::once(primary).chain(
        primary
            .alternate_source_ids
            .iter()
            .filter_map(|id| sources.get(id)),
    ) {
        if let Ok(payload) = source.snapshot(snapshot_id).await {
            selected = Some((source, payload));
            break;
        }
    }
    let (source, payload) =
        selected.ok_or_else(|| error("restore.snapshot_not_found_or_invalid"))?;
    SecretStore::validate_snapshot(&payload.secrets, &payload.dek)
        .map_err(|_| error("restore.secrets_invalid"))?;
    // Stage only encrypted archive/package bytes. Context is encrypted under both DEKs so a crash
    // between atomic key and secret replacements can resume without the recovery key being stored.
    let op = app
        .journal
        .start(
            "Restore",
            json!({"snapshotId":snapshot_id,"local":source.local}),
            None,
            "recovery-key",
            STEPS,
        )
        .map_err(error)?;
    let root = stage(app, &op.id)?;
    std::fs::create_dir_all(&root).map_err(error)?;
    let entry = source
        .index
        .snapshots
        .iter()
        .find(|s| s.snapshot_id == snapshot_id)
        .unwrap();
    let raw = source
        .backend
        .read(&format!(
            "appliance/{}/{}",
            source.index.appliance_id, entry.file
        ))
        .await
        .map_err(error)?
        .ok_or_else(|| error("restore.snapshot_missing"))?;
    atomic_write(&root.join("snapshot.fdstate"), &raw).map_err(error)?;
    let context = json!({"key":source.key,"masters":source.masters.iter().map(|(id,key)|(id.clone(),json!(hex::encode(key.as_ref())))).collect::<serde_json::Map<_,_>>(),"entry":entry});
    let context = serde_json::to_vec(&context).map_err(error)?;
    let old_dek = app.secrets.lock().await.dek_bytes();
    atomic_write(
        &root.join("context.old"),
        &fdstate::encrypt_bytes(&context, &old_dek, b"FDCTX\0\0\0").map_err(error)?,
    )
    .map_err(error)?;
    atomic_write(
        &root.join("context.new"),
        &fdstate::encrypt_bytes(&context, &payload.dek, b"FDCTX\0\0\0").map_err(error)?,
    )
    .map_err(error)?;
    for hash in &entry.packages {
        let mut package = source.package(hash).await.map_err(error)?;
        if package.is_none() {
            for other in std::iter::once(primary).chain(
                primary
                    .alternate_source_ids
                    .iter()
                    .filter_map(|id| sources.get(id)),
            ) {
                if let Ok(Some(bytes)) = other.package(hash).await {
                    package = Some(bytes);
                    break;
                }
            }
        }
        if let Some(bytes) = package {
            atomic_write(&root.join(format!("{hash}.fdext")), &bytes).map_err(error)?;
        }
    }
    drop(sources);
    drop(_guard);
    let app = app.clone();
    let id = op.id.clone();
    tokio::spawn(async move {
        if let Err(e) = resume(&app, &id).await {
            let _ =
                app.journal
                    .set_state(&id, fd_journal::OperationState::Failed, Some(e.message()));
            let _ = app.events.emit(
                "operation.failed",
                None,
                json!({"id":id,"code":e.message()}),
            );
        }
    });
    Ok(op)
}
fn context(
    app: &App,
    id: &str,
    dek: &[u8; 32],
) -> Result<(Value, fdstate::SnapshotPayload), Status> {
    let root = stage(app, id)?;
    for filename in ["context.old", "context.new"] {
        let Ok(bytes) = std::fs::read(root.join(filename)) else {
            continue;
        };
        if let Ok(plain) = fdstate::decrypt_bytes(&bytes, dek, b"FDCTX\0\0\0") {
            let ctx: Value = serde_json::from_slice(&plain).map_err(error)?;
            let epoch = ctx["entry"]["keyId"]
                .as_str()
                .ok_or_else(|| error("restore.context_invalid"))?;
            let master: [u8; 32] = hex::decode(
                ctx["masters"][epoch]
                    .as_str()
                    .ok_or_else(|| error("restore.context_invalid"))?,
            )
            .map_err(error)?
            .try_into()
            .map_err(|_| error("restore.context_invalid"))?;
            let payload = fdstate::decode(
                &std::fs::read(root.join("snapshot.fdstate")).map_err(error)?,
                &master,
            )
            .map_err(error)?;
            return Ok((ctx, payload));
        }
    }
    Err(error("restore.context_unavailable"))
}
pub async fn resume(app: &Arc<App>, id: &str) -> Result<(), Status> {
    let _guard = app.restore_lock.lock().await;
    let dek = app.secrets.lock().await.dek_bytes();
    let (ctx, payload) = context(app, id, &dek)?;
    SecretStore::validate_snapshot(&payload.secrets, &payload.dek)
        .map_err(|_| error("restore.secrets_invalid"))?;
    for name in ["verify", "install-secrets", "import-core"] {
        if app
            .journal
            .get(id)
            .map_err(error)?
            .steps
            .iter()
            .any(|s| s.name == name && s.state == "done")
        {
            continue;
        }
        app.journal.begin_step(id, name).map_err(error)?;
        match name {
            "verify" => {
                fd_appstate::fdconfig::validate(&payload.config).map_err(error)?;
            }
            "install-secrets" => {
                atomic_write(&app.config.dek, &payload.dek).map_err(error)?;
                atomic_write(&app.config.secrets, &payload.secrets).map_err(error)?;
                *app.secrets.lock().await =
                    SecretStore::open(&app.config.secrets, &app.config.dek).map_err(error)?;
                let key: fd_secrets::recovery_key::RecoveryKeyInfo =
                    serde_json::from_value(ctx["key"].clone()).map_err(error)?;
                let master: [u8; 32] = hex::decode(
                    ctx["masters"][&key.key_id]
                        .as_str()
                        .ok_or_else(|| error("restore.context_invalid"))?,
                )
                .map_err(error)?
                .try_into()
                .map_err(|_| error("restore.context_invalid"))?;
                *app.recovery.lock().await =
                    RecoveryKeyStore::install(&app.config.recovery_keys, &payload.dek, key, master)
                        .map_err(error)?;
            }
            "import-core" => {
                let present = present_pools(app)?;
                import_config(app.store.db(), &payload.config, id, &present).map_err(error)?;
                app.store
                    .db()
                    .with(|c| {
                        db::put_setting(c, "management.config", &payload.config["management"])?;
                        db::put_setting(
                            c,
                            "management.config_seq",
                            &json!(
                                payload.config["management"]["configSeq"]
                                    .as_u64()
                                    .unwrap_or(1)
                            ),
                        )?;
                        db::put_setting(c, "restore.active_operation", &json!(id))
                    })
                    .map_err(error)?;
            }
            _ => {}
        }
        app.journal
            .finish_step(id, name, Ok(json!({"durable":true})))
            .map_err(error)?;
    }
    let automatic_checked = app
        .store
        .db()
        .with(|c| db::get_setting(c, &format!("restore.auto_takeover_checked.{id}")))
        .map_err(error)?
        .is_some();
    if !automatic_checked && app.journal.get(id).map_err(error)?.params["local"] == true {
        app.store
            .db()
            .with(|c| {
                db::put_setting(
                    c,
                    &format!("restore.auto_takeover_checked.{id}"),
                    &json!(true),
                )
            })
            .map_err(error)?;
        if crate::runtime::configure(app).await.is_ok() {
            for location in runtime::locations(app)? {
                if location.role == Role::RecoveryReader
                    && location.owner_appliance_id == runtime::appliance_id(app)
                {
                    let request = pb::ConfirmTakeoverRequest {
                        connection_id: location.connection_id,
                        bucket: location.bucket,
                        prefix: location.prefix,
                        observed_generation: location.generation,
                        acknowledged_recent_writer: false,
                    };
                    let _ = takeover(app, &request, true).await;
                }
            }
        }
    }
    if crate::storage_runtime::enabled().is_ok() {
        let _storage = app.storage_lock.lock().await;
        if let Err(error) = crate::pool_recovery::reattach(app) {
            tracing::warn!(%error,"Some restored pools require manual mapping");
        }
        if let Err(error) = crate::storage_runtime::initialize(app) {
            tracing::warn!(%error,"Restored System Pool is unavailable");
        }
    }
    for row in crate::extension_runtime::rows(app, fd_state::ResourceKind::Extension)? {
        if let Some(hash) = row.spec["packageSha256"].as_str() {
            let file = stage(app, id)?.join(format!("{hash}.fdext"));
            if let Ok(bytes) = std::fs::read(&file)
                && crate::extension_runtime::restore_package(app, &row, &bytes)
                    .await
                    .is_ok()
            {
                let _ = std::fs::remove_file(file);
            }
        }
    }
    let mut report = build_report(
        &payload.config,
        &present_pools(app)?,
        id,
        ctx["entry"]["takenAt"].as_str().unwrap_or(""),
    );
    for item in report["items"].as_array_mut().into_iter().flatten() {
        if item["kind"] == "Extension"
            && let Some(extension) = payload.config["core"]["subsystems"]
                .as_array()
                .and_then(|a| a.iter().find(|e| e["id"] == item["id"]))
        {
            item["name"] = extension["subsystemId"].clone();
            if let Some(subsystem) = extension["subsystemId"]
                .as_str()
                .and_then(|name| app.subsystems.get(name).ok().flatten())
            {
                item["status"] = json!(subsystem.state);
                item["detail"] = json!(
                    "Package verified; runtime and data-binding availability determine extension status."
                );
                continue;
            }

            if let Some(hash) = extension["packageSha256"].as_str()
                && stage(app, id)?.join(format!("{hash}.fdext")).is_file()
            {
                item["status"] = json!("AwaitingInstallation");
                item["detail"] = json!(
                    "Verified package is available; its installation provider must complete before this extension can run."
                );
            }
        }
    }
    app.store
        .db()
        .with(|c| db::put_setting(c, &format!("restore.report.{id}"), &report))
        .map_err(error)?;
    app.journal
        .set_state(id, fd_journal::OperationState::AwaitingConfirmation, None)
        .map_err(error)?;
    app.events
        .emit(
            "operation.progressed",
            None,
            json!({"id":id,"step":"import-management"}),
        )
        .map_err(error)?;
    Ok(())
}
fn present_pools(app: &App) -> Result<Vec<String>, Status> {
    Ok(fd_appstate::detect::pool_roots(&app.config.pools_root)
        .map_err(error)?
        .iter()
        .filter_map(|(root, _)| {
            root.ancestors()
                .nth(2)?
                .file_name()?
                .to_str()
                .map(str::to_owned)
        })
        .collect())
}
pub async fn management(app: &App, id: &str) -> Result<Value, Status> {
    let operation = app
        .store
        .db()
        .with(|c| db::get_setting(c, "restore.imported_operation"))
        .map_err(error)?;
    if operation.as_ref().and_then(Value::as_str) == Some(id) {
        return app
            .store
            .db()
            .with(|c| db::get_setting(c, "restore.management"))
            .map_err(error)?
            .ok_or_else(|| error("restore.management_pending"));
    }
    let sources = app.restore_sources.lock().await;
    let source = sources
        .get(id)
        .ok_or_else(|| error("restore.source_not_found"))?;
    let snapshots = source.snapshots().await.map_err(error)?;
    let id = snapshots["snapshots"]
        .as_array()
        .and_then(|a| a.iter().find(|s| s["newestValid"] == true))
        .and_then(|s| s["snapshotId"].as_str())
        .ok_or_else(|| error("restore.no_valid_snapshot"))?;
    Ok(source.snapshot(id).await.map_err(error)?.config["management"].clone())
}
pub fn ack_management(app: &App) -> fd_appstate::Result<()> {
    let Some(id) = app
        .store
        .db()
        .with(|c| db::get_setting(c, "restore.active_operation"))?
        .and_then(|v| v.as_str().map(str::to_owned))
    else {
        return Ok(());
    };
    let restored = app
        .store
        .db()
        .with(|c| db::get_setting(c, "restore.management"))?;
    let current = app
        .store
        .db()
        .with(|c| db::get_setting(c, "management.config"))?;
    if restored.as_ref().map(|v| &v["administrators"])
        != current.as_ref().map(|v| &v["administrators"])
    {
        return Ok(());
    }
    app.journal
        .finish_step(&id, "import-management", Ok(json!({"imported":true})))
        .map_err(|_| fd_appstate::invalid("restore.journal_failed"))?;
    let report = app
        .store
        .db()
        .with(|c| db::get_setting(c, &format!("restore.report.{id}")))?
        .unwrap_or(Value::Null);
    let waiting = report["items"].as_array().is_some_and(|items| {
        items.iter().any(|i| {
            matches!(
                i["status"].as_str(),
                Some(
                    "PoolUnmapped"
                        | "PackageNeeded"
                        | "AwaitingInstallation"
                        | "AwaitingProvisioning"
                )
            )
        })
    });
    if waiting {
        app.journal
            .set_state(&id, fd_journal::OperationState::AwaitingConfirmation, None)
            .map_err(|_| fd_appstate::invalid("restore.journal_failed"))?;
    } else {
        for step in ["provision", "report"] {
            app.journal
                .finish_step(&id, step, Ok(json!({"completed":true})))
                .map_err(|_| fd_appstate::invalid("restore.journal_failed"))?;
        }
        let _ = app
            .events
            .emit("operation.completed", None, json!({"id":id}));
    }
    Ok(())
}
pub async fn snapshot_operation(app: &App, id: &str) {
    let _ = app.journal.begin_step(id, "snapshot");
    let result = runtime::snapshot(app, true).await.and_then(|covered| {
        if covered {
            Ok(json!({"allCopiesDurable":true}))
        } else {
            Err(error("state.copy_pending"))
        }
    });
    let success = result.is_ok();
    let _ = app
        .journal
        .finish_step(id, "snapshot", result.map_err(|e| e.message().to_owned()));
    let _ = app.events.emit(
        if success {
            "operation.completed"
        } else {
            "operation.failed"
        },
        None,
        json!({"id":id}),
    );
}
pub async fn map_pools(
    app: &Arc<App>,
    request: &pb::MapPoolsRequest,
) -> Result<fd_journal::Operation, Status> {
    crate::pool_recovery::map(app, request).await
}
pub async fn takeover(
    app: &App,
    request: &pb::ConfirmTakeoverRequest,
    automatic: bool,
) -> Result<fd_journal::Operation, Status> {
    let mut locations = runtime::locations(app)?;
    let location = locations
        .iter_mut()
        .find(|l| {
            l.connection_id == request.connection_id
                && l.bucket == request.bucket
                && l.prefix == request.prefix
        })
        .ok_or_else(|| error("location.not_found"))?;
    let (io, connector, safety) = runtime::object_io(
        app,
        &location.connection_id,
        &location.bucket,
        &location.prefix,
        false,
    )
    .await?;
    let id = runtime::appliance_id(app);
    if automatic {
        let current = io
            .read(ownership::OWNER_KEY)
            .await
            .map_err(error)?
            .ok_or_else(|| error("location.owner_missing"))?;
        let owner: ownership::Owner =
            serde_json::from_slice(&current.bytes).map_err(|_| error("location.owner_invalid"))?;
        if owner.appliance_id != id {
            return Err(error("location.owner_mismatch"));
        }
    }
    let owner = ownership::takeover(
        io.as_ref(),
        &id,
        "filedock",
        request.observed_generation,
        &safety,
        request.acknowledged_recent_writer,
        automatic,
    )
    .await
    .map_err(error)?;
    location.owner_appliance_id = owner.appliance_id;
    location.generation = owner.generation;
    location.last_write_at = owner.last_write_at;
    location.role = Role::Writer;
    location.served_by_connector = connector;
    location.claim_safety = safety;
    runtime::save_locations(app, &locations)?;
    let op = app
        .journal
        .start(
            "LocationTakeover",
            json!({"automatic":automatic,"generation":owner.generation,"reason":if automatic{"reinstall-restore"}else{"administrator-takeover"}}),
            None,
            if automatic { "system" } else { "administrator" },
            &["claim"],
        )
        .map_err(error)?;
    app.journal
        .finish_step(&op.id, "claim", Ok(json!({"generation":owner.generation})))
        .map_err(error)?;
    app.events
        .emit(
            "location.takeover",
            None,
            json!({"id":op.id,"automatic":automatic,"generation":owner.generation}),
        )
        .map_err(error)?;
    app.journal.get(&op.id).map_err(error)
}
pub fn report_proto(v: &Value) -> pb::RestoreReport {
    pb::RestoreReport {
        restore_operation_id: v["restoreOperationId"].as_str().unwrap_or("").into(),
        snapshot_taken_at: v["snapshotTakenAt"].as_str().and_then(timestamp),
        folders: v["folders"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|f| pb::RestoreFolderOutcome {
                folder_id: f["folderId"].as_str().unwrap_or("").into(),
                name: f["name"].as_str().unwrap_or("").into(),
                storage_mode: f["storageMode"].as_str().unwrap_or("").into(),
                data_importance: f["dataImportance"].as_str().unwrap_or("").into(),
                recovery_order: f["recoveryOrder"].as_u64().unwrap_or(6) as u32,
                outcome: match f["outcome"].as_str().unwrap_or("") {
                    "DataPresent" => 1,
                    "AwaitingDataRestore" => 2,
                    "Reconnected" => 3,
                    "ToRebuild" => 4,
                    "Lost" => 5,
                    "PoolUnmapped" => 6,
                    _ => 0,
                },
                detail: f["detail"].as_str().unwrap_or("").into(),
            })
            .collect(),
        items: v["items"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|i| pb::RestoreItemOutcome {
                kind: i["kind"].as_str().unwrap_or("").into(),
                id: i["id"].as_str().unwrap_or("").into(),
                name: i["name"].as_str().unwrap_or("").into(),
                status: i["status"].as_str().unwrap_or("").into(),
                detail: i["detail"].as_str().unwrap_or("").into(),
            })
            .collect(),
    }
}
pub async fn temporary_source(
    app: &App,
    target: &pb::ObjectStorageTarget,
    family: &str,
    definition: Option<&prost_types::Struct>,
    credentials: &HashMap<String, Vec<u8>>,
) -> Result<Arc<dyn ObjectIo>, Status> {
    if family == "azure-blob" {
        let definition = definition
            .map(crate::proto_conv::struct_to_value)
            .ok_or_else(|| Status::invalid_argument("restore.definition_required"))?;
        let account = definition["account"]
            .as_str()
            .ok_or_else(|| Status::invalid_argument("restore.account_required"))?;
        let key = credentials
            .get("key")
            .ok_or_else(|| Status::invalid_argument("restore.credentials_required"))?;
        if key.len() > 4096 {
            return Err(Status::invalid_argument("restore.credentials_invalid"));
        }
        let key = std::str::from_utf8(key)
            .map_err(|_| Status::invalid_argument("restore.credentials_invalid"))?;
        let name = format!("restore-{}", uuid::Uuid::new_v4());
        let adapter = fd_connectors::rclone::adapter::RcloneAdapter::with_socket(
            app.config.socket.parent().unwrap().join("rclone.sock"),
        );
        adapter
            .create_azure_recovery_remote(
                &name,
                json!({"account":account,"key":key,"no_check_container":"true"}),
            )
            .await
            .map_err(|_| error("restore.connector_configuration_failed"))?;
        if target.prefix.starts_with('/') || target.prefix.split('/').any(|p| p == ".." || p == ".")
        {
            let _ = adapter.delete_recovery_remote(&name).await;
            return Err(Status::invalid_argument("destination.prefix_invalid"));
        }
        return Ok(Arc::new(fd_connectors::object_io::RcloneObjects {
            adapter,
            remote: format!("{name}:{}", target.bucket),
            prefix: if target.prefix.is_empty() {
                String::new()
            } else {
                format!("{}/", target.prefix.trim_end_matches('/'))
            },
            cleanup_name: Some(name),
        }));
    }
    if family != "s3-compatible" {
        return Err(error("restore.provider_definition_required"));
    }

    let definition = definition
        .map(crate::proto_conv::struct_to_value)
        .ok_or_else(|| Status::invalid_argument("restore.definition_required"))?;
    let endpoint = definition["endpoint"]
        .as_str()
        .ok_or_else(|| Status::invalid_argument("restore.endpoint_required"))?;
    if !endpoint.starts_with("https://") && !endpoint.starts_with("http://") {
        return Err(Status::invalid_argument("restore.endpoint_invalid"));
    }
    let id = format!("restore-{}", uuid::Uuid::new_v4());
    let root = app.config.secrets_run.join("fd-s3").join(&id);
    std::fs::create_dir_all(&root).map_err(error)?;
    let mut files = HashMap::new();
    for name in ["accessKey", "secretKey"] {
        let value = credentials
            .get(name)
            .ok_or_else(|| Status::invalid_argument("restore.credentials_required"))?;
        if value.len() > 4096 {
            return Err(Status::invalid_argument("restore.credentials_invalid"));
        }
        let file = root.join(name);
        atomic_write(&file, value).map_err(error)?;
        files.insert(format!("{id}/{name}"), file.to_string_lossy().to_string());
    }
    let channel = crate::runtime::channel(crate::runtime::s3_socket(app)).await?;
    let config = json!({"id":id,"endpoint":endpoint,"region":definition["region"].as_str().unwrap_or("us-east-1"),"bucket":target.bucket,"virtual_hosted":definition["addressingStyle"]=="Virtual","access_key_ref":format!("{id}/accessKey"),"secret_key_ref":format!("{id}/secretKey"),"checksums":false,"storage_class":definition["storageClass"],"kms_key_id":definition["serverSideEncryption"]["kms"]["keyId"].as_str().or_else(||definition["serverSideEncryption"]["keyId"].as_str()),"bandwidth_limit":definition["bandwidthLimit"]});
    app.temporary_s3
        .lock()
        .await
        .insert(id.clone(), (config, files));
    if crate::runtime::configure(app).await.is_err() {
        app.temporary_s3.lock().await.remove(&id);
        let _ = std::fs::remove_dir_all(&root);
        return Err(error("restore.connector_configuration_failed"));
    }
    if target.prefix.starts_with('/') || target.prefix.split('/').any(|p| p == ".." || p == ".") {
        return Err(Status::invalid_argument("destination.prefix_invalid"));
    }
    Ok(Arc::new(fd_connectors::object_io::S3Objects {
        channel,
        connection_id: id,
        cleanup_directory: Some(root),
        prefix: if target.prefix.is_empty() {
            String::new()
        } else {
            format!("{}/", target.prefix.trim_end_matches('/'))
        },
    }))
}

/// Recover the key-store replacement if a crash occurred after writing the restored DEK.
pub fn recover_key_on_start(
    config: &crate::config::Config,
    db: &fd_state::Db,
    dek: &[u8; 32],
) -> Result<Option<RecoveryKeyStore>, Box<dyn std::error::Error>> {
    let journal = fd_journal::Journal::new(db.clone());
    for op in journal
        .unfinished()?
        .into_iter()
        .filter(|o| o.kind == "Restore")
    {
        let root = config.state_spool.join("restore").join(&op.id);
        for name in ["context.old", "context.new"] {
            let Ok(raw) = std::fs::read(root.join(name)) else {
                continue;
            };
            let Ok(plain) = fdstate::decrypt_bytes(&raw, dek, b"FDCTX\0\0\0") else {
                continue;
            };
            let ctx: Value = serde_json::from_slice(&plain)?;
            let key: fd_secrets::recovery_key::RecoveryKeyInfo =
                serde_json::from_value(ctx["key"].clone())?;
            let master: [u8; 32] = hex::decode(
                ctx["masters"][&key.key_id]
                    .as_str()
                    .ok_or("restore context invalid")?,
            )?
            .try_into()
            .map_err(|_| "restore context invalid")?;
            return Ok(Some(RecoveryKeyStore::install(
                &config.recovery_keys,
                dek,
                key,
                master,
            )?));
        }
    }
    Ok(None)
}

pub async fn recorded_remote(
    app: &App,
    source: &fd_appstate::restore::sources::Source,
    recovery_key: &str,
) -> Result<Option<fd_appstate::restore::sources::Source>, Status> {
    let records = source.snapshots().await.map_err(error)?;
    let Some(id) = records["snapshots"]
        .as_array()
        .and_then(|a| a.iter().find(|s| s["newestValid"] == true))
        .and_then(|s| s["snapshotId"].as_str())
    else {
        return Ok(None);
    };
    let payload = source.snapshot(id).await.map_err(error)?;
    let target = &payload.config["core"]["stateDestination"]["target"]["objectStorage"];
    let Some(connection_id) = target["connectionId"].as_str() else {
        return Ok(None);
    };
    let Some(connection) = payload.config["core"]["connections"]
        .as_array()
        .and_then(|a| a.iter().find(|c| c["id"] == connection_id))
    else {
        return Ok(None);
    };
    let definition = &connection["definition"];
    let mut credentials = HashMap::new();
    for (field, name) in [("accessKeyRef", "accessKey"), ("secretKeyRef", "secretKey")] {
        if let Some(reference) = definition[field].as_str() {
            let value = SecretStore::read_snapshot(&payload.secrets, &payload.dek, reference)
                .map_err(|_| error("restore.credentials_unavailable"))?;
            credentials.insert(name.into(), value.to_vec());
        }
    }
    let target = pb::ObjectStorageTarget {
        connection_id: connection_id.into(),
        bucket: target["bucket"].as_str().unwrap_or("").into(),
        prefix: target["prefix"].as_str().unwrap_or("").into(),
    };
    let family = connection["providerFamily"].as_str().unwrap_or("");
    let definition = crate::proto_conv::value_to_struct(definition);
    let io = temporary_source(app, &target, family, Some(&definition), &credentials).await?;
    let remote = fd_appstate::restore::sources::Source::open(
        fd_appstate::restore::sources::ReadBackend::Remote(io),
        recovery_key,
        true,
    )
    .await
    .map_err(error)?;
    if remote.index.appliance_id != source.index.appliance_id {
        return Err(error("restore.identity_mismatch"));
    }
    Ok(Some(remote))
}
