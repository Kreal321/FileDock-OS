//! Journaled extension activation, rollback, and recovery using the generic contract.
use crate::api::App;
use fd_state::{Change, ChangeOrigin, ResourceKind, StoredResource, db};
use fd_subsystems::{
    SubsystemAdapter,
    bridge::Instance,
    package::Package,
    runtime::Runtime,
    sandbox::{Binding, Sandbox},
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tonic::Status;
fn error(e: impl std::fmt::Display) -> Status {
    Status::failed_precondition(e.to_string())
}
pub fn rows(app: &App, kind: ResourceKind) -> Result<Vec<StoredResource>, Status> {
    app.store.list(kind, None, i64::MAX as usize).map_err(error)
}
pub fn find(app: &App, id: &str) -> Result<Option<StoredResource>, Status> {
    Ok(rows(app, ResourceKind::Extension)?
        .into_iter()
        .find(|r| r.spec["subsystemId"] == id))
}
pub fn folder_map(app: &App) -> Result<BTreeMap<String, Binding>, Status> {
    Ok(rows(app, ResourceKind::ManagedFolder)?
        .into_iter()
        .map(|f| {
            (
                f.resource.id.clone(),
                Binding {
                    folder_id: f.resource.id,
                    source: f.spec["filedockPath"].as_str().unwrap_or("").into(),
                    protected: f.spec["storageMode"] == "HybridBackup"
                        && f.spec["advanced"]["backupEnabled"] != false,
                },
            )
        })
        .collect())
}
pub fn dependencies(app: &App, id: &str) -> Result<Vec<String>, Status> {
    let record = app
        .subsystems
        .get(id)
        .map_err(error)?
        .ok_or_else(|| Status::not_found("extension.not_found"))?;
    let defaults = fd_subsystems::capabilities::defaults(app.store.db()).map_err(error)?;
    let mut dependent = vec![];
    for folder in rows(app, ResourceKind::ManagedFolder)? {
        let mut caps = vec!["storage.local"];
        match folder.spec["storageMode"].as_str() {
            Some("HybridBackup") => caps.extend(["backup.snapshot", "backup.restore"]),
            Some("CloudMount") => caps.push("cloud.mount"),
            Some("TieredStorage") => caps.push("tier.storage"),
            _ => {}
        }
        for access in folder.spec["access"].as_array().into_iter().flatten() {
            match access.as_str() {
                Some("Smb") => caps.push("protocol.smb"),
                Some("Nfs") => caps.push("protocol.nfs"),
                Some("WebDav") => caps.push("protocol.webdav"),
                _ => {}
            }
        }
        if caps.iter().any(|cap| {
            record.manifest["capabilities"]
                .as_array()
                .is_some_and(|c| c.iter().any(|c| c == cap))
                && defaults.get(*cap).is_none_or(|provider| provider == id)
        }) {
            dependent.push(folder.resource.id);
        }
    }
    Ok(dependent)
}
fn refers(value: &Value, id: &str) -> bool {
    match value {
        Value::String(s) => s == id,
        Value::Array(a) => a.iter().any(|v| refers(v, id)),
        Value::Object(o) => o.values().any(|v| refers(v, id)),
        _ => false,
    }
}
pub fn removable(app: &App, id: &str) -> Result<(), Status> {
    let record = app.subsystems.get(id).map_err(error)?;
    if let Some(record) = &record {
        if record.class != "Extension" {
            return Err(error("extension.not_removable"));
        }
        if !dependencies(app, id)?.is_empty() {
            return Err(error("extension.in_use"));
        }
    } else {
        if find(app, id)?.is_none() {
            return Err(Status::not_found("extension.not_found"));
        }
        if fd_subsystems::capabilities::defaults(app.store.db())
            .map_err(error)?
            .values()
            .any(|provider| provider == id)
            && !rows(app, ResourceKind::ManagedFolder)?.is_empty()
        {
            return Err(error("extension.in_use"));
        }
    }
    for kind in [
        ResourceKind::ConnectorRoutes,
        ResourceKind::CloudConnection,
        ResourceKind::ManagedFolder,
        ResourceKind::ContainerApp,
    ] {
        for row in rows(app, kind)? {
            if refers(&row.spec, id) {
                return Err(error("extension.in_use"));
            }
        }
    }
    Ok(())
}
pub fn inspect(app: &App, path: &str) -> Result<(Package, Vec<u8>), Status> {
    let path = Path::new(path);
    if path.parent() != Some(app.config.extension_inbox.as_path())
        || path.extension().and_then(|s| s.to_str()) != Some("fdext")
    {
        return Err(error("extension.staged_upload_required"));
    }
    let meta = std::fs::symlink_metadata(path).map_err(|_| error("extension.upload_missing"))?;
    if !meta.is_file() || meta.len() > fd_subsystems::package::MAX_PACKAGE as u64 {
        return Err(error("extension.upload_invalid"));
    }
    let bytes = std::fs::read(path).map_err(error)?;
    let package = read_approved(app, &bytes, None)?;
    if path.file_stem().and_then(|v| v.to_str()) != Some(package.hash.as_str()) {
        return Err(error("extension.upload_hash_mismatch"));
    }
    validate_runtime(&package)?;
    Ok((package, bytes))
}
fn validate_runtime(package: &Package) -> Result<(), Status> {
    let r = &package.manifest.raw["runtime"];
    fd_subsystems::runtime::runtime_env(&package.manifest).map_err(error)?;
    match r["type"].as_str() {
        Some("native-process") => {
            let name = r["executable"]
                .as_str()
                .and_then(|n| n.strip_prefix("./"))
                .ok_or_else(|| error("extension.executable_must_be_packaged"))?;
            fd_subsystems::package::relative(name).map_err(error)?;
            if !package.executable.contains(name) {
                return Err(error("extension.executable_missing"));
            }
        }
        Some("systemd-service") => {
            let name = r["unit"]
                .as_str()
                .ok_or_else(|| error("extension.unit_missing"))?;
            let content = std::str::from_utf8(
                package
                    .files
                    .get(name)
                    .ok_or_else(|| error("extension.unit_missing"))?,
            )
            .map_err(|_| error("extension.unit_invalid"))?;
            let command = fd_subsystems::runtime::service_command(content).map_err(error)?;
            if !package
                .executable
                .contains(command[0].strip_prefix("./").unwrap())
            {
                return Err(error("extension.executable_missing"));
            }
        }
        Some("docker-service") => {
            let image = r["image"].as_str().unwrap_or("");
            let digest = image
                .rsplit_once("@sha256:")
                .map(|(_, d)| d)
                .or_else(|| image.strip_prefix("sha256:"))
                .ok_or_else(|| error("extension.image_must_be_pinned"))?;
            if digest.len() != 64 || !digest.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(error("extension.image_must_be_pinned"));
            }
        }
        _ => return Err(error("extension.runtime_invalid")),
    }
    Ok(())
}
pub fn socket(app: &App, id: &str) -> PathBuf {
    if let Some(root) = std::env::var_os("FD_EXTENSION_RUN") {
        PathBuf::from(root).join(id).join("subsystem.sock")
    } else {
        let _ = app;
        PathBuf::from(format!("/run/filedock-extension-{id}/subsystem.sock"))
    }
}
pub async fn plan(app: &App, spec: &Value, rotate: bool) -> Result<Runtime, Status> {
    let id = spec["subsystemId"]
        .as_str()
        .ok_or_else(|| error("extension.id_invalid"))?;
    let record = app
        .subsystems
        .get(id)
        .map_err(error)?
        .ok_or_else(|| error("extension.manifest_missing"))?;
    let manifest =
        fd_subsystems::manifest::parse_manifest(&record.manifest.to_string()).map_err(error)?;
    let data: BTreeMap<String, String> =
        serde_json::from_value(spec["dataBindings"].clone()).map_err(error)?;
    let folders = folder_map(app)?;
    let bindings = data
        .iter()
        .map(|(name, id)| {
            folders
                .get(id)
                .cloned()
                .map(|b| (name.clone(), b))
                .ok_or_else(|| error("extension.folder_missing"))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    let sandbox = Sandbox::derive(&manifest, &bindings).map_err(error)?;
    let runtime = socket(app, id).parent().unwrap().to_path_buf();
    let env_file = app
        .config
        .socket
        .parent()
        .unwrap()
        .join("extension-env")
        .join(format!("{id}.env"));
    let token = if !rotate {
        std::fs::read_to_string(&env_file).ok().and_then(|s| {
            s.lines()
                .find_map(|l| l.strip_prefix("FD_EXTENSION_TOKEN=").map(str::to_owned))
        })
    } else {
        None
    }
    .unwrap_or_else(|| {
        format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        )
    });
    let mut permissions = spec["approvedPermissions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect::<std::collections::BTreeSet<_>>();
    let mut secrets_map = BTreeMap::new();
    let vault = app.secrets.lock().await;
    let consumed = vault
        .consumers_of(id)
        .into_iter()
        .map(|(r, _)| r)
        .collect::<std::collections::BTreeSet<_>>();
    let mut refs = permissions
        .iter()
        .filter_map(|p| p.strip_prefix("secrets:"))
        .map(|name| {
            spec["config"]["secretRefs"][name]
                .as_str()
                .unwrap_or(name)
                .to_owned()
        })
        .collect::<Vec<_>>();
    if refs.iter().any(|r| !consumed.contains(r)) {
        return Err(error("extension.secret_not_granted"));
    }
    if permissions.contains("cloud.credentials") {
        for row in rows(app, ResourceKind::CloudConnection)? {
            if routed_to(app, &row, id)?
                && let Ok(fd_state::resources::ResourceSpec::CloudConnection(connection)) =
                    row.parsed_spec()
            {
                match connection.definition {
                    fd_state::resources::ConnectionDefinition::S3(s) => {
                        refs.extend([s.access_key_ref, s.secret_key_ref])
                    }
                    fd_state::resources::ConnectionDefinition::Bound(b) => {
                        refs.extend(b.credentials)
                    }
                }
            }
        }
    }
    refs.sort();
    refs.dedup();
    let materialized =
        fd_secrets::materialize(&vault, &app.config.secrets_run, id, &refs).map_err(error)?;
    drop(vault);
    for (reference, file) in refs.into_iter().zip(materialized.files) {
        secrets_map.insert(reference, file);
    }
    let mut environment = fd_subsystems::runtime::runtime_env(&manifest).map_err(error)?;
    environment.insert("FD_EXTENSION_TOKEN".into(), token.clone());
    environment.insert("FD_EXTENSION_ID".into(), manifest.id.clone());
    environment.insert("FD_EXTENSION_VERSION".into(), manifest.version.clone());
    environment.insert(
        "FD_CORE_SOCKET".into(),
        format!("{}/bridge.sock", runtime.display()),
    );
    environment.insert(
        "FD_EXTENSION_SOCKET".into(),
        socket(app, id).display().to_string(),
    );
    let text = environment
        .iter()
        .map(|(k, v)| format!("{k}=\"{}\"\n", v.replace('\\', "\\\\").replace('"', "\\\"")))
        .collect::<String>(); // Token is also stored without quotes for reliable restart adoption.
    let text = text.replace(
        &format!("FD_EXTENSION_TOKEN=\"{token}\""),
        &format!("FD_EXTENSION_TOKEN={token}"),
    );
    crate::runtime::write_private(&env_file, text.as_bytes()).map_err(error)?;
    permissions.retain(|p| !p.starts_with("storage.read:") && !p.starts_with("storage.write:"));
    for binding in bindings.values() {
        permissions.insert(format!("storage.read:{}", binding.folder_id));
        permissions.insert(format!("storage.write:{}", binding.folder_id));
    }
    if app.extension_instances.authenticate(&token, None).is_err() {
        app.extension_instances.issue(
            &token,
            Instance {
                id: id.into(),
                version: manifest.version.clone(),
                socket: socket(app, id).display().to_string(),
                capabilities: manifest.capabilities.iter().cloned().collect(),
                permissions,
                registered: false,
            },
        );
    }
    Ok(Runtime {
        payload: app.config.extension_root.join("runtimes").join(
            spec["packageSha256"]
                .as_str()
                .ok_or_else(|| error("extension.hash_missing"))?,
        ),
        manifest,
        sandbox,
        runtime,
        env_file,
        bridge: app.config.socket.parent().unwrap().join("extensions.sock"),
        token,
        secrets: secrets_map,
    })
}
fn save_registry(app: &App, package: &Package, spec: &Value) -> Result<(), Status> {
    let now = db::now_text();
    app.store.db().with(|c|{c.execute("INSERT INTO subsystem(id,class,manifest,version,approved_permissions,signature,state,enabled,package_sha256,data_bindings,updated_at) VALUES(?1,'Extension',?2,?3,?4,?5,'Starting',1,?6,?7,?8) ON CONFLICT(id) DO UPDATE SET previous_version=subsystem.version,manifest=excluded.manifest,version=excluded.version,approved_permissions=excluded.approved_permissions,signature=excluded.signature,state='Starting',package_sha256=excluded.package_sha256,data_bindings=excluded.data_bindings,updated_at=excluded.updated_at",rusqlite::params![package.manifest.id,package.manifest.raw.to_string(),package.manifest.version,spec["approvedPermissions"].to_string(),spec["signature"].to_string(),package.hash,spec["dataBindings"].to_string(),now])?;Ok::<_,fd_state::DbError>(())}).map_err(error)
}
async fn configure(app: &App, plan: &Runtime, spec: &Value) -> Result<(), Status> {
    let paths = plan.manifest.raw["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|d| Some((d["name"].as_str()?.into(), d["mountPath"].as_str()?.into())))
        .collect();
    let secrets = plan
        .secrets
        .keys()
        .map(|name| {
            (
                name.clone(),
                format!(
                    "{}/secrets/{}",
                    plan.runtime.display(),
                    plan.secrets[name].file_name().unwrap().to_string_lossy()
                ),
            )
        })
        .collect();
    if let Some(schema) = plan.manifest.raw["configSchema"].as_str() {
        let value: Value =
            serde_json::from_slice(&std::fs::read(plan.payload.join(schema)).map_err(error)?)
                .map_err(error)?;
        let validator = jsonschema_validate(&value)?;
        if !validator.is_valid(&spec["config"]) {
            return Err(error("extension.configuration_invalid"));
        }
    }
    plan.remote()
        .configure_full(spec["config"].clone(), secrets, paths)
        .await
        .map_err(error)?;
    let _ = app;
    Ok(())
}
fn jsonschema_validate(schema: &Value) -> Result<jsonschema::Validator, Status> {
    jsonschema::validator_for(schema).map_err(|_| error("extension.configuration_schema_invalid"))
}
async fn activate(app: &App, spec: &Value) -> Result<(), Status> {
    let existing = plan(app, spec, false).await?;
    if existing.health().await {
        let description = existing.remote().describe().await.map_err(error)?;
        if description.version == existing.manifest.version
            && description
                .capabilities
                .iter()
                .all(|c| existing.manifest.capabilities.contains(c))
        {
            app.extension_instances.register(
                &existing.token,
                &fd_api::extension::v1::RegisterRequest {
                    subsystem_id: existing.manifest.id.clone(),
                    version: existing.manifest.version.clone(),
                    socket_path: existing.remote().socket.display().to_string(),
                    capabilities: description.capabilities.clone(),
                },
            )?;
            app.subsystems
                .record_capabilities(&existing.manifest.id, &description.capabilities)
                .map_err(error)?;
            configure(app, &existing, spec).await?;
            return Ok(());
        }
    }
    if existing.active().await {
        existing.stop().await.map_err(error)?;
    }
    let plan = plan(app, spec, true).await?;
    plan.start().await.map_err(error)?;
    let seconds = plan.manifest.raw["health"]["startupGraceSeconds"]
        .as_u64()
        .unwrap_or(60)
        .max(5);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(seconds);
    loop {
        if app.extension_instances.registered(&plan.manifest.id) && plan.health().await {
            let description = plan.remote().describe().await.map_err(error)?;
            if description.version != plan.manifest.version
                || description
                    .capabilities
                    .iter()
                    .any(|c| !plan.manifest.capabilities.contains(c))
            {
                return Err(error("extension.description_mismatch"));
            }
            configure(app, &plan, spec).await?;
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(error("extension.health_failed"));
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}
pub async fn resume(app: &Arc<App>, id: &str) -> Result<(), Status> {
    let operation = app.journal.get(id).map_err(error)?;
    let gate = lock(
        app,
        operation.params["spec"]["subsystemId"]
            .as_str()
            .unwrap_or(""),
    );
    let _guard = gate.lock().await;
    if operation.state.is_terminal() {
        return Ok(());
    }
    if operation.state == fd_journal::OperationState::RollingBack {
        return rollback(app, &operation).await;
    }
    if let Some(current) = find(
        app,
        operation.params["spec"]["subsystemId"]
            .as_str()
            .unwrap_or(""),
    )? {
        let stale = operation.resource.as_ref() != Some(&current.resource)
            || (!operation.params["previous"].is_null()
                && current.spec != operation.params["previous"]
                && current.spec != operation.params["spec"]);
        if stale {
            app.journal
                .set_state(
                    id,
                    fd_journal::OperationState::Failed,
                    Some("extension.configuration_changed"),
                )
                .map_err(error)?;
            return Err(error("extension.configuration_changed"));
        }
    }
    let result = run(app, &operation).await;
    if let Err(e) = result {
        let previous = operation.params["previous"].clone();
        let new = operation.params["spec"].clone();
        if operation.kind == "UpgradeExtension" && !previous.is_null() {
            app.journal
                .set_state(
                    id,
                    fd_journal::OperationState::RollingBack,
                    Some(e.message()),
                )
                .map_err(error)?;
            return rollback(app, &operation).await;
        }
        if operation.kind == "InstallExtension" && previous.is_null() {
            if let Ok(runtime) = plan(app, &new, false).await {
                let _ = runtime.remove().await;
            }
            let subsystem = new["subsystemId"].as_str().unwrap_or("");
            app.extension_instances.revoke(subsystem);
            let _ = app.store.db().with(|c| {
                c.execute(
                    "DELETE FROM subsystem WHERE id=?1 AND class='Extension'",
                    [subsystem],
                )
            });
        }
        app.journal
            .set_state(id, fd_journal::OperationState::Failed, Some(e.message()))
            .map_err(error)?;
        app.events
            .emit(
                "operation.failed",
                None,
                json!({"id":id,"detail":e.message()}),
            )
            .map_err(error)?;
        return Err(e);
    }
    Ok(())
}
async fn rollback(app: &Arc<App>, operation: &fd_journal::Operation) -> Result<(), Status> {
    let previous = &operation.params["previous"];
    let new = &operation.params["spec"];
    if previous.is_null() {
        return Err(error("extension.rollback_spec_missing"));
    }
    // RollingBack is persisted before effects, so a Core restart resumes the old version.
    if let Ok(runtime) = plan(app, new, false).await
        && runtime.active().await
    {
        runtime.stop().await.map_err(error)?;
    }
    app.extension_instances
        .revoke(new["subsystemId"].as_str().unwrap_or(""));
    let manifest = operation
        .params
        .get("previousManifest")
        .ok_or_else(|| error("extension.rollback_manifest_missing"))?;
    restore_registry(app, previous, manifest)?;
    activate(app, previous).await?;
    // A crash may happen after committing the new spec but before completing the step.
    if let Some(reference) = &operation.resource
        && let Some(current) = app.store.get(reference).map_err(error)?
        && current.spec != *previous
    {
        if current.spec != *new {
            return Err(error("extension.configuration_changed"));
        }
        app.store
            .apply(
                ChangeOrigin::System {
                    reason: format!("extension-rollback:{}", operation.id),
                },
                &[Change::Upsert {
                    resource: reference.clone(),
                    expected_generation: current.generation,
                    spec: previous.clone(),
                }],
            )
            .map_err(error)?;
    }
    app.store
        .db()
        .with(|c| {
            c.execute(
                "UPDATE subsystem SET state='Running' WHERE id=?1",
                [previous["subsystemId"].as_str().unwrap_or("")],
            )
        })
        .map_err(error)?;
    app.journal
        .set_state(
            &operation.id,
            fd_journal::OperationState::RolledBack,
            Some("extension.upgrade_health_failed"),
        )
        .map_err(error)?;
    app.events
        .emit(
            "operation.rolled_back",
            None,
            json!({"id":operation.id,"subsystemId":previous["subsystemId"]}),
        )
        .map_err(error)?;
    Ok(())
}
fn mark_verified(root: &Path, hash: &str) -> Result<(), Status> {
    use std::io::Write;
    let mut marker = std::fs::File::create(root.join(".verified")).map_err(error)?;
    marker.write_all(hash.as_bytes()).map_err(error)?;
    marker.sync_all().map_err(error)?;
    std::fs::File::open(root)
        .and_then(|directory| directory.sync_all())
        .map_err(error)
}
fn restore_registry(app: &App, spec: &Value, manifest: &Value) -> Result<(), Status> {
    app.store.db().with(|c|{c.execute("UPDATE subsystem SET manifest=?1,version=?2,approved_permissions=?3,signature=?4,package_sha256=?5,data_bindings=?6,state='Starting' WHERE id=?7",rusqlite::params![manifest.to_string(),spec["version"].as_str(),spec["approvedPermissions"].to_string(),spec["signature"].to_string(),spec["packageSha256"].as_str(),spec["dataBindings"].to_string(),spec["subsystemId"].as_str()])?;Ok::<_,fd_state::DbError>(())}).map_err(error)
}
async fn run(app: &Arc<App>, op: &fd_journal::Operation) -> Result<(), Status> {
    for step in &op.steps {
        if step.state == "done" {
            continue;
        }
        app.journal.begin_step(&op.id, &step.name).map_err(error)?;
        app.events
            .emit(
                "operation.progressed",
                op.resource.clone(),
                json!({"id":op.id,"step":step.name,"state":"acting"}),
            )
            .map_err(error)?;
        match step.name.as_str() {
            "stage" => {
                let hash = op.params["spec"]["packageSha256"]
                    .as_str()
                    .ok_or_else(|| error("extension.hash_missing"))?;
                let dek = app.secrets.lock().await.dek_bytes();
                let bytes = fd_appstate::packages::load(&app.config.extension_root, hash, &dek)
                    .map_err(error)?
                    .ok_or_else(|| error("extension.package_needed"))?;
                let package = read_approved(app, &bytes, Some(&op.params["spec"]))?;
                let root = app.config.extension_root.join("runtimes").join(hash);
                if !root.join(".verified").is_file() {
                    if root.exists() {
                        std::fs::remove_dir_all(&root).map_err(error)?;
                    }
                    package.extract_verified(&root).map_err(error)?;
                    mark_verified(&root, hash)?;
                }
                save_registry(app, &package, &op.params["spec"])?;
            }
            "activate" => {
                if !op.params["previous"].is_null()
                    && let Ok(previous) = plan(app, &op.params["previous"], false).await
                    && previous.active().await
                {
                    previous.stop().await.map_err(error)?;
                }
                activate(app, &op.params["spec"]).await?;
            }
            "commit" => {
                if !plan(app, &op.params["spec"], false).await?.health().await {
                    activate(app, &op.params["spec"]).await?;
                }
                let reference = op
                    .resource
                    .clone()
                    .ok_or_else(|| error("extension.resource_missing"))?;
                let current = app.store.get(&reference).map_err(error)?;
                if current.as_ref().is_none_or(|r| r.spec != op.params["spec"]) {
                    app.store
                        .apply(
                            ChangeOrigin::System {
                                reason: format!("extension-operation:{}", op.id),
                            },
                            &[Change::Upsert {
                                resource: reference,
                                expected_generation: current.map(|r| r.generation).unwrap_or(0),
                                spec: op.params["spec"].clone(),
                            }],
                        )
                        .map_err(error)?;
                }
                let subsystem = op.params["spec"]["subsystemId"].as_str().unwrap();
                app.store
                    .db()
                    .with(|c| {
                        c.execute(
                            "UPDATE subsystem SET state='Running' WHERE id=?1",
                            [subsystem],
                        )
                    })
                    .map_err(error)?;
            }
            "remove" => {
                let subsystem = op.params["spec"]["subsystemId"].as_str().unwrap();
                if find(app, subsystem)?.is_none()
                    && app.subsystems.get(subsystem).map_err(error)?.is_none()
                {
                    app.journal
                        .finish_step(&op.id, &step.name, Ok(json!({"alreadyRemoved":true})))
                        .map_err(error)?;
                    continue;
                }
                removable(app, subsystem)?;
                if app.subsystems.get(subsystem).map_err(error)?.is_some() {
                    plan(app, &op.params["spec"], false)
                        .await?
                        .remove()
                        .await
                        .map_err(error)?;
                }
                app.extension_instances.revoke(subsystem);
                if let Some(row) = find(app, subsystem)? {
                    app.store
                        .apply(
                            ChangeOrigin::System {
                                reason: "extension-remove".into(),
                            },
                            &[Change::Delete {
                                resource: row.resource,
                                expected_generation: row.generation,
                            }],
                        )
                        .map_err(error)?;
                }
                let mut defaults =
                    fd_subsystems::capabilities::defaults(app.store.db()).map_err(error)?;
                defaults.retain(|_, id| id != subsystem);
                app.store
                    .db()
                    .with(|c| db::put_setting(c, "capability_defaults", &json!(defaults)))
                    .map_err(error)?;
                app.store
                    .db()
                    .with(|c| {
                        c.execute(
                            "DELETE FROM subsystem WHERE id=?1 AND class='Extension'",
                            [subsystem],
                        )
                    })
                    .map_err(error)?;
            }
            _ => return Err(error("extension.step_unknown")),
        }
        app.journal
            .finish_step(&op.id, &step.name, Ok(json!({"complete":true})))
            .map_err(error)?;
    }
    app.events
        .emit("operation.completed", None, json!({"id":op.id}))
        .map_err(error)?;
    Ok(())
}
pub fn lock(app: &App, id: &str) -> Arc<tokio::sync::Mutex<()>> {
    let mut locks = app.extension_locks.lock().unwrap();
    locks.retain(|_, lock| lock.strong_count() > 0);
    if let Some(lock) = locks.get(id).and_then(std::sync::Weak::upgrade) {
        return lock;
    }
    let lock = Arc::new(tokio::sync::Mutex::new(()));
    locks.insert(id.into(), Arc::downgrade(&lock));
    lock
}
pub async fn supervise(app: Arc<App>) {
    let mut workers: BTreeMap<String, tokio::task::JoinHandle<()>> = BTreeMap::new();
    loop {
        let rows = rows(&app, ResourceKind::Extension).unwrap_or_default();
        let present = rows
            .iter()
            .filter_map(|r| r.spec["subsystemId"].as_str())
            .collect::<std::collections::BTreeSet<_>>();
        workers.retain(|id, worker| {
            if !present.contains(id.as_str()) {
                worker.abort();
                return false;
            }
            !worker.is_finished()
        });
        for row in rows {
            let id = row.spec["subsystemId"].as_str().unwrap_or("").to_owned();
            if workers.contains_key(&id) {
                continue;
            }
            let app = app.clone();
            let worker_id = id.clone();
            workers.insert(
                id,
                tokio::spawn(async move {
                    loop {
                        let Some(row) = find(&app, &worker_id).ok().flatten() else {
                            return;
                        };
                        let gate = lock(&app, &worker_id);
                        let mut delay = Duration::from_secs(1);
                        if let Ok(_guard) = gate.try_lock()
                            && let Some(record) = app.subsystems.get(&worker_id).ok().flatten()
                        {
                            let binding_ready = record.state != "AwaitingDataBindings"
                                || row.spec["dataBindings"]
                                    .as_object()
                                    .into_iter()
                                    .flatten()
                                    .all(|(_, id)| {
                                        id.as_str()
                                            .and_then(|id| folder_map(&app).ok()?.get(id).cloned())
                                            .is_some_and(|b| Path::new(&b.source).is_dir())
                                    });
                            if record.state != "PackageNeeded" && binding_ready {
                                let adapter = ExtensionAdapter {
                                    app: app.clone(),
                                    spec: row.spec,
                                };
                                let _ = app
                                    .supervisor
                                    .inspect(&adapter, chrono::Utc::now().timestamp_millis())
                                    .await;
                                let supervision =
                                    app.supervisor.status(&worker_id).unwrap_or_default();
                                if supervision.state == "Running" {
                                    delay = Duration::from_secs(
                                        record.manifest["health"]["intervalSeconds"]
                                            .as_u64()
                                            .unwrap_or(30)
                                            .clamp(5, 3600),
                                    );
                                }
                            }
                        }
                        tokio::time::sleep(delay).await;
                    }
                }),
            );
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}
struct ExtensionAdapter {
    app: Arc<App>,
    spec: Value,
}
#[async_trait::async_trait]
impl SubsystemAdapter for ExtensionAdapter {
    fn id(&self) -> &str {
        self.spec["subsystemId"].as_str().unwrap_or("")
    }
    fn startup_grace(&self) -> Duration {
        let seconds = self
            .app
            .subsystems
            .get(self.id())
            .ok()
            .flatten()
            .and_then(|r| r.manifest["health"]["startupGraceSeconds"].as_u64())
            .unwrap_or(60)
            .min(3600);
        Duration::from_secs(seconds)
    }
    async fn describe(
        &self,
    ) -> Result<fd_subsystems::adapter::Description, fd_subsystems::AdapterError> {
        plan(&self.app, &self.spec, false)
            .await
            .map_err(|e| fd_subsystems::AdapterError::Failed(e.message().into()))?
            .remote()
            .describe()
            .await
    }
    async fn state(&self) -> Result<Value, fd_subsystems::AdapterError> {
        Ok(self.spec.clone())
    }
    async fn configure(&self, _: Value) -> Result<(), fd_subsystems::AdapterError> {
        Ok(())
    }
    async fn start(&self) -> Result<(), fd_subsystems::AdapterError> {
        let runtime = plan(&self.app, &self.spec, true)
            .await
            .map_err(|e| fd_subsystems::AdapterError::Failed(e.message().into()))?;
        if runtime.active().await {
            runtime.stop().await?;
        }
        runtime.start().await
    }
    async fn stop(&self) -> Result<(), fd_subsystems::AdapterError> {
        plan(&self.app, &self.spec, false)
            .await
            .map_err(|e| fd_subsystems::AdapterError::Failed(e.message().into()))?
            .stop()
            .await
    }
    async fn health(&self) -> Result<fd_subsystems::adapter::Health, fd_subsystems::AdapterError> {
        let plan = plan(&self.app, &self.spec, false)
            .await
            .map_err(|e| fd_subsystems::AdapterError::Failed(e.message().into()))?;
        Ok(fd_subsystems::adapter::Health {
            healthy: plan.health().await,
            detail: "Extension RPC and runtime health".into(),
        })
    }
    async fn diagnostics(&self) -> Result<Value, fd_subsystems::AdapterError> {
        let runtime = plan(&self.app, &self.spec, false)
            .await
            .map_err(|e| fd_subsystems::AdapterError::Failed(e.message().into()))?;
        let data = runtime.remote().diagnostics().await?;
        Ok(self.app.secrets.lock().await.redact_observed(data))
    }
}
pub fn routed_to(app: &App, row: &StoredResource, id: &str) -> Result<bool, Status> {
    use fd_state::resources::{ResourceSpec, RouteTarget, RouteUse};
    let Some(routes) = app
        .store
        .get(&fd_state::ResourceRef::singleton(
            ResourceKind::ConnectorRoutes,
        ))
        .map_err(error)?
    else {
        return Ok(false);
    };
    let ResourceSpec::ConnectorRoutes(routes) =
        routes.parsed_spec().map_err(|_| error("route.invalid"))?
    else {
        return Ok(false);
    };
    let ResourceSpec::CloudConnection(connection) =
        row.parsed_spec().map_err(|_| error("connection.invalid"))?
    else {
        return Ok(false);
    };
    Ok([
        RouteUse::OwnedObjectStorage,
        RouteUse::BackupRepository,
        RouteUse::CloudMount,
    ]
    .into_iter()
    .any(|use_kind| {
        use_kind.allowed_for_family(&connection.provider_family)
            && fd_connectors::routes::resolve(
                &routes,
                Some(&connection),
                &connection.provider_family,
                use_kind,
            )
            .is_ok_and(
                |target| matches!(target,RouteTarget::Connector{subsystem_id}if subsystem_id==id),
            )
    }))
}
pub async fn configure_connectors(app: &App) -> Result<(), Status> {
    for extension in rows(app, ResourceKind::Extension)? {
        let result = async {
            let id = extension.spec["subsystemId"].as_str().unwrap_or("");
            let gate = lock(app, id);
            let Ok(_guard) = gate.try_lock() else {
                return Ok::<(), Status>(());
            };
            let Some(record) = app.subsystems.get(id).map_err(error)? else {
                return Ok::<(), Status>(());
            };
            if !record.enabled
                || !record.manifest["permissions"]
                    .as_array()
                    .is_some_and(|p| p.iter().any(|p| p == "cloud.credentials"))
            {
                return Ok::<(), Status>(());
            }
            let connections = rows(app, ResourceKind::CloudConnection)?
                .into_iter()
                .filter_map(|row| match routed_to(app, &row, id) {
                    Ok(true) => Some(Ok(json!({"id":row.resource.id,"spec":row.spec}))),
                    Ok(false) => None,
                    Err(e) => Some(Err(e)),
                })
                .collect::<Result<Vec<_>, Status>>()?;
            let runtime = plan(app, &extension.spec, false).await?;
            // New credential files stay inside the mounted per-extension tmpfs directory.
            for file in runtime.secrets.values() {
                let result = tokio::process::Command::new("setfacl")
                    .args([
                        "-m",
                        &format!("u:{}:r", runtime.user()),
                        &file.display().to_string(),
                    ])
                    .kill_on_drop(true)
                    .status()
                    .await
                    .map_err(error)?;
                if !result.success() {
                    return Err(error("extension.secret_grant_failed"));
                }
            }
            let secrets = runtime
                .secrets
                .iter()
                .map(|(name, file)| {
                    (
                        name.clone(),
                        format!(
                            "{}/secrets/{}",
                            runtime.runtime.display(),
                            file.file_name().unwrap().to_string_lossy()
                        ),
                    )
                })
                .collect();
            runtime
                .remote()
                .configure_full(
                    json!({"connections":connections,"userConfig":extension.spec["config"]}),
                    secrets,
                    Default::default(),
                )
                .await
                .map_err(error)?;
            Ok::<(), Status>(())
        }
        .await;
        if let Err(error) = result {
            tracing::warn!(subsystem=?extension.spec["subsystemId"],code=%error.message(),"extension connector configuration failed");
        }
    }
    Ok(())
}
pub async fn restore_package(
    app: &Arc<App>,
    row: &StoredResource,
    bytes: &[u8],
) -> Result<(), Status> {
    let hash = row.spec["packageSha256"]
        .as_str()
        .ok_or_else(|| error("extension.hash_missing"))?;
    let package = fd_subsystems::package::Package::read_recorded(
        bytes,
        &app.config.extension_keys,
        app.config.release_version,
        hash,
        row.spec["signature"]["signed"]["keyId"]
            .as_str()
            .unwrap_or(""),
    )
    .map_err(error)?;
    fd_appstate::packages::require_recorded(
        bytes,
        hash,
        row.spec["version"].as_str().unwrap_or(""),
        &package.manifest.version,
    )
    .map_err(error)?;
    let dek = app.secrets.lock().await.dek_bytes();
    fd_appstate::packages::store(&app.config.extension_root, bytes, &dek).map_err(error)?;
    let root = app.config.extension_root.join("runtimes").join(hash);
    if !root.join(".verified").is_file() {
        if root.exists() {
            std::fs::remove_dir_all(&root).map_err(error)?;
        }
        package.extract_verified(&root).map_err(error)?;
        mark_verified(&root, hash)?;
    }
    save_registry(app, &package, &row.spec)?;
    let available = folder_map(app)?
        .values()
        .filter(|b| {
            row.spec["dataBindings"]
                .as_object()
                .is_some_and(|m| m.values().any(|id| id == &b.folder_id))
        })
        .all(|b| Path::new(&b.source).is_dir());
    let state = if !available {
        "AwaitingDataBindings"
    } else if std::env::var("FD_RUNTIME_ENABLED").as_deref() == Ok("0") {
        "AwaitingRuntime"
    } else {
        activate(app, &row.spec).await?;
        "Running"
    };
    app.store
        .db()
        .with(|c| {
            c.execute(
                "UPDATE subsystem SET state=?1 WHERE id=?2",
                rusqlite::params![state, package.manifest.id],
            )
        })
        .map_err(error)?;
    Ok(())
}
fn read_approved(app: &App, bytes: &[u8], approved: Option<&Value>) -> Result<Package, Status> {
    use sha2::{Digest, Sha256};
    let hash = hex::encode(Sha256::digest(bytes));
    let known = approved
        .filter(|s| s["packageSha256"] == hash)
        .cloned()
        .or_else(|| {
            rows(app, ResourceKind::Extension)
                .ok()?
                .into_iter()
                .find(|r| r.spec["packageSha256"] == hash)
                .map(|r| r.spec)
        });
    if let Some(spec) = known {
        Package::read_recorded(
            bytes,
            &app.config.extension_keys,
            app.config.release_version,
            &hash,
            spec["signature"]["signed"]["keyId"].as_str().unwrap_or(""),
        )
        .map_err(error)
    } else {
        Package::read(
            bytes,
            &app.config.extension_keys,
            app.config.release_version,
        )
        .map_err(error)
    }
}
