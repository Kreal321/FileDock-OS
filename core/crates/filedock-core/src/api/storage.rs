use super::{
    App,
    interceptors::{actor_of, check_metadata},
};
use crate::{api::control::operation_to_proto, storage_runtime as runtime};
use fd_api::core::v1::{self as pb, storage_server::Storage};
use fd_state::{ResourceKind, ResourceRef};
use serde_json::json;
use std::sync::Arc;
use tonic::{Request, Response, Status};
pub struct StorageSvc {
    pub app: Arc<App>,
}
fn error(e: impl std::fmt::Display) -> Status {
    Status::failed_precondition(e.to_string())
}
#[tonic::async_trait]
impl Storage for StorageSvc {
    async fn list_disks(
        &self,
        r: Request<pb::ListDisksRequest>,
    ) -> Result<Response<pb::ListDisksResponse>, Status> {
        check_metadata(r.metadata(), self.app.config.interface_version, false)?;
        let disks = runtime::inventory(&self.app).map_err(error)?;
        Ok(Response::new(pb::ListDisksResponse {
            disks: disks
                .into_iter()
                .map(|d| pb::Disk {
                    serial: d.serial,
                    wwn: d.wwn,
                    model: d.model,
                    size_bytes: d.size_bytes,
                    device_path: d.device_path,
                    is_system_disk: d.is_system_disk,
                    is_blank: d.is_blank,
                    existing_filesystem: d.existing_filesystem,
                    pool_id: d.pool_id,
                    health: match d.health.as_str() {
                        "OK" => 1,
                        "Warning" => 2,
                        "Failing" => 3,
                        _ => 0,
                    },
                })
                .collect(),
        }))
    }
    async fn create_pool(
        &self,
        r: Request<pb::CreatePoolRequest>,
    ) -> Result<Response<pb::Operation>, Status> {
        check_metadata(r.metadata(), self.app.config.interface_version, true)?;
        let actor = actor_of(r.metadata());
        let r = r.into_inner();
        self.start(
            "CreatePool",
            uuid::Uuid::now_v7().to_string(),
            r.name,
            r.disk_serials,
            r.typed_confirmation,
            &actor,
        )
        .await
    }
    async fn adopt_filesystem(
        &self,
        r: Request<pb::AdoptFilesystemRequest>,
    ) -> Result<Response<pb::Operation>, Status> {
        check_metadata(r.metadata(), self.app.config.interface_version, true)?;
        let actor = actor_of(r.metadata());
        let r = r.into_inner();
        self.start(
            "AdoptFilesystem",
            uuid::Uuid::now_v7().to_string(),
            r.name,
            vec![r.disk_serial],
            String::new(),
            &actor,
        )
        .await
    }
    async fn extend_pool(
        &self,
        r: Request<pb::ExtendPoolRequest>,
    ) -> Result<Response<pb::Operation>, Status> {
        check_metadata(r.metadata(), self.app.config.interface_version, true)?;
        let actor = actor_of(r.metadata());
        let r = r.into_inner();
        self.start(
            "ExtendPool",
            r.pool_id,
            String::new(),
            vec![r.disk_serial],
            r.typed_confirmation,
            &actor,
        )
        .await
    }
    async fn recreate_pool(
        &self,
        r: Request<pb::RecreatePoolRequest>,
    ) -> Result<Response<pb::Operation>, Status> {
        check_metadata(r.metadata(), self.app.config.interface_version, true)?;
        let actor = actor_of(r.metadata());
        let r = r.into_inner();
        self.start(
            "RecreatePool",
            r.pool_id,
            String::new(),
            r.disk_serials,
            r.typed_confirmation,
            &actor,
        )
        .await
    }
    async fn get_system_pool(
        &self,
        r: Request<pb::Empty>,
    ) -> Result<Response<pb::SystemPool>, Status> {
        check_metadata(r.metadata(), self.app.config.interface_version, false)?;
        Ok(Response::new(
            runtime::system_view(&self.app).map_err(error)?,
        ))
    }
    async fn set_system_pool_limits(
        &self,
        r: Request<pb::SetSystemPoolLimitsRequest>,
    ) -> Result<Response<pb::Operation>, Status> {
        check_metadata(r.metadata(), self.app.config.interface_version, true)?;
        let actor = actor_of(r.metadata());
        let r = r.into_inner();
        let _gate = self.app.storage_lock.lock().await;
        runtime::enabled()?;
        let current = runtime::system_view(&self.app).map_err(error)?;
        if current.generation != r.expected_generation {
            return Err(Status::aborted("generation mismatch"));
        }
        let system = fd_storage::pressure::space(std::path::Path::new("/var/lib/filedock"))
            .map_err(error)?;
        let image = runtime::image();
        if image.exists() && !current.available {
            return Err(error("system_pool.unavailable"));
        }
        let cap = fd_storage::system_pool::limits::capacity(
            system.total,
            system
                .total
                .saturating_sub(system.free)
                .saturating_sub(current.used_bytes),
            current.used_bytes.saturating_add(current.used_bytes / 20),
            r.max_usage_percent,
            r.reserved_free_bytes,
        )
        .map_err(error)?;
        if cap.bytes
            < std::fs::metadata(&image)
                .map(|m| m.len())
                .unwrap_or(current.effective_capacity_bytes)
            && !r.acknowledged_interruption
        {
            return Err(error("system_pool.interruption_unacknowledged"));
        }
        let params = json!({"id":current.pool_id,"generation":current.generation,"percent":r.max_usage_percent,"reserve":r.reserved_free_bytes,"capacity":cap.bytes});
        runtime::ensure_no_operation(&self.app, &current.pool_id, &[])?;
        let op = self
            .app
            .journal
            .start(
                "SystemPoolResize",
                params,
                None,
                &actor,
                &["resize", "commit"],
            )
            .map_err(error)?;
        runtime::spawn(self.app.clone(), op.id.clone());
        Ok(Response::new(operation_to_proto(&op)))
    }
}
impl StorageSvc {
    async fn start(
        &self,
        kind: &str,
        id: String,
        name: String,
        serials: Vec<String>,
        confirmation: String,
        actor: &str,
    ) -> Result<Response<pb::Operation>, Status> {
        let _gate = self.app.storage_lock.lock().await;
        runtime::enabled()?;
        runtime::ensure_no_operation(&self.app, &id, &serials)?;
        let disks = runtime::inventory(&self.app).map_err(error)?;
        let selected = fd_storage::disks::select(
            &disks,
            &serials,
            if kind == "AdoptFilesystem" {
                None
            } else {
                Some(&confirmation)
            },
        )
        .map_err(error)?;
        let mut spec = json!({"name":name,"kind":if kind=="AdoptFilesystem"{"Adopted"}else{"Lvm"},"layers":if kind=="AdoptFilesystem"{vec!["ext4"]}else{vec!["lvm","ext4"]},"memberDisks":selected.iter().map(|d|json!({"serial":d.serial,"wwn":d.wwn})).collect::<Vec<_>>()});
        let mut generation = 0;
        if kind == "AdoptFilesystem"
            && (selected.len() != 1 || selected[0].existing_filesystem != "ext4")
        {
            return Err(error("pool.adopt_requires_unmounted_ext4"));
        }
        if kind == "ExtendPool" || kind == "RecreatePool" {
            let old = self
                .app
                .store
                .get(&ResourceRef::new(ResourceKind::Pool, &id))
                .map_err(error)?
                .ok_or_else(|| Status::not_found("pool.not_found"))?;
            if old.spec["kind"] == "System" {
                return Err(error("system_pool.immutable"));
            }
            if old.spec["kind"] != "Lvm" {
                return Err(error("pool.not_lvm"));
            }
            if kind == "RecreatePool" && old.status.as_ref().is_none_or(|s| s["state"] != "Failed")
            {
                return Err(error("pool.not_failed"));
            }
            generation = old.generation;
            spec["name"] = old.spec["name"].clone();
            if kind == "ExtendPool" {
                let members = old.spec["memberDisks"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                spec["memberDisks"]
                    .as_array_mut()
                    .unwrap()
                    .splice(0..0, members);
            }
        }
        let change = fd_state::Change::Upsert {
            resource: ResourceRef::new(ResourceKind::Pool, &id),
            expected_generation: generation,
            spec: spec.clone(),
        };
        let issues = self.app.store.validate(&[change]).map_err(error)?;
        if issues.iter().any(|i| i.is_error()) {
            return Err(error(format!("pool.invalid: {issues:?}")));
        }
        let params = json!({"id":id,"spec":spec,"generation":generation,"serials":serials});
        let steps = if kind == "RecreatePool" {
            &["retire", "pv", "volume", "format", "mount", "commit"][..]
        } else if kind == "AdoptFilesystem" {
            &["adopt", "commit"][..]
        } else if kind == "ExtendPool" {
            &["extend", "commit"][..]
        } else {
            &["pv", "volume", "format", "mount", "commit"][..]
        };
        let op = self
            .app
            .journal
            .start(
                kind,
                params,
                Some(ResourceRef::new(ResourceKind::Pool, &id)),
                actor,
                steps,
            )
            .map_err(error)?;
        runtime::spawn(self.app.clone(), op.id.clone());
        Ok(Response::new(operation_to_proto(&op)))
    }
}
