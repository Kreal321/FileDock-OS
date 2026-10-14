//! Authenticated configuration snapshots and key-authorized recovery sources.
use super::{App, control::operation_to_proto, events::timestamp, interceptors::check_metadata};
use crate::state_runtime::{self as runtime, error};
use fd_api::core::v1::{self as pb, recovery_server::Recovery};
use fd_appstate::{
    engine::Engine,
    format::fdbackup,
    restore::sources::{ReadBackend, Source},
};
use fd_connectors::ownership::{self, OwnedLocation, Role};
use fd_state::{
    Change, ResourceKind, ResourceRef, db,
    resources::{ApplianceStateDestinationSpec, StateDestinationTarget},
};
use serde_json::{Value, json};
use std::{pin::Pin, sync::Arc};
use tokio_stream::Stream;
use tonic::{Request, Response, Status, Streaming};
pub struct RecoverySvc {
    pub app: Arc<App>,
}
impl RecoverySvc {
    fn metadata<T>(&self, r: &Request<T>, mutating: bool) -> Result<(), Status> {
        check_metadata(r.metadata(), self.app.config.interface_version, mutating)
    }
    async fn fresh(&self) -> Result<(), Status> {
        if self.app.recovery.lock().await.current().is_some() {
            return Err(error("restore.target_not_fresh"));
        }
        let imported = self
            .app
            .store
            .db()
            .with(|c| db::get_setting(c, "restore.imported_operation"))
            .map_err(error)?;
        if imported.is_some() {
            return Err(error("restore.target_not_fresh"));
        }
        for kind in ResourceKind::ALL {
            if !matches!(kind, ResourceKind::Settings | ResourceKind::ConnectorRoutes)
                && !self
                    .app
                    .store
                    .list(kind, None, 1)
                    .map_err(error)?
                    .is_empty()
            {
                return Err(error("restore.target_not_fresh"));
            }
        }
        Ok(())
    }
    async fn remember(&self, source: Source) -> Result<pb::RestoreSource, Status> {
        let response = pb::RestoreSource {
            id: source.id.clone(),
            owner_appliance_id: source.index.appliance_id.clone(),
            owner_appliance_name: source.index.appliance_name.clone(),
            owner_last_write_at: None,
            role_recovery_reader: true,
            local: source.local,
            recorded_remote_source_id: source.recorded_remote_source_id.clone().unwrap_or_default(),
        };
        let mut sources = self.app.restore_sources.lock().await;
        sources.retain(|_, s| s.valid());
        if sources.len() >= 12 {
            return Err(Status::resource_exhausted("restore.too_many_sources"));
        }
        sources.insert(source.id.clone(), source);
        Ok(response)
    }
}
#[tonic::async_trait]
impl Recovery for RecoverySvc {
    async fn generate_recovery_key(
        &self,
        r: Request<pb::GenerateRecoveryKeyRequest>,
    ) -> Result<Response<pb::GeneratedRecoveryKey>, Status> {
        self.metadata(&r, true)?;
        let generated = self
            .app
            .recovery
            .lock()
            .await
            .generate(r.into_inner().regenerate)
            .map_err(error)?;
        Ok(Response::new(pb::GeneratedRecoveryKey {
            key_id: generated.key_id,
            recovery_key: generated.recovery_key,
        }))
    }
    async fn confirm_recovery_key(
        &self,
        r: Request<pb::ConfirmRecoveryKeyRequest>,
    ) -> Result<Response<pb::Empty>, Status> {
        self.metadata(&r, true)?;
        self.app
            .recovery
            .lock()
            .await
            .confirm(&r.into_inner().key_id)
            .map_err(error)?;
        self.app
            .store
            .db()
            .with(|c| {
                db::put_setting(
                    c,
                    "config.changed_at",
                    &json!(chrono::Utc::now().to_rfc3339()),
                )
            })
            .map_err(error)?;
        Ok(Response::new(pb::Empty {}))
    }
    async fn put_management_config(
        &self,
        r: Request<pb::ManagementConfig>,
    ) -> Result<Response<pb::Empty>, Status> {
        self.metadata(&r, true)?;
        let document = r.into_inner();
        if document.schema_version != "fdconfig.management/1"
            || document.document.len() > 4 * 1024 * 1024
        {
            return Err(Status::invalid_argument("management.config_invalid"));
        }
        let value: Value = serde_json::from_slice(&document.document)
            .map_err(|_| Status::invalid_argument("management.config_invalid"))?;
        if !value.is_object() {
            return Err(Status::invalid_argument("management.config_invalid"));
        }
        self.app
            .store
            .db()
            .with(|c| {
                let tx = c.transaction()?;
                let previous = db::get_setting(&tx, "management.config_seq")?
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                if document.config_seq < previous {
                    return Err(fd_state::DbError::from(rusqlite_error()));
                }
                if document.config_seq != previous
                    || db::get_setting(&tx, "management.config")?.as_ref() != Some(&value)
                {
                    let changed_at = document
                        .exported_at
                        .as_ref()
                        .and_then(|t| {
                            chrono::DateTime::from_timestamp(t.seconds, t.nanos.try_into().ok()?)
                        })
                        .unwrap_or_else(chrono::Utc::now)
                        .to_rfc3339();
                    db::put_setting(&tx, "management.changed_at", &json!(changed_at))?;
                }
                db::put_setting(&tx, "management.config", &value)?;
                db::put_setting(&tx, "management.config_seq", &json!(document.config_seq))?;
                tx.commit()?;
                Ok::<_, fd_state::DbError>(())
            })
            .map_err(error)?;
        crate::restore_runtime::ack_management(&self.app).map_err(error)?;
        Ok(Response::new(pb::Empty {}))
    }
    async fn get_state_protection(
        &self,
        r: Request<pb::Empty>,
    ) -> Result<Response<pb::StateProtection>, Status> {
        self.metadata(&r, false)?;
        let mut v = Engine::protection(self.app.store.db()).map_err(error)?;
        if self.app.recovery.lock().await.snapshot_key().is_err() {
            v["warning"] = json!(true);
        }
        let copies = v["copies"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| pb::StateCopy {
                kind: match c["kind"].as_str().unwrap_or("") {
                    "LocalPool" => 1,
                    "LocalSystemDisk" => 2,
                    "RemoteObjectStorage" => 3,
                    "RemoteRemovableDisk" => 4,
                    _ => 0,
                },
                pool_id: c["poolId"].as_str().unwrap_or("").into(),
                description: c["description"].as_str().unwrap_or("").into(),
                last_snapshot_at: c["lastSnapshotAt"].as_str().and_then(timestamp),
                stale: c["stale"].as_bool().unwrap_or(true),
                last_error: c["lastError"].as_str().unwrap_or("").into(),
            })
            .collect();
        Ok(Response::new(pb::StateProtection {
            remote_destination_registered: v["remoteDestinationRegistered"] == true,
            latest_change_at: v["latestChangeAt"].as_str().and_then(timestamp),
            copies,
            reinstall_covered: v["reinstallCovered"] == true,
            machine_loss_covered: v["machineLossCovered"] == true,
            warning: v["warning"] == true,
        }))
    }
    async fn set_state_destination(
        &self,
        r: Request<pb::SetStateDestinationRequest>,
    ) -> Result<Response<pb::Operation>, Status> {
        self.metadata(&r, true)?;
        let actor = r
            .metadata()
            .get("fd-actor")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("management")
            .to_owned();
        let req = r.into_inner();
        let target = match req
            .target
            .ok_or_else(|| Status::invalid_argument("destination.required"))?
        {
            pb::set_state_destination_request::Target::RemovableDiskPath(path) => {
                if !runtime::mounted_removable(&path) {
                    return Err(error("destination.disk_not_mounted_or_not_separate"));
                }
                StateDestinationTarget::RemovableDisk { path }
            }
            pb::set_state_destination_request::Target::ObjectStorage(target) => {
                let prior = runtime::locations(&self.app)?.into_iter().find(|l| {
                    l.connection_id == target.connection_id
                        && l.bucket == target.bucket
                        && l.prefix == target.prefix
                });
                if prior.as_ref().is_some_and(|l| l.role != Role::Writer) {
                    return Err(error("location.takeover_required"));
                }
                crate::runtime::configure(&self.app).await?;
                let (io, connector, safety) = runtime::object_io(
                    &self.app,
                    &target.connection_id,
                    &target.bucket,
                    &target.prefix,
                    false,
                )
                .await?;
                let third = !matches!(connector.as_str(), "fd-s3" | "rclone");
                if third && !req.acknowledged_third_party_connector {
                    return Err(error("location.third_party_ack_required"));
                }
                let id = runtime::appliance_id(&self.app);
                let name = self
                    .app
                    .store
                    .get(&ResourceRef::singleton(ResourceKind::Settings))
                    .map_err(error)?
                    .map(|r| {
                        r.spec["applianceName"]
                            .as_str()
                            .unwrap_or("filedock")
                            .to_owned()
                    })
                    .unwrap_or_else(|| "filedock".into());
                let owner = ownership::claim(
                    io.as_ref(),
                    &id,
                    &name,
                    safety.clone(),
                    req.acknowledged_basic_claim_safety,
                )
                .await
                .map_err(error)?;
                if prior
                    .as_ref()
                    .is_some_and(|l| l.generation != owner.generation)
                {
                    return Err(error("location.ownership_lost"));
                }
                let mut locations = runtime::locations(&self.app)?;
                locations.retain(|l| {
                    !(l.connection_id == target.connection_id
                        && l.bucket == target.bucket
                        && l.prefix == target.prefix)
                });
                locations.push(OwnedLocation {
                    id: uuid::Uuid::new_v4().to_string(),
                    connection_id: target.connection_id.clone(),
                    bucket: target.bucket.clone(),
                    prefix: target.prefix.clone(),
                    purpose: "ApplianceState".into(),
                    owner_appliance_id: owner.appliance_id,
                    generation: owner.generation,
                    last_write_at: owner.last_write_at,
                    role: Role::Writer,
                    served_by_connector: connector,
                    served_by_third_party: third,
                    claim_safety: safety,
                });
                runtime::save_locations(&self.app, &locations)?;
                StateDestinationTarget::ObjectStorage {
                    connection_id: target.connection_id,
                    bucket: target.bucket,
                    prefix: target.prefix,
                }
            }
        };
        let reference = ResourceRef::singleton(ResourceKind::ApplianceStateDestination);
        let existing = self.app.store.get(&reference).map_err(error)?;
        self.app
            .store
            .apply(
                fd_state::ChangeOrigin::Management {
                    actor: actor.clone(),
                    request_id: uuid::Uuid::new_v4().to_string(),
                },
                &[Change::Upsert {
                    resource: reference,
                    expected_generation: existing.map(|r| r.generation).unwrap_or(0),
                    spec: serde_json::to_value(ApplianceStateDestinationSpec { target })
                        .map_err(error)?,
                }],
            )
            .map_err(error)?;
        let operation = self
            .app
            .journal
            .start("StateSnapshot", json!({}), None, &actor, &["snapshot"])
            .map_err(error)?;
        let app = self.app.clone();
        let id = operation.id.clone();
        tokio::spawn(async move {
            crate::restore_runtime::snapshot_operation(&app, &id).await;
        });
        Ok(Response::new(operation_to_proto(&operation)))
    }
    type ExportBackupFileStream =
        Pin<Box<dyn Stream<Item = Result<pb::BundleChunk, Status>> + Send>>;
    async fn export_backup_file(
        &self,
        r: Request<pb::Empty>,
    ) -> Result<Response<Self::ExportBackupFileStream>, Status> {
        self.metadata(&r, false)?;
        runtime::snapshot(&self.app, true).await?;
        let input = runtime::input(&self.app).await?;
        let bytes = self
            .app
            .state_engine
            .lock()
            .await
            .export(&input)
            .map_err(error)?;
        let chunks = bytes
            .chunks(65536)
            .map(|data| {
                Ok(pb::BundleChunk {
                    data: data.to_vec(),
                })
            })
            .collect::<Vec<_>>();
        Ok(Response::new(Box::pin(tokio_stream::iter(chunks))))
    }
    async fn detect_local_state(
        &self,
        r: Request<pb::Empty>,
    ) -> Result<Response<pb::DetectLocalStateResponse>, Status> {
        self.metadata(&r, false)?;
        self.fresh().await?;
        let mut roots = vec![(self.app.config.state_root.clone(), "system disk".into())];
        roots.extend(fd_appstate::detect::pool_roots(&self.app.config.pools_root).map_err(error)?);
        if cfg!(target_os = "linux") && std::env::var("FD_RUNTIME_ENABLED").as_deref() != Ok("0") {
            let output = self
                .app
                .config
                .socket
                .parent()
                .unwrap()
                .join(format!("detect-{}", uuid::Uuid::new_v4()));
            let child = tokio::process::Command::new("unshare")
                .args(["--mount", "--propagation", "private"])
                .arg(std::env::current_exe().map_err(error)?)
                .arg("detect-state-private")
                .env("FD_DETECT_OUTPUT", &output)
                .output()
                .await
                .map_err(error)?;
            if child.status.success()
                && let Ok(discovered) =
                    serde_json::from_slice::<Vec<(std::path::PathBuf, String)>>(&child.stdout)
            {
                roots.extend(discovered);
            }
        }
        let detected = fd_appstate::detect::scan(&roots).map_err(error)?;
        let response = pb::DetectLocalStateResponse {
            appliances: detected
                .iter()
                .map(|d| pb::DetectedAppliance {
                    appliance_id: d.appliance_id.clone(),
                    appliance_name: d.appliance_name.clone(),
                    newest_snapshot_at: d.newest_snapshot_at.as_deref().and_then(timestamp),
                    found_on: d.found_on.clone(),
                    local_source_token: d.local_source_token.clone(),
                })
                .collect(),
        };
        *self.app.detected.lock().await = detected;
        Ok(Response::new(response))
    }
    async fn open_local_restore_source(
        &self,
        r: Request<pb::OpenLocalRestoreSourceRequest>,
    ) -> Result<Response<pb::RestoreSource>, Status> {
        self.metadata(&r, true)?;
        self.fresh().await?;
        let req = r.into_inner();
        let root = self
            .app
            .detected
            .lock()
            .await
            .iter()
            .find(|d| d.local_source_token == req.local_source_token)
            .map(|d| d.root.clone())
            .ok_or_else(|| error("restore.source_not_found"))?;
        crate::restore_runtime::throttle(&self.app)?;
        let mut source = Source::open(ReadBackend::Local(root), &req.recovery_key, true)
            .await
            .map_err(|e| {
                error(if e.to_string().contains("backup_file.too_new") {
                    "backup_file.too_new"
                } else {
                    "recovery.key_invalid"
                })
            })?;
        let roots = self
            .app
            .detected
            .lock()
            .await
            .iter()
            .filter(|d| d.appliance_id == source.index.appliance_id)
            .map(|d| d.root.clone())
            .collect::<Vec<_>>();
        for root in roots.into_iter().take(8) {
            if let Ok(alternate) =
                Source::open(ReadBackend::Local(root), &req.recovery_key, true).await
            {
                let response = self.remember(alternate).await?;
                source.alternate_source_ids.push(response.id);
            }
        }
        if let Ok(Some(remote)) =
            crate::restore_runtime::recorded_remote(&self.app, &source, &req.recovery_key).await
        {
            let response = self.remember(remote).await?;
            source.recorded_remote_source_id = Some(response.id.clone());
            source.alternate_source_ids.push(response.id);
        }
        Ok(Response::new(self.remember(source).await?))
    }
    async fn open_restore_source(
        &self,
        r: Request<pb::OpenRestoreSourceRequest>,
    ) -> Result<Response<pb::RestoreSource>, Status> {
        self.metadata(&r, true)?;
        self.fresh().await?;
        let req = r.into_inner();
        crate::restore_runtime::throttle(&self.app)?;
        let backend = match req
            .source
            .ok_or_else(|| Status::invalid_argument("restore.source_required"))?
        {
            pb::open_restore_source_request::Source::RemovableDiskPath(path) => {
                if !runtime::mounted_removable(&path) {
                    return Err(error("destination.disk_not_mounted_or_not_separate"));
                }
                ReadBackend::Local(std::path::PathBuf::from(path).join("filedock/appliance-state"))
            }
            pb::open_restore_source_request::Source::ObjectStorage(target) => {
                let io = crate::restore_runtime::temporary_source(
                    &self.app,
                    &target,
                    &req.provider_family,
                    req.connection_definition.as_ref(),
                    &req.connection_credentials,
                )
                .await?;
                ReadBackend::Remote(io)
            }
        };
        let source = Source::open(backend, &req.recovery_key, false)
            .await
            .map_err(|e| {
                error(if e.to_string().contains("backup_file.too_new") {
                    "backup_file.too_new"
                } else {
                    "recovery.key_invalid"
                })
            })?;
        Ok(Response::new(self.remember(source).await?))
    }
    async fn upload_restore_file(
        &self,
        r: Request<Streaming<pb::RestoreFileChunk>>,
    ) -> Result<Response<pb::RestoreSource>, Status> {
        self.metadata(&r, true)?;
        self.fresh().await?;
        crate::restore_runtime::throttle(&self.app)?;
        let mut stream = r.into_inner();
        let mut bytes = Vec::new();
        let mut key = String::new();
        while let Some(mut chunk) = stream.message().await? {
            if bytes.is_empty() {
                key = std::mem::take(&mut chunk.recovery_key);
            } else if !chunk.recovery_key.is_empty() {
                return Err(Status::invalid_argument("restore.key_first_chunk_only"));
            }
            if bytes.len() + chunk.data.len() > fdbackup::MAX_BACKUP_BYTES {
                return Err(Status::resource_exhausted("restore.file_too_large"));
            }
            bytes.extend(chunk.data);
        }
        let files = fdbackup::decode(&bytes).map_err(error)?;
        let source = Source::open(ReadBackend::Files(Arc::new(files)), &key, false)
            .await
            .map_err(|e| {
                error(if e.to_string().contains("backup_file.too_new") {
                    "backup_file.too_new"
                } else {
                    "recovery.key_invalid"
                })
            })?;
        Ok(Response::new(self.remember(source).await?))
    }
    async fn list_restore_snapshots(
        &self,
        r: Request<pb::RestoreSourceRef>,
    ) -> Result<Response<pb::ListRestoreSnapshotsResponse>, Status> {
        self.metadata(&r, false)?;
        let id = r.into_inner().id;
        let sources = self.app.restore_sources.lock().await;
        let source = sources
            .get(&id)
            .ok_or_else(|| error("restore.source_not_found"))?;
        let mut records = source.snapshots().await.map_err(error)?;
        for id in &source.alternate_source_ids {
            if let Some(alternate) = sources.get(id)
                && let Ok(mut other) = alternate.snapshots().await
            {
                records["snapshots"]
                    .as_array_mut()
                    .unwrap()
                    .append(other["snapshots"].as_array_mut().unwrap());
            }
        }
        let entries = records["snapshots"].as_array_mut().unwrap();
        entries.sort_by(|a, b| {
            b["takenAt"]
                .as_str()
                .cmp(&a["takenAt"].as_str())
                .then_with(|| b["integrityOk"].as_bool().cmp(&a["integrityOk"].as_bool()))
        });
        let mut seen = std::collections::BTreeSet::new();
        entries.retain(|e| seen.insert(e["snapshotId"].as_str().unwrap_or("").to_owned()));
        for e in entries.iter_mut() {
            e["newestValid"] = json!(false);
        }
        if let Some(e) = entries.iter_mut().find(|e| e["integrityOk"] == true) {
            e["newestValid"] = json!(true);
        }

        let snapshots = records["snapshots"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| pb::RestoreSnapshot {
                snapshot_id: s["snapshotId"].as_str().unwrap_or("").into(),
                taken_at: s["takenAt"].as_str().and_then(timestamp),
                appliance_name: source.index.appliance_name.clone(),
                integrity_ok: s["integrityOk"] == true,
                key_id: s["keyId"].as_str().unwrap_or("").into(),
                restore_source_id: s["restoreSourceId"].as_str().unwrap_or(&id).into(),
                newest_valid: s["newestValid"] == true,
            })
            .collect();
        Ok(Response::new(pb::ListRestoreSnapshotsResponse {
            snapshots,
        }))
    }
    async fn begin_restore(
        &self,
        r: Request<pb::BeginRestoreRequest>,
    ) -> Result<Response<pb::Operation>, Status> {
        self.metadata(&r, true)?;
        self.fresh().await?;
        let req = r.into_inner();
        let op = crate::restore_runtime::begin(&self.app, &req.restore_source_id, &req.snapshot_id)
            .await?;
        Ok(Response::new(operation_to_proto(&op)))
    }
    async fn map_pools(
        &self,
        r: Request<pb::MapPoolsRequest>,
    ) -> Result<Response<pb::Operation>, Status> {
        self.metadata(&r, true)?;
        let req = r.into_inner();
        crate::restore_runtime::map_pools(&self.app, &req)
            .await
            .map(|op| Response::new(operation_to_proto(&op)))
    }
    async fn get_management_config_for_restore(
        &self,
        r: Request<pb::RestoreSourceRef>,
    ) -> Result<Response<pb::ManagementConfig>, Status> {
        self.metadata(&r, false)?;
        let id = r.into_inner().id;
        let document = crate::restore_runtime::management(&self.app, &id).await?;
        Ok(Response::new(pb::ManagementConfig {
            schema_version: "fdconfig.management/1".into(),
            document: serde_json::to_vec(&document).map_err(error)?,
            exported_at: None,
            config_seq: document["configSeq"].as_u64().unwrap_or(1),
        }))
    }
    async fn get_restore_report(
        &self,
        r: Request<pb::OperationRef>,
    ) -> Result<Response<pb::RestoreReport>, Status> {
        self.metadata(&r, false)?;
        let id = r.into_inner().id;
        let v = self
            .app
            .store
            .db()
            .with(|c| db::get_setting(c, &format!("restore.report.{id}")))
            .map_err(error)?
            .ok_or_else(|| Status::not_found("restore.report_not_found"))?;
        Ok(Response::new(crate::restore_runtime::report_proto(&v)))
    }
    async fn list_owned_locations(
        &self,
        r: Request<pb::Empty>,
    ) -> Result<Response<pb::ListOwnedLocationsResponse>, Status> {
        self.metadata(&r, false)?;
        Ok(Response::new(pb::ListOwnedLocationsResponse {
            locations: runtime::inspect_locations(&self.app)
                .await?
                .iter()
                .map(|l| pb::OwnedLocation {
                    connection_id: l.connection_id.clone(),
                    bucket: l.bucket.clone(),
                    prefix: l.prefix.clone(),
                    purpose: l.purpose.clone(),
                    owner_appliance_id: l.owner_appliance_id.clone(),
                    generation: l.generation,
                    last_write_at: timestamp(&l.last_write_at),
                    role: match l.role {
                        Role::Writer => 1,
                        Role::RecoveryReader => 2,
                        Role::OwnershipLost => 3,
                    },
                    served_by_connector: l.served_by_connector.clone(),
                    served_by_third_party: l.served_by_third_party,
                    claim_safety: match l.claim_safety {
                        ownership::ClaimSafety::Conditional => 1,
                        ownership::ClaimSafety::Basic => 2,
                    },
                })
                .collect(),
        }))
    }
    async fn confirm_takeover(
        &self,
        r: Request<pb::ConfirmTakeoverRequest>,
    ) -> Result<Response<pb::Operation>, Status> {
        self.metadata(&r, true)?;
        let req = r.into_inner();
        let op = crate::restore_runtime::takeover(&self.app, &req, false).await?;
        Ok(Response::new(operation_to_proto(&op)))
    }
}
fn rusqlite_error() -> rusqlite::Error {
    rusqlite::Error::InvalidQuery
}
