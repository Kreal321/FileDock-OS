//! Appliance-state capture, independent copy delivery, and single-writer ownership.
use crate::api::App;
use fd_appstate::{
    copy::backends::{Backend, CopyKind, CopyTarget},
    engine::{Engine, SnapshotInput},
};
use fd_connectors::ownership::{ClaimSafety, ObjectIo, OwnedLocation, Role, WriterSession};
use fd_state::{
    ResourceKind, ResourceRef, db,
    resources::{
        ConnectionDefinition, ResourceSpec, RouteTarget, RouteUse, StateDestinationTarget,
    },
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
};
use tonic::Status;
pub fn error(e: impl std::fmt::Display) -> Status {
    Status::failed_precondition(e.to_string())
}
pub fn appliance_id(app: &App) -> String {
    app.store
        .db()
        .with(|c| db::get_setting(c, "appliance_id"))
        .ok()
        .flatten()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_else(|| app.appliance_id.clone())
}
pub fn locations(app: &App) -> Result<Vec<OwnedLocation>, Status> {
    app.store
        .db()
        .with(|c| db::get_setting(c, "owned_locations"))
        .map_err(error)?
        .map(serde_json::from_value)
        .transpose()
        .map_err(error)
        .map(Option::unwrap_or_default)
}
pub fn save_locations(app: &App, locations: &[OwnedLocation]) -> Result<(), Status> {
    app.store
        .db()
        .with(|c| {
            let tx = c.transaction()?;
            let mut previous = db::get_setting(&tx, "owned_locations")?.unwrap_or(json!([]));
            let mut current = json!(locations);
            for value in [&mut previous, &mut current] {
                for location in value.as_array_mut().into_iter().flatten() {
                    if let Some(object) = location.as_object_mut() {
                        object.remove("lastWriteAt");
                    }
                }
            }
            if previous != current {
                db::put_setting(
                    &tx,
                    "config.changed_at",
                    &json!(chrono::Utc::now().to_rfc3339()),
                )?;
            }
            db::put_setting(&tx, "owned_locations", &json!(locations))?;
            tx.commit()?;
            Ok::<_, fd_state::DbError>(())
        })
        .map_err(error)
}
pub async fn input(app: &App) -> Result<SnapshotInput, Status> {
    let core_seq = app.store.change_seq().map_err(error)?;
    let changed_at = app
        .store
        .db()
        .with(|c| {
            Ok::<_, fd_state::DbError>(
                [
                    db::get_setting(c, "config.changed_at")?,
                    db::get_setting(c, "management.changed_at")?,
                ]
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .max(),
            )
        })
        .map_err(error)?;
    let (management, seq) = app
        .store
        .db()
        .with(|c| {
            Ok::<_, fd_state::DbError>((
                db::get_setting(c, "management.config")?.unwrap_or(json!({})),
                db::get_setting(c, "management.config_seq")?
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0),
            ))
        })
        .map_err(error)?;
    if management.as_object().is_none_or(|v| v.is_empty()) {
        return Err(Status::failed_precondition(
            "state.management_config_pending",
        ));
    }
    let recovery = app.recovery.lock().await;
    let masters = recovery.snapshot_keys().map_err(error)?;
    let key = recovery
        .current()
        .cloned()
        .ok_or_else(|| Status::failed_precondition("recovery.key_unconfirmed"))?;
    drop(recovery);
    let secrets = app.secrets.lock().await;
    let encrypted = secrets.encrypted_bytes().map_err(error)?;
    let dek = secrets.dek_bytes();
    drop(secrets);
    let id = appliance_id(app);
    let config =
        fd_appstate::fdconfig::serialize(app.store.db(), &id, management).map_err(error)?;
    let mut packages = BTreeMap::new();
    for item in config["core"]["subsystems"]
        .as_array()
        .into_iter()
        .flatten()
    {
        if let Some(hash) = item["packageSha256"].as_str() {
            if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(error("state.package_invalid"));
            }
            if let Some(bytes) = fd_appstate::packages::load(&app.config.extension_root, hash, &dek)
                .map_err(error)?
            {
                packages.insert(hash.into(), bytes);
            }
        }
    }
    Ok(SnapshotInput {
        appliance_id: id,
        config,
        core_seq,
        changed_at,
        management_seq: seq,
        secrets: encrypted,
        dek,
        key,
        masters,
        packages,
        release_version: app.config.release_version.into(),
    })
}
pub fn mounted_removable(path: &str) -> bool {
    let path = Path::new(path);
    if !path.is_absolute()
        || path
            .components()
            .any(|p| matches!(p, std::path::Component::ParentDir))
        || !std::fs::symlink_metadata(path).is_ok_and(|m| !m.file_type().is_symlink() && m.is_dir())
    {
        return false;
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::MetadataExt;
        let Ok(root) = std::fs::metadata("/") else {
            return false;
        };
        let Ok(target) = std::fs::metadata(path) else {
            return false;
        };
        if root.dev() == target.dev() {
            return false;
        }
        let Ok(mounts) = std::fs::read_to_string("/proc/self/mountinfo") else {
            return false;
        };
        let Ok(canonical) = path.canonicalize() else {
            return false;
        };
        return mounts.lines().any(|l| {
            l.split_whitespace()
                .nth(4)
                .is_some_and(|p| Path::new(p) == canonical)
        });
    }
    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}
pub async fn object_io(
    app: &App,
    id: &str,
    bucket: &str,
    prefix: &str,
    builtin: bool,
) -> Result<(Arc<dyn ObjectIo>, String, ClaimSafety), Status> {
    if prefix.starts_with('/')
        || prefix.contains('\\')
        || prefix.split('/').any(|p| p == ".." || p == ".")
    {
        return Err(error("destination.prefix_invalid"));
    }
    let prefix = if prefix.is_empty() {
        String::new()
    } else {
        format!("{}/", prefix.trim_end_matches('/'))
    };
    let _route_guard = app.route_lock.lock().await;
    let row = app
        .store
        .get(&ResourceRef::new(ResourceKind::CloudConnection, id))
        .map_err(error)?
        .ok_or_else(|| error("connection.not_found"))?;
    let ResourceSpec::CloudConnection(connection) = row
        .parsed_spec()
        .map_err(|_| error("resource.spec_invalid"))?
    else {
        return Err(error("connection.invalid"));
    };
    let routes = app
        .store
        .get(&ResourceRef::singleton(ResourceKind::ConnectorRoutes))
        .map_err(error)?
        .ok_or_else(|| error("route.missing"))?;
    let ResourceSpec::ConnectorRoutes(routes) = routes
        .parsed_spec()
        .map_err(|_| error("resource.spec_invalid"))?
    else {
        return Err(error("route.missing"));
    };
    let target = if builtin {
        if connection.provider_family == "s3-compatible" {
            "fd-s3".into()
        } else {
            "rclone".into()
        }
    } else {
        match fd_connectors::routes::resolve(
            &routes,
            Some(&connection),
            &connection.provider_family,
            RouteUse::OwnedObjectStorage,
        )
        .map_err(error)?
        {
            RouteTarget::Connector { subsystem_id } => subsystem_id,
            RouteTarget::BackupEngine => return Err(error("route.use_not_allowed")),
        }
    };
    if !builtin {
        crate::runtime::require_connector(app, &target, id, RouteUse::OwnedObjectStorage)?;
    }
    let registry = app.subsystems.list().map_err(error)?;
    if !builtin {
        fd_connectors::routes::validate_target(
            &connection.provider_family,
            RouteUse::OwnedObjectStorage,
            &RouteTarget::Connector {
                subsystem_id: target.clone(),
            },
            Some(&connection),
            &registry,
            None,
        )
        .map_err(error)?;
    }
    let safety = fd_connectors::route_change::claim_safety(
        &row,
        &RouteTarget::Connector {
            subsystem_id: target.clone(),
        },
        &registry,
    );
    if let ConnectionDefinition::S3(def) = &connection.definition
        && def.bucket != bucket
    {
        return Err(error("destination.bucket_mismatch"));
    }
    if target == "rclone" {
        let mut io = crate::session_runtime::rclone(app, &connection).await?;
        io.remote = format!("{}:{bucket}", io.cleanup_name.as_ref().unwrap());
        io.prefix = prefix;
        return Ok((Arc::new(io), target, safety));
    }
    let mut io = crate::session_runtime::native(app, &row, &target).await?;
    io.prefix = prefix;
    Ok((Arc::new(io), target, safety))
}

pub async fn targets(app: &App) -> Result<Vec<CopyTarget>, Status> {
    let mut targets = vec![CopyTarget {
        id: "system".into(),
        kind: CopyKind::LocalSystemDisk,
        pool_id: String::new(),
        description: "System disk".into(),
        available: true,
        backend: Backend::Local(app.config.state_root.clone()),
    }];
    for row in app
        .store
        .list(ResourceKind::Pool, None, 10000)
        .map_err(error)?
    {
        let root = app.config.pools_root.join(&row.resource.id);
        if row
            .status
            .as_ref()
            .is_some_and(|s| matches!(s["state"].as_str(), Some("Healthy" | "Ready" | "Available")))
            && root.join(".filedock-pool.json").is_file()
        {
            targets.push(CopyTarget {
                id: format!("pool:{}", row.resource.id),
                kind: CopyKind::LocalPool,
                pool_id: row.resource.id.clone(),
                description: row.spec["name"].as_str().unwrap_or("Data pool").into(),
                available: true,
                backend: Backend::Local(root.join(".filedock/appliance-state")),
            });
        }
    }
    if let Some(row) = app
        .store
        .get(&ResourceRef::singleton(
            ResourceKind::ApplianceStateDestination,
        ))
        .map_err(error)?
    {
        let ResourceSpec::ApplianceStateDestination(destination) = row
            .parsed_spec()
            .map_err(|_| error("resource.spec_invalid"))?
        else {
            return Err(error("destination.invalid"));
        };
        match destination.target {
            StateDestinationTarget::RemovableDisk { path } => {
                let available = mounted_removable(&path);
                targets.push(CopyTarget {
                    id: format!("disk:{path}"),
                    kind: CopyKind::RemoteRemovableDisk,
                    pool_id: String::new(),
                    description: path.clone(),
                    available,
                    backend: Backend::Local(PathBuf::from(path).join("filedock/appliance-state")),
                });
            }
            StateDestinationTarget::ObjectStorage {
                connection_id,
                bucket,
                prefix,
            } => {
                let location = locations(app)?
                    .into_iter()
                    .find(|l| {
                        l.connection_id == connection_id && l.bucket == bucket && l.prefix == prefix
                    })
                    .ok_or_else(|| error("location.not_claimed"))?;
                let (available, backend) = remote_copy_backend(
                    &location,
                    app.config.state_spool.join("unavailable"),
                    object_io(app, &connection_id, &bucket, &prefix, false),
                )
                .await;
                targets.push(CopyTarget {
                    id: location.id.clone(),
                    kind: CopyKind::RemoteObjectStorage,
                    pool_id: String::new(),
                    description: format!("{bucket}/{prefix}"),
                    available,
                    backend,
                });
            }
        }
    }
    Ok(targets)
}
// Session construction waits for the route lock and connector configuration. A short
// wall-clock timeout here would misclassify a healthy connector during an atomic route
// change. Provider RPCs have their own deadlines; write_copy bounds delivery separately.
async fn remote_copy_backend(
    location: &OwnedLocation,
    unavailable: PathBuf,
    session: impl std::future::Future<Output = Result<(Arc<dyn ObjectIo>, String, ClaimSafety), Status>>,
) -> (bool, Backend) {
    let resolved = session.await;
    let available = location.role == Role::Writer && resolved.is_ok();
    let backend = match resolved {
        Ok((io, _, _)) => Backend::Remote(Arc::new(WriterSession {
            io,
            location: location.clone(),
        })),
        Err(_) => Backend::Local(unavailable),
    };
    (available, backend)
}

pub async fn snapshot(app: &App, force: bool) -> Result<bool, Status> {
    let _guard = app.restore_lock.lock().await;
    let _ = tokio::time::timeout(std::time::Duration::from_secs(1), inspect_locations(app)).await;
    let targets = targets(app).await?;
    app.state_engine
        .lock()
        .await
        .inventory(&targets)
        .map_err(error)?;
    let input = input(app).await?;
    let result = app
        .state_engine
        .lock()
        .await
        .tick(&input, &targets, force)
        .await
        .map_err(error)?;
    // The run loop compares the complete protection projection and emits only changes.
    // A successful no-op tick must not become durable activity every two seconds.
    Ok(result)
}
pub async fn run(app: Arc<App>) {
    let mut previous = Value::Null;
    loop {
        app.restore_sources
            .lock()
            .await
            .retain(|_, source| source.valid());
        if let Err(e) = snapshot(&app, false).await
            && !matches!(
                e.message(),
                "state.management_config_pending" | "recovery.key_unconfirmed"
            )
        {
            tracing::warn!(code=%e.message(),"appliance snapshot pending");
        }
        if let Ok(records) = Engine::records(app.store.db())
            && let Ok(mut locations) = locations(&app)
        {
            let mut changed = false;
            for location in &mut locations {
                if records.iter().any(|r| {
                    r.id == location.id && r.last_error.contains("location.ownership_lost")
                }) {
                    location.role = Role::OwnershipLost;
                    changed = true;
                    let _ = app.alerts.raise(
                        &format!("location:{}", location.id),
                        None,
                        "location.ownership_lost",
                        fd_alerts::Severity::Critical,
                        "Ownership changed. Writes, write probes and sweeping have stopped.",
                    );
                }
            }
            if changed {
                let _ = save_locations(&app, &locations);
            }
        }
        if let Ok(protection) = Engine::protection(app.store.db())
            && protection != previous
        {
            let _ = app
                .events
                .emit("state.protection.changed", None, protection.clone());
            for copy in protection["copies"].as_array().into_iter().flatten() {
                let source = format!(
                    "appliance-state:{}",
                    copy["id"].as_str().unwrap_or("unknown")
                );
                if copy["stale"] == true {
                    let _ = app.alerts.raise(
                        &source,
                        None,
                        "state.copy_stale",
                        fd_alerts::Severity::Warning,
                        "A configuration copy is unavailable or more than five minutes behind.",
                    );
                } else {
                    let _ = app.alerts.resolve(&source, None, "state.copy_stale");
                }
            }
            previous = protection;
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
}
/// Inspect the actual owner before a write probe, and provide the current generation to the UI.
pub async fn inspect_locations(app: &App) -> Result<Vec<OwnedLocation>, Status> {
    let mut persisted = locations(app)?;
    let mut visible = persisted.clone();
    let mut changed = false;
    for (cache, view) in persisted.iter_mut().zip(visible.iter_mut()) {
        let (io, _, _) = object_io(
            app,
            &cache.connection_id,
            &cache.bucket,
            &cache.prefix,
            false,
        )
        .await?;
        let actual = io
            .read(fd_connectors::ownership::OWNER_KEY)
            .await
            .map_err(error)?
            .ok_or_else(|| error("location.owner_missing"))?;
        let owner: fd_connectors::ownership::Owner =
            serde_json::from_slice(&actual.bytes).map_err(|_| error("location.owner_invalid"))?;
        if owner.schema_version != 1 || owner.generation == 0 {
            return Err(error("location.owner_invalid"));
        }
        if cache.role == Role::Writer
            && (owner.generation != cache.generation
                || owner.appliance_id != cache.owner_appliance_id)
        {
            cache.role = Role::OwnershipLost;
            view.role = Role::OwnershipLost;
            changed = true;
            let _ = app.alerts.raise(
                &format!("location:{}", cache.id),
                None,
                "location.ownership_lost",
                fd_alerts::Severity::Critical,
                "Ownership changed. All writes have stopped.",
            );
        }
        if cache.last_write_at != owner.last_write_at {
            cache.last_write_at = owner.last_write_at.clone();
            changed = true;
        }
        view.generation = owner.generation;
        view.last_write_at = owner.last_write_at;
        view.owner_appliance_id = owner.appliance_id;
    }
    if changed {
        save_locations(app, &persisted)?;
    }
    Ok(visible)
}

/// Recompute local claim observations without touching remote ownership or stopping writes.
pub fn recompute_claim_safety(app: &App) -> Result<(), Status> {
    let row = app
        .store
        .get(&ResourceRef::singleton(ResourceKind::ConnectorRoutes))
        .map_err(error)?
        .ok_or_else(|| error("route.missing"))?;
    let ResourceSpec::ConnectorRoutes(routes) = row.parsed_spec().map_err(|e| error(e.code))?
    else {
        return Err(error("route.invalid"));
    };
    let connectors = app.subsystems.list().map_err(error)?;
    let mut all = locations(app)?;
    for location in &mut all {
        let Some(row) = app
            .store
            .get(&ResourceRef::new(
                ResourceKind::CloudConnection,
                &location.connection_id,
            ))
            .map_err(error)?
        else {
            continue;
        };
        let ResourceSpec::CloudConnection(c) = row.parsed_spec().map_err(|e| error(e.code))? else {
            continue;
        };
        let Ok(target) = fd_connectors::routes::resolve(
            &routes,
            Some(&c),
            &c.provider_family,
            RouteUse::OwnedObjectStorage,
        ) else {
            continue;
        };
        let next = fd_connectors::route_change::claim_safety(&row, &target, &connectors);
        if matches!(location.claim_safety, ClaimSafety::Conditional)
            && matches!(next, ClaimSafety::Basic)
        {
            let _=app.alerts.raise(&format!("location:{}",location.id),None,"location.claim_safety_downgraded",fd_alerts::Severity::Warning,"Conditional ownership protection is no longer proven; writes continue using basic verification.");
        }
        if matches!(next, ClaimSafety::Conditional) {
            let _ = app.alerts.resolve(
                &format!("location:{}", location.id),
                None,
                "location.claim_safety_downgraded",
            );
        }
        location.claim_safety = next;
        location.served_by_connector = target.subsystem_id().unwrap_or("backup-engine").into();
        location.served_by_third_party =
            fd_connectors::route_change::third_party(&target, &connectors);
    }
    if all
        .iter()
        .any(|l| matches!(l.claim_safety, ClaimSafety::Basic))
    {
        let _=app.alerts.raise("appliance",None,"location.basic_claim_safety",fd_alerts::Severity::Warning,"An owned location uses basic claim safety; concurrent writers cannot be prevented by conditional writes.");
    } else {
        let _ = app
            .alerts
            .resolve("appliance", None, "location.basic_claim_safety");
    }
    save_locations(app, &all)
}

#[cfg(test)]
mod copy_session_tests {
    use super::*;

    fn location(role: Role) -> OwnedLocation {
        OwnedLocation {
            id: "test-location".into(),
            connection_id: "test-connection".into(),
            bucket: "test-bucket".into(),
            prefix: "state".into(),
            purpose: "ApplianceState".into(),
            owner_appliance_id: "test-owner".into(),
            generation: 1,
            last_write_at: String::new(),
            role,
            served_by_connector: "fd-s3".into(),
            served_by_third_party: false,
            claim_safety: ClaimSafety::Conditional,
        }
    }

    fn io() -> Arc<dyn ObjectIo> {
        Arc::new(fd_connectors::object_io::S3Objects {
            // No network I/O: this check exercises session readiness, not provider operations.
            channel: tonic::transport::Endpoint::from_static("http://localhost").connect_lazy(),
            connection_id: "test-connection".into(),
            prefix: String::new(),
            cleanup_directory: None,
        })
    }

    #[tokio::test]
    async fn route_contention_does_not_make_a_healthy_copy_unavailable() {
        let lock = Arc::new(tokio::sync::Mutex::new(()));
        let held = lock.clone().lock_owned().await;
        let waiter = lock.clone();
        let session = async move {
            let _guard = waiter.lock().await;
            Ok((io(), "fd-s3".into(), ClaimSafety::Conditional))
        };
        let copy = tokio::spawn(async move {
            remote_copy_backend(&location(Role::Writer), "/unused".into(), session).await
        });
        tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
        assert!(
            !copy.is_finished(),
            "route lock contention is not connector failure"
        );
        drop(held);
        let (available, backend) = copy.await.unwrap();
        assert!(available);
        assert!(matches!(backend, Backend::Remote(_)));
    }

    #[tokio::test]
    async fn unavailable_connector_and_nonwriter_stay_unavailable() {
        let (available, backend) =
            remote_copy_backend(&location(Role::Writer), "/unused".into(), async {
                Err(Status::unavailable("connector.unavailable"))
            })
            .await;
        assert!(!available);
        assert!(matches!(backend, Backend::Local(_)));
        for role in [Role::RecoveryReader, Role::OwnershipLost] {
            let (available, _) = remote_copy_backend(&location(role), "/unused".into(), async {
                Ok((io(), "fd-s3".into(), ClaimSafety::Conditional))
            })
            .await;
            assert!(!available);
        }
    }
}
