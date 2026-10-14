//! Management extensions RPCs: validation, explicit approval, and journaled lifecycle.
use super::{
    App,
    interceptors::{actor_of, check_metadata},
};
use crate::{extension_runtime as runtime, proto_conv};
use fd_api::core::v1::{self as pb, extensions_server::Extensions};
use fd_state::{ResourceKind, ResourceRef};
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Arc};
use tonic::{Request, Response, Status};
pub struct ExtensionsSvc {
    pub app: Arc<App>,
}
fn err(e: impl std::fmt::Display) -> Status {
    Status::failed_precondition(e.to_string())
}
impl ExtensionsSvc {
    fn metadata<T>(&self, r: &Request<T>, write: bool) -> Result<(), Status> {
        check_metadata(r.metadata(), self.app.config.interface_version, write)
    }
    async fn start(
        &self,
        kind: &str,
        spec: Value,
        previous: Option<fd_state::StoredResource>,
        bytes: &[u8],
        actor: &str,
    ) -> Result<pb::Operation, Status> {
        let hash = spec["packageSha256"]
            .as_str()
            .ok_or_else(|| err("extension.hash_missing"))?;
        let dek = self.app.secrets.lock().await.dek_bytes();
        let stored = fd_appstate::packages::store(&self.app.config.extension_root, bytes, &dek)
            .map_err(err)?;
        if hash != stored {
            return Err(err("extension.package_integrity_failed"));
        }
        let id = spec["subsystemId"].as_str().unwrap();
        let old_manifest = self
            .app
            .subsystems
            .get(id)
            .map_err(err)?
            .map(|r| r.manifest)
            .unwrap_or(Value::Null);
        let reference = previous
            .as_ref()
            .map(|r| r.resource.clone())
            .unwrap_or_else(|| {
                ResourceRef::new(ResourceKind::Extension, uuid::Uuid::new_v4().to_string())
            });
        let op=self.app.journal.start(kind,json!({"spec":spec,"previous":previous.map(|r|r.spec).unwrap_or(Value::Null),"previousManifest":old_manifest}),Some(reference),actor,&["stage","activate","commit"]).map_err(err)?;
        let app = self.app.clone();
        let op_id = op.id.clone();
        tokio::spawn(async move {
            let _ = runtime::resume(&app, &op_id).await;
        });
        Ok(super::control::operation_to_proto(&op))
    }
}
#[tonic::async_trait]
impl Extensions for ExtensionsSvc {
    async fn inspect_package(
        &self,
        r: Request<pb::InspectPackageRequest>,
    ) -> Result<Response<pb::PackageInspection>, Status> {
        self.metadata(&r, false)?;
        match runtime::inspect(&self.app,&r.into_inner().package_path){Ok((package,_))=>Ok(Response::new(pb::PackageInspection{valid:true,issues:vec![],manifest:Some(proto_conv::value_to_struct(&fd_secrets::redact::redact_value(package.manifest.raw.clone()))),requested_permissions:package.permissions(),signed:package.signed,signer_key_id:package.signer_key_id.clone(),data_paths:package.manifest.raw["data"].as_array().into_iter().flatten().map(|d|pb::DeclaredDataPath{name:d["name"].as_str().unwrap_or("").into(),description:d["description"].as_str().unwrap_or("").into(),mount_path:d["mountPath"].as_str().unwrap_or("").into(),suggested_importance:d["suggestedImportance"].as_str().unwrap_or("Important").into()}).collect()})),Err(error)=>Ok(Response::new(pb::PackageInspection{valid:false,issues:vec![pb::ValidationIssue{code:error.message().into(),field:"package".into(),message:"Package validation failed. Check its integrity, signature, manifest, runtime and compatibility.".into(),severity:1,conflicting:None}],..Default::default()}))}
    }
    async fn install_extension(
        &self,
        r: Request<pb::InstallExtensionRequest>,
    ) -> Result<Response<pb::Operation>, Status> {
        self.metadata(&r, true)?;
        let actor = actor_of(r.metadata());
        let body = r.into_inner();
        let (package, bytes) = runtime::inspect(&self.app, &body.package_path)?;
        if self
            .app
            .subsystems
            .get(&package.manifest.id)
            .map_err(err)?
            .is_some_and(|r| r.class != "Extension")
        {
            return Err(err("extension.identity_reserved"));
        }
        let previous = runtime::find(&self.app, &package.manifest.id)?;
        let spec = if let Some(previous) = &previous {
            if previous.spec["packageSha256"] != package.hash
                || previous.spec["version"] != package.manifest.version
            {
                return Err(err("extension.recorded_package_required"));
            }
            if self
                .app
                .subsystems
                .get(&package.manifest.id)
                .map_err(err)?
                .is_some_and(|r| r.state == "Running")
            {
                return Err(err("extension.already_installed"));
            }
            previous.spec.clone()
        } else {
            let bindings = body.data_bindings.into_iter().collect::<BTreeMap<_, _>>();
            fd_subsystems::lifecycle::approve(
                &package,
                &body.approved_permissions,
                body.acknowledged_unsigned,
                &bindings,
                &runtime::folder_map(&self.app)?,
                body.acknowledged_unprotected_data,
            )
            .map_err(err)?;
            json!({"subsystemId":package.manifest.id,"version":package.manifest.version,"packageSha256":package.hash,"config":{},"approvedPermissions":body.approved_permissions,"signature":if package.signed{json!({"signed":{"keyId":package.signer_key_id}})}else{json!({"unsigned":{"acknowledged":true}})},"dataBindings":bindings})
        };
        Ok(Response::new(
            self.start("InstallExtension", spec, previous, &bytes, &actor)
                .await?,
        ))
    }
    async fn upgrade_extension(
        &self,
        r: Request<pb::UpgradeExtensionRequest>,
    ) -> Result<Response<pb::Operation>, Status> {
        self.metadata(&r, true)?;
        let actor = actor_of(r.metadata());
        let body = r.into_inner();
        let previous = runtime::find(&self.app, &body.subsystem_id)?
            .ok_or_else(|| Status::not_found("extension.not_found"))?;
        let (package, bytes) = runtime::inspect(&self.app, &body.package_path)?;
        if package.manifest.id != body.subsystem_id
            || package.manifest.version == previous.spec["version"].as_str().unwrap_or("")
        {
            return Err(err("extension.upgrade_identity_or_version_invalid"));
        }
        let bindings =
            serde_json::from_value(previous.spec["dataBindings"].clone()).map_err(err)?;
        fd_subsystems::lifecycle::approve(
            &package,
            &body.approved_permissions,
            body.acknowledged_unsigned,
            &bindings,
            &runtime::folder_map(&self.app)?,
            true,
        )
        .map_err(err)?;
        let mut spec = previous.spec.clone();
        spec["version"] = json!(package.manifest.version);
        spec["packageSha256"] = json!(package.hash);
        spec["approvedPermissions"] = json!(body.approved_permissions);
        spec["signature"] = if package.signed {
            json!({"signed":{"keyId":package.signer_key_id}})
        } else {
            json!({"unsigned":{"acknowledged":true}})
        };
        Ok(Response::new(
            self.start("UpgradeExtension", spec, Some(previous), &bytes, &actor)
                .await?,
        ))
    }
    async fn remove_extension(
        &self,
        r: Request<pb::SubsystemRef>,
    ) -> Result<Response<pb::Operation>, Status> {
        self.metadata(&r, true)?;
        let actor = actor_of(r.metadata());
        let id = r.into_inner().subsystem_id;
        runtime::removable(&self.app, &id)?;
        let row = runtime::find(&self.app, &id)?
            .ok_or_else(|| Status::not_found("extension.not_found"))?;
        let op = self
            .app
            .journal
            .start(
                "RemoveExtension",
                json!({"spec":row.spec}),
                Some(row.resource),
                &actor,
                &["remove"],
            )
            .map_err(err)?;
        let app = self.app.clone();
        let op_id = op.id.clone();
        tokio::spawn(async move {
            let _ = runtime::resume(&app, &op_id).await;
        });
        Ok(Response::new(super::control::operation_to_proto(&op)))
    }
    async fn list_subsystems(
        &self,
        r: Request<pb::Empty>,
    ) -> Result<Response<pb::ListSubsystemsResponse>, Status> {
        self.metadata(&r, false)?;
        let folders = runtime::folder_map(&self.app)?;
        let rows = runtime::rows(&self.app, ResourceKind::Extension)?;
        let mut subsystems = self
            .app
            .subsystems
            .list()
            .map_err(err)?
            .into_iter()
            .map(|record| {
                let spec = rows
                    .iter()
                    .find(|r| r.spec["subsystemId"] == record.id)
                    .map(|r| r.spec.clone())
                    .unwrap_or(json!({}));
                let bindings: BTreeMap<String, String> =
                    serde_json::from_value(spec.get("dataBindings").cloned().unwrap_or(json!({})))
                        .unwrap_or_default();
                Ok(pb::Subsystem {
                    subsystem_id: record.id.clone(),
                    name: record.manifest["name"]
                        .as_str()
                        .unwrap_or(&record.id)
                        .into(),
                    version: record.version,
                    class: record.class,
                    capabilities: record.manifest["capabilities"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|v| v.as_str().map(str::to_owned))
                        .collect(),
                    runtime_type: record.manifest["runtime"]["type"]
                        .as_str()
                        .unwrap_or("")
                        .into(),
                    state: record.state,
                    dependent_folders: runtime::dependencies(&self.app, &record.id)?,
                    data_protected: bindings
                        .values()
                        .all(|id| folders.get(id).is_some_and(|f| f.protected)),
                    data_bindings: bindings.into_iter().collect(),
                })
            })
            .collect::<Result<Vec<_>, Status>>()?;
        for row in &rows {
            let id = row.spec["subsystemId"].as_str().unwrap_or("");
            if !subsystems.iter().any(|s| s.subsystem_id == id) {
                let bindings: BTreeMap<String, String> = serde_json::from_value(
                    row.spec.get("dataBindings").cloned().unwrap_or(json!({})),
                )
                .unwrap_or_default();
                subsystems.push(pb::Subsystem {
                    subsystem_id: id.into(),
                    name: id.into(),
                    version: row.spec["version"].as_str().unwrap_or("").into(),
                    class: "Extension".into(),
                    state: "PackageNeeded".into(),
                    data_protected: bindings
                        .values()
                        .all(|id| folders.get(id).is_some_and(|f| f.protected)),
                    data_bindings: bindings.into_iter().collect(),
                    ..Default::default()
                });
            }
        }
        Ok(Response::new(pb::ListSubsystemsResponse {
            subsystems,
            default_providers: fd_subsystems::capabilities::defaults(self.app.store.db())
                .map_err(err)?
                .into_iter()
                .collect(),
        }))
    }
    async fn set_default_provider(
        &self,
        r: Request<pb::SetDefaultProviderRequest>,
    ) -> Result<Response<pb::Empty>, Status> {
        self.metadata(&r, true)?;
        let body = r.into_inner();
        fd_subsystems::capabilities::set_default(
            &self.app.subsystems,
            &body.capability,
            &body.subsystem_id,
        )
        .map_err(err)?;
        self.app
            .events
            .emit(
                "capability.default_changed",
                None,
                json!({"capability":body.capability,"subsystemId":body.subsystem_id}),
            )
            .map_err(err)?;
        Ok(Response::new(pb::Empty {}))
    }
    async fn configure_notification_channel(
        &self,
        r: Request<pb::NotificationChannel>,
    ) -> Result<Response<pb::NotificationChannel>, Status> {
        self.metadata(&r, true)?;
        Err(Status::unimplemented(
            "notification channel delivery is Phase 9",
        ))
    }
}
