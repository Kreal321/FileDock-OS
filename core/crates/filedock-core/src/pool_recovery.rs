//! Pool identity recovery delegates all destructive work to the same durable storage steps.
use crate::api::App;
use fd_api::core::v1 as pb;
use fd_state::{Change, ChangeOrigin, ResourceKind, ResourceRef};
use fd_storage::{Error, Result};
use serde_json::{Value, json};
use std::sync::Arc;
fn error(e: impl std::fmt::Display) -> Error {
    Error(e.to_string())
}
pub fn reattach(app: &App) -> Result<()> {
    for row in crate::storage_runtime::rows(app, ResourceKind::Pool)? {
        if row.spec["kind"] == "System"
            || fd_storage::pool::health::mounted(&app.config.pools_root, &row.resource.id).is_ok()
        {
            continue;
        }
        let id = &row.resource.id;
        let name = row.spec["name"].as_str().unwrap_or("pool");
        let mut members = Vec::new();
        if row.spec["kind"] == "Lvm" {
            let vg = fd_storage::pool::lvm::vg(id);
            if !fd_storage::run("vgs", &["--noheadings", "-o", "vg_tags", &vg]).is_ok_and(|tags| {
                tags.split(',')
                    .any(|t| t.trim() == format!("filedock_pool={id}"))
            }) {
                continue;
            }
            fd_storage::run("vgchange", &["-ay", &vg])?;
            fd_storage::pool::finish(
                &app.config.pools_root,
                id,
                name,
                &fd_storage::pool::lvm::device(id),
            )?;
            let paths = fd_storage::run(
                "pvs",
                &[
                    "--noheadings",
                    "-o",
                    "pv_name",
                    "--select",
                    &format!("vg_name={vg}"),
                ],
            )?;
            for disk in fd_storage::disks::discover()? {
                if paths.lines().any(|p| p.trim() == disk.device_path) {
                    members.push(json!({"serial":disk.serial,"wwn":disk.wwn}));
                }
            }
        } else {
            for disk in fd_storage::disks::discover()?
                .into_iter()
                .filter(|d| !d.is_system_disk && d.existing_filesystem == "ext4")
            {
                if fd_storage::run("findmnt", &["--source", &disk.device_path, "--noheadings"])
                    .is_ok()
                {
                    continue;
                }
                let probe = app
                    .config
                    .socket
                    .parent()
                    .unwrap()
                    .join(format!("pool-probe-{}", uuid::Uuid::new_v4()));
                std::fs::create_dir(&probe)?;
                let mounted = fd_storage::run(
                    "mount",
                    &[
                        "-o",
                        "ro,noload",
                        &disk.device_path,
                        probe.to_str().unwrap(),
                    ],
                );
                let marker = if mounted.is_ok() {
                    std::fs::read(probe.join(".filedock-pool.json"))
                        .ok()
                        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
                } else {
                    None
                };
                if mounted.is_ok() {
                    fd_storage::run("umount", &[probe.to_str().unwrap()])?;
                }
                let _ = std::fs::remove_dir(&probe);
                if marker.is_some_and(|m| m["poolId"] == *id) {
                    fd_storage::pool::adopt::adopt(&app.config.pools_root, id, name, &disk)?;
                    members.push(json!({"serial":disk.serial,"wwn":disk.wwn}));
                    break;
                }
            }
        }
        if !members.is_empty() {
            let mut spec = row.spec;
            spec["memberDisks"] = json!(members);
            app.store
                .apply(
                    ChangeOrigin::Restore,
                    &[Change::Upsert {
                        resource: row.resource,
                        expected_generation: row.generation,
                        spec,
                    }],
                )
                .map_err(error)?;
        }
    }
    Ok(())
}
pub async fn map(
    app: &Arc<App>,
    request: &pb::MapPoolsRequest,
) -> std::result::Result<fd_journal::Operation, tonic::Status> {
    use tonic::Status;
    crate::storage_runtime::enabled()?;
    let _gate = app.storage_lock.lock().await;
    let restore = app
        .journal
        .get(&request.restore_operation_id)
        .map_err(|e| Status::not_found(e.to_string()))?;
    if restore.kind != "Restore" || request.mappings.is_empty() {
        return Err(Status::invalid_argument("restore.mapping_required"));
    }
    let inventory = crate::storage_runtime::inventory(app)
        .map_err(|e| Status::failed_precondition(e.to_string()))?;
    let mut seen = std::collections::BTreeSet::new();
    let mut serials = Vec::new();
    let mut mappings = Vec::new();
    let mut pools = std::collections::BTreeSet::new();
    for mapping in &request.mappings {
        let row = app
            .store
            .get(&ResourceRef::new(ResourceKind::Pool, &mapping.pool_id))
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found("pool.not_found"))?;
        if row.spec["kind"] == "System" {
            return Err(Status::failed_precondition("system_pool.immutable"));
        }
        if !pools.insert(mapping.pool_id.clone()) {
            return Err(Status::invalid_argument("restore.duplicate_mapping"));
        }
        if fd_storage::pool::health::mounted(&app.config.pools_root, &mapping.pool_id).is_ok() {
            return Err(Status::failed_precondition("restore.pool_already_attached"));
        }
        crate::storage_runtime::ensure_no_operation(app, &mapping.pool_id, &mapping.disk_serials)?;
        let disks = fd_storage::disks::select(
            &inventory,
            &mapping.disk_serials,
            if mapping.adopt_existing {
                None
            } else {
                Some(&mapping.typed_confirmation)
            },
        )
        .map_err(|e| Status::failed_precondition(e.to_string()))?;
        if mapping.adopt_existing && (disks.len() != 1 || disks[0].existing_filesystem != "ext4") {
            return Err(Status::failed_precondition(
                "pool.adopt_requires_unmounted_ext4",
            ));
        }
        for disk in &disks {
            if !seen.insert(disk.serial.clone()) {
                return Err(Status::invalid_argument("restore.disk_reused"));
            }
            serials.push(disk.serial.clone());
        }
        let mut spec = row.spec;
        spec["memberDisks"] = json!(
            disks
                .iter()
                .map(|d| json!({"serial":d.serial,"wwn":d.wwn}))
                .collect::<Vec<_>>()
        );
        spec["kind"] = json!(if mapping.adopt_existing {
            "Adopted"
        } else {
            "Lvm"
        });
        spec["layers"] = json!(if mapping.adopt_existing {
            vec!["ext4"]
        } else {
            vec!["lvm", "ext4"]
        });
        mappings.push(json!({"id":mapping.pool_id,"generation":row.generation,"serials":mapping.disk_serials,"spec":spec,"adopt":mapping.adopt_existing}));
    }
    let steps = mappings
        .iter()
        .map(|m| m["id"].as_str().unwrap())
        .collect::<Vec<_>>();
    let op=app.journal.start("MapPools",json!({"restoreOperationId":request.restore_operation_id,"mappings":mappings,"serials":serials}),None,"restore",&steps).map_err(|e|Status::internal(e.to_string()))?;
    crate::storage_runtime::spawn(app.clone(), op.id.clone());
    Ok(op)
}
pub fn resume(app: &App, id: &str) -> Result<()> {
    let parent = app.journal.get(id).map_err(error)?;
    for step in parent.steps.iter().filter(|s| s.state != "done") {
        app.journal.begin_step(id, &step.name).map_err(error)?;
        let mut mapping = parent.params["mappings"]
            .as_array()
            .and_then(|a| a.iter().find(|m| m["id"] == step.name))
            .cloned()
            .ok_or_else(|| error("restore.mapping_missing"))?;
        mapping["mappingParent"] = json!(id);
        let existing=app.store.db().with(|c|{use rusqlite::OptionalExtension;c.query_row("SELECT id FROM operation WHERE json_extract(params,'$.mappingParent')=?1 AND json_extract(params,'$.id')=?2",rusqlite::params![id,step.name],|r|r.get::<_,String>(0)).optional()}).map_err(error)?;
        let child = if let Some(id) = existing {
            app.journal.get(&id).map_err(error)?
        } else {
            let adopt = mapping["adopt"] == true;
            app.journal
                .start(
                    if adopt {
                        "AdoptFilesystem"
                    } else {
                        "RecreatePool"
                    },
                    mapping,
                    None,
                    "restore",
                    if adopt {
                        &["adopt", "commit"]
                    } else {
                        &["retire", "pv", "volume", "format", "mount", "commit"]
                    },
                )
                .map_err(error)?
        };
        let result = if child.state == fd_journal::OperationState::Succeeded {
            Ok(())
        } else if child.state.is_terminal() {
            Err(error("restore.pool_operation_failed"))
        } else {
            crate::storage_runtime::resume(app, &child.id)
        };
        let failure = result.as_ref().err().map(ToString::to_string);
        app.journal
            .finish_step(
                id,
                &step.name,
                result
                    .map(|_| json!({"operationId":child.id}))
                    .map_err(|e| e.to_string()),
            )
            .map_err(error)?;
        if let Some(failure) = failure {
            return Err(error(failure));
        }
        let restore = parent.params["restoreOperationId"].as_str().unwrap();
        let key = format!("restore.report.{restore}");
        if let Some(mut report) = app
            .store
            .db()
            .with(|c| fd_state::db::get_setting(c, &key))
            .map_err(error)?
        {
            for item in report["items"].as_array_mut().into_iter().flatten() {
                if item["kind"] == "Pool" && item["id"] == step.name {
                    item["status"] = json!(if child.params["adopt"] == true {
                        "Reattached"
                    } else {
                        "Recreated"
                    });
                }
            }
            let folders = crate::storage_runtime::rows(app, ResourceKind::ManagedFolder)?;
            for folder in report["folders"].as_array_mut().into_iter().flatten() {
                if folders.iter().any(|f| {
                    f.resource.id == folder["folderId"].as_str().unwrap_or("")
                        && f.spec["reference"]["poolId"] == step.name
                }) {
                    folder["outcome"] = json!(if child.params["adopt"] == true {
                        "DataPresent"
                    } else if folder["storageMode"] == "LocalOnly" {
                        "Lost"
                    } else {
                        "AwaitingDataRestore"
                    });
                    folder["detail"] = json!(if child.params["adopt"] == true {
                        "Existing filesystem reattached without formatting; data remains in place."
                    } else {
                        "Pool recreated on replacement disks; previous local-only data is lost. Provider-backed restore is pending where available."
                    });
                }
            }
            app.store
                .db()
                .with(|c| fd_state::db::put_setting(c, &key, &report))
                .map_err(error)?;
        }
    }
    Ok(())
}
