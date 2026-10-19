//! Runtime wiring for the Phase 2 built-in connectors.
use crate::{api::App, proto_conv::value_to_struct};
use fd_api::extension::v1 as ext;
use fd_connectors::routes::resolve;
use fd_state::{
    ResourceKind, ResourceRef,
    resources::{ConnectionDefinition, ResourceSpec, RouteTarget, RouteUse},
};
use serde_json::json;
use std::{collections::BTreeMap, path::Path, sync::Arc, time::Duration};
use tonic::{
    Status,
    transport::{Channel, Endpoint},
};

pub async fn channel(path: impl AsRef<Path>) -> Result<Channel, Status> {
    let path = path.as_ref().to_path_buf();
    Endpoint::from_static("http://localhost")
        .timeout(Duration::from_secs(30))
        .connect_with_connector(tower::service_fn(move |_| {
            let path = path.clone();
            async move {
                tokio::net::UnixStream::connect(path)
                    .await
                    .map(hyper_util::rt::TokioIo::new)
            }
        }))
        .await
        .map_err(|_| Status::unavailable("connector.unavailable"))
}

pub fn s3_socket(app: &App) -> std::path::PathBuf {
    app.config
        .socket
        .parent()
        .unwrap_or(Path::new("/run/filedock"))
        .join("fd-s3.sock")
}

pub async fn configure(app: &App) -> Result<(), Status> {
    let _configuration = app.connector_config_lock.lock().await;
    let routes_row = app
        .store
        .get(&ResourceRef::singleton(ResourceKind::ConnectorRoutes))
        .map_err(internal)?
        .ok_or_else(|| Status::not_found("ConnectorRoutes"))?;
    let ResourceSpec::ConnectorRoutes(routes) = routes_row
        .parsed_spec()
        .map_err(|_| Status::internal("invalid routes"))?
    else {
        return Err(Status::internal("invalid routes"));
    };
    let rows = app
        .store
        .list(ResourceKind::CloudConnection, None, usize::MAX / 2)
        .map_err(internal)?;
    let mut recipients: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut s3 = Vec::new();
    let mut rclone_connections = Vec::new();
    for row in rows {
        let ResourceSpec::CloudConnection(connection) = row
            .parsed_spec()
            .map_err(|_| Status::internal("invalid connection"))?
        else {
            continue;
        };
        let refs = match &connection.definition {
            ConnectionDefinition::S3(def) => {
                vec![def.access_key_ref.clone(), def.secret_key_ref.clone()]
            }
            ConnectionDefinition::Bound(def) => def.credentials.clone(),
        };
        let mut targets = std::collections::BTreeSet::new();
        let uses = connection_uses(app, &row.resource.id)?;
        for use_kind in uses {
            if !use_kind.allowed_for_family(&connection.provider_family) {
                continue;
            }
            if let Ok(target) = resolve(
                &routes,
                Some(&connection),
                &connection.provider_family,
                use_kind,
            ) {
                targets.insert(match target {
                    RouteTarget::Connector { subsystem_id } => subsystem_id,
                    RouteTarget::BackupEngine => "backup-engine".into(),
                });
            }
        }
        for target in &targets {
            recipients
                .entry(target.clone())
                .or_default()
                .extend(refs.clone());
        }
        if targets.contains("fd-s3")
            && let ConnectionDefinition::S3(def) = &connection.definition
        {
            s3.push(json!({"id": row.resource.id, "endpoint": def.endpoint, "region": def.region, "bucket": def.bucket,
                    "virtual_hosted": def.addressing_style == fd_state::resources::AddressingStyle::Virtual,
                    "access_key_ref": def.access_key_ref, "secret_key_ref": def.secret_key_ref, "checksums": proven_checksums(&row,"fd-s3"), "storage_class":def.storage_class,"kms_key_id":match &def.server_side_encryption {Some(fd_state::resources::ServerSideEncryption::Kms{key_id})=>Some(key_id),_=>None},"bandwidth_limit":def.bandwidth_limit}));
        }
        if targets.contains("rclone") {
            rclone_connections.push((row.resource.id, connection));
        }
    }
    // rclone may rotate OAuth credentials during an operation. Persist the latest token in the
    // encrypted vault before deriving the next tmpfs configuration, so restart cannot restore an old token.
    if app
        .supervisor
        .status("rclone")
        .is_ok_and(|s| s.state == "Running")
    {
        let adapter = fd_connectors::rclone::RcloneAdapter::with_socket(
            app.config.socket.parent().unwrap().join("rclone.sock"),
        );
        for (id, connection) in &rclone_connections {
            if let ConnectionDefinition::Bound(bound) = &connection.definition
                && let Ok(remote) = fd_connectors::oauth::credentials(&adapter, id).await
                && let Some(token) = remote["token"].as_str()
            {
                let mut vault = app.secrets.lock().await;
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
                        app.store
                            .db()
                            .with(|c| {
                                fd_state::db::put_setting(
                                    c,
                                    "config.changed_at",
                                    &json!(fd_state::db::now_text()),
                                )
                            })
                            .map_err(internal)?;
                    }
                }
            }
        }
    }
    let secrets = app.secrets.lock().await;
    let mut secret_files = BTreeMap::new();
    for target in ["fd-s3", "rclone", "backup-engine"] {
        let references = recipients.get(target).cloned().unwrap_or_default();
        let files = fd_secrets::materialize(&secrets, &app.config.secrets_run, target, &references)
            .map_err(internal)?;
        if target == "fd-s3" {
            for (reference, path) in references.iter().zip(files.files) {
                secret_files.insert(reference.clone(), path.to_string_lossy().into_owned());
            }
        }
    }
    let mut remotes = fd_connectors::rclone::conf::remotes_for(&rclone_connections);
    for remote in &mut remotes {
        let spec = &rclone_connections
            .iter()
            .find(|(id, _)| *id == remote.name)
            .unwrap()
            .1;
        if let ConnectionDefinition::S3(def) = &spec.definition {
            remote.lines.push((
                "access_key_id".into(),
                String::from_utf8(secrets.get(&def.access_key_ref).map_err(internal)?.to_vec())
                    .map_err(internal)?,
            ));
            remote.lines.push((
                "secret_access_key".into(),
                String::from_utf8(secrets.get(&def.secret_key_ref).map_err(internal)?.to_vec())
                    .map_err(internal)?,
            ));
        }
        if let ConnectionDefinition::Bound(def) = &spec.definition {
            for reference in &def.credentials {
                let bytes = secrets.get(reference).map_err(internal)?;
                let credentials: serde_json::Value =
                    serde_json::from_slice(&bytes).map_err(internal)?;
                if let Some(values) = credentials.as_object() {
                    for (key, value) in values {
                        if key == "type" {
                            continue;
                        }
                        if let Some(text) = value.as_str() {
                            if key.contains(['\n', '\r', '=']) || text.contains(['\n', '\r']) {
                                return Err(Status::invalid_argument("connection.config_invalid"));
                            }
                            remote.lines.push((key.clone(), text.to_owned()));
                        }
                    }
                }
            }
        }
    }
    drop(secrets);
    let conf = fd_connectors::rclone::conf::render_conf(&remotes);
    let path = app.config.socket.parent().unwrap().join("rclone.conf");
    let previous = std::fs::read_to_string(&path).unwrap_or_default();
    let changed = previous != conf;
    if changed {
        if app
            .supervisor
            .status("rclone")
            .is_ok_and(|s| s.state == "Running")
        {
            let adapter = fd_connectors::rclone::RcloneAdapter::with_socket(
                app.config.socket.parent().unwrap().join("rclone.sock"),
            );
            for remote in &remotes {
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
                adapter
                    .configure_remote(&remote.name, kind, parameters)
                    .await
                    .map_err(internal)?;
            }
            for old in previous
                .lines()
                .filter_map(|l| l.strip_prefix('[').and_then(|l| l.strip_suffix(']')))
            {
                if !remotes.iter().any(|r| r.name == old) {
                    adapter
                        .delete_recovery_remote(old)
                        .await
                        .map_err(internal)?;
                }
            }
        }
        // Active session aliases remain in the running daemon; only desired routes persist for reboot.
        write_private(&path, conf.as_bytes()).map_err(internal)?;
    }
    app.temporary_s3
        .lock()
        .await
        .retain(|_, (_, files)| files.values().all(|p| Path::new(p).is_file()));
    if let Err(error) = crate::extension_runtime::configure_connectors(app).await {
        tracing::warn!(code=%error.message(),"extension connector configuration failed");
    }
    if app
        .subsystems
        .get("fd-s3")
        .map_err(internal)?
        .is_some_and(|s| !s.enabled)
    {
        return Ok(());
    }
    if !app
        .supervisor
        .status("fd-s3")
        .is_ok_and(|s| s.state == "Running")
    {
        return Ok(());
    }
    {
        let mut temporary = app.temporary_s3.lock().await;
        temporary.retain(|_, (_, files)| {
            files
                .values()
                .all(|path| std::path::Path::new(path).is_file())
        });
        for (row, files) in temporary
            .values()
            .filter(|(row, _)| row["connector_id"].as_str().unwrap_or("fd-s3") == "fd-s3")
        {
            s3.push(row.clone());
            secret_files.extend(files.clone());
        }
    }
    let mut client = ext::subsystem_client::SubsystemClient::new(channel(s3_socket(app)).await?);
    let response = client
        .configure(ext::ConfigureRequest {
            config: Some(value_to_struct(&json!({"connections": s3}))),
            secret_files: secret_files.into_iter().collect(),
            data_paths: Default::default(),
        })
        .await?
        .into_inner();
    if !response.accepted {
        return Err(Status::failed_precondition(
            "connector configuration rejected",
        ));
    }
    Ok(())
}

pub async fn test_connection(
    app: &App,
    id: &str,
) -> Result<fd_api::core::v1::ConnectionTestResult, Status> {
    let reference = ResourceRef::new(ResourceKind::CloudConnection, id);
    let row = app
        .store
        .get(&reference)
        .map_err(internal)?
        .ok_or_else(|| Status::not_found("connection"))?;
    let ResourceSpec::CloudConnection(connection) = row
        .parsed_spec()
        .map_err(|_| Status::internal("invalid connection"))?
    else {
        return Err(Status::internal("invalid connection"));
    };
    let routes = app
        .store
        .get(&ResourceRef::singleton(ResourceKind::ConnectorRoutes))
        .map_err(internal)?
        .unwrap();
    let ResourceSpec::ConnectorRoutes(routes) = routes
        .parsed_spec()
        .map_err(|_| Status::internal("invalid routes"))?
    else {
        return Err(Status::internal("invalid routes"));
    };
    let connector = fd_connectors::ConnectionTester::routed_connector(
        &routes,
        &connection,
        if connection.is_object_storage() {
            RouteUse::OwnedObjectStorage
        } else {
            RouteUse::CloudMount
        },
    )
    .map_err(internal)?;
    require_connector(
        app,
        &connector,
        id,
        if connection.is_object_storage() {
            RouteUse::OwnedObjectStorage
        } else {
            RouteUse::CloudMount
        },
    )?;
    crate::state_runtime::inspect_locations(app).await?;
    if connector == "rclone" {
        return test_rclone(app, &reference, &connection).await;
    }
    let mut discovery = crate::session_runtime::native(app, &row, &connector).await?;
    let mut client = ext::cloud_remote_client::CloudRemoteClient::new(discovery.channel.clone());
    let locations = crate::state_runtime::locations(app)?;
    let registered = locations.iter().find(|l| l.connection_id == id);
    let root_prefix = registered
        .map(|l| l.prefix.trim_end_matches('/'))
        .unwrap_or("");
    discovery.prefix = if root_prefix.is_empty() {
        String::new()
    } else {
        format!("{root_prefix}/")
    };
    use fd_connectors::ownership::ObjectIo;
    let owner = discovery
        .read(fd_connectors::ownership::OWNER_KEY)
        .await
        .map_err(internal)?
        .map(|object| serde_json::from_slice::<serde_json::Value>(&object.bytes))
        .transpose()
        .map_err(|_| Status::failed_precondition("location.owner_invalid"))?;
    let read_only = crate::state_runtime::locations(app)?
        .iter()
        .any(|l| l.connection_id == id && l.role != fd_connectors::ownership::Role::Writer)
        || owner.as_ref().is_some_and(|v| {
            v.get("applianceId").and_then(|v| v.as_str())
                != Some(crate::state_runtime::appliance_id(app).as_str())
        });
    let result = client
        .probe(ext::ProbeRequest {
            connection_id: discovery.connection_id.clone(),
            prefix: format!(
                "{}{}_filedock/probe/{}/{}",
                root_prefix,
                if root_prefix.is_empty() { "" } else { "/" },
                crate::state_runtime::appliance_id(app),
                uuid::Uuid::now_v7()
            ),
            read_only,
        })
        .await?
        .into_inner();
    save_probe(app, &reference, &connector, result)
}

async fn test_rclone(
    app: &App,
    reference: &ResourceRef,
    connection: &fd_state::resources::CloudConnectionSpec,
) -> Result<fd_api::core::v1::ConnectionTestResult, Status> {
    let lease = crate::session_runtime::rclone(app, connection).await?;
    let locations = crate::state_runtime::locations(app)?;
    let registered = locations.iter().find(|l| l.connection_id == reference.id);
    let remote = if let Some(location) = registered {
        format!(
            "{}:{}",
            lease.cleanup_name.as_ref().unwrap(),
            location.bucket
        )
    } else {
        lease.remote.clone()
    };
    let root_prefix = registered
        .map(|l| format!("{}/", l.prefix.trim_end_matches('/')))
        .filter(|p| p != "/")
        .unwrap_or_default();
    let adapter = &lease.adapter;
    if !connection.is_object_storage()
        || (connection.provider_family == "azure-blob" && registered.is_none())
    {
        adapter.list(&remote, "").await.map_err(internal)?;
        crate::session_runtime::persist_token(app, connection, &lease).await?;
        return save_probe(app,reference,"rclone",ext::ProbeResult{checks:vec![ext::ProbeCheck{name:"list".into(),ok:true,detail:"Authorization verified; no write probe is performed for this provider family".into()}],write_steps_run:false,..Default::default()});
    }
    // Fail closed on any ownership read error other than a known missing object.
    let owner = adapter
        .read(&remote, &format!("{root_prefix}_filedock/owner.json"))
        .await;
    let read_only = crate::state_runtime::locations(app)?.iter().any(|l| {
        l.connection_id == reference.id && l.role != fd_connectors::ownership::Role::Writer
    }) || match owner {
        Ok(bytes) => {
            serde_json::from_slice::<serde_json::Value>(&bytes)
                .map_err(internal)?
                .get("applianceId")
                .and_then(|v| v.as_str())
                != Some(crate::state_runtime::appliance_id(app).as_str())
        }
        Err(error) if error.contains("404") || error.contains("object not found") => false,
        Err(error) => return Err(internal(error)),
    };
    let prefix = format!(
        "{root_prefix}_filedock/probe/{}/{}",
        app.appliance_id,
        uuid::Uuid::now_v7()
    );
    adapter
        .list(&remote, &root_prefix)
        .await
        .map_err(internal)?;
    let mut checks = vec![ext::ProbeCheck {
        name: "list".into(),
        ok: true,
        detail: String::new(),
    }];
    if !read_only {
        adapter
            .write_verified(&remote, &prefix, b"filedock-probe")
            .await
            .map_err(internal)?;
        adapter
            .deletefile(&remote, &prefix)
            .await
            .map_err(internal)?;
        checks.push(ext::ProbeCheck {
            name: "write-read-delete".into(),
            ok: true,
            detail: String::new(),
        });
    }
    crate::session_runtime::persist_token(app, connection, &lease).await?;
    save_probe(
        app,
        reference,
        "rclone",
        ext::ProbeResult {
            checks,
            write_steps_run: !read_only,
            ..Default::default()
        },
    )
}

fn save_probe(
    app: &App,
    reference: &ResourceRef,
    connector: &str,
    probe: ext::ProbeResult,
) -> Result<fd_api::core::v1::ConnectionTestResult, Status> {
    let now = fd_state::db::now_text();
    let stored = fd_connectors::StoredTestResult::from_probe(connector, now.clone(), &probe);
    let mut status = app
        .store
        .get(reference)
        .map_err(internal)?
        .and_then(|r| r.status)
        .unwrap_or(json!({}));
    status["lastTest"] = json!(stored);
    if !status["testsByConnector"].is_object() {
        status["testsByConnector"] = json!({});
    }
    status["testsByConnector"][connector] = json!(stored);
    app.store
        .write_status(reference, &status)
        .map_err(internal)?;
    crate::state_runtime::recompute_claim_safety(app)?;
    let at = fd_state::db::parse_timestamp(&now).unwrap();
    Ok(fd_api::core::v1::ConnectionTestResult {
        tested_at: Some(prost_types::Timestamp {
            seconds: at.timestamp(),
            nanos: 0,
        }),
        connector_id: connector.into(),
        checks: probe
            .checks
            .into_iter()
            .map(|c| fd_api::core::v1::ConnectionCheck {
                name: c.name,
                ok: c.ok,
                detail: c.detail,
            })
            .collect(),
        conditional_create: probe.conditional_create,
        conditional_replace: probe.conditional_replace,
        provider_checksums: probe.provider_checksums,
        clock_skew_ms: probe.clock_skew_ms,
    })
}

struct ConnectorHealthAdapter {
    unit: fd_subsystems::supervisor::UnitAdapter,
    app: Arc<App>,
}
#[tonic::async_trait]
impl fd_subsystems::SubsystemAdapter for ConnectorHealthAdapter {
    fn id(&self) -> &str {
        fd_subsystems::SubsystemAdapter::id(&self.unit)
    }
    async fn describe(
        &self,
    ) -> Result<fd_subsystems::adapter::Description, fd_subsystems::AdapterError> {
        self.unit.describe().await
    }
    async fn state(&self) -> Result<serde_json::Value, fd_subsystems::AdapterError> {
        self.unit.state().await
    }
    async fn configure(
        &self,
        desired: serde_json::Value,
    ) -> Result<(), fd_subsystems::AdapterError> {
        self.unit.configure(desired).await
    }
    async fn start(&self) -> Result<(), fd_subsystems::AdapterError> {
        self.unit.start().await
    }
    async fn stop(&self) -> Result<(), fd_subsystems::AdapterError> {
        self.unit.stop().await
    }
    async fn diagnostics(&self) -> Result<serde_json::Value, fd_subsystems::AdapterError> {
        self.unit.diagnostics().await
    }
    async fn health(&self) -> Result<fd_subsystems::adapter::Health, fd_subsystems::AdapterError> {
        let unit = self.unit.health().await?;
        if !unit.healthy {
            return Ok(unit);
        }
        let healthy = if self.id() == "fd-s3" {
            match channel(s3_socket(&self.app)).await {
                Ok(channel) => ext::subsystem_client::SubsystemClient::new(channel)
                    .health(ext::Empty {})
                    .await
                    .is_ok_and(|r| {
                        r.get_ref().status == ext::health_report::Status::Healthy as i32
                    }),
                Err(_) => false,
            }
        } else {
            fd_connectors::rclone::adapter::RcloneAdapter::new()
                .health()
                .await
                .is_ok()
        };
        Ok(fd_subsystems::adapter::Health {
            healthy,
            detail: if healthy {
                "serving"
            } else {
                "connector is not responding"
            }
            .into(),
        })
    }
}

pub async fn run(app: Arc<App>) {
    let conf = app.config.socket.parent().unwrap().join("rclone.conf");
    if !conf.exists() {
        let _ = write_private(&conf, b"");
    }
    if let Ok(records) = app.subsystems.list() {
        for record in records {
            let adapter: Option<Arc<dyn fd_subsystems::SubsystemAdapter>> =
                match fd_reconcile::adopt::Runtime::from_manifest(&record.id, &record.manifest) {
                    Ok(Some(fd_reconcile::adopt::Runtime::Unit(unit))) => {
                        let unit = fd_subsystems::supervisor::UnitAdapter {
                            id: record.id.clone(),
                            unit,
                        };
                        if record.id == "fd-s3" || record.id == "rclone" {
                            Some(Arc::new(ConnectorHealthAdapter {
                                unit,
                                app: app.clone(),
                            }))
                        } else {
                            Some(Arc::new(unit))
                        }
                    }
                    Ok(None) if record.id == "kopia" => Some(Arc::new(fd_subsystems::KopiaAdapter)),
                    Ok(Some(fd_reconcile::adopt::Runtime::ComposeProject(project))) => {
                        Some(Arc::new(fd_subsystems::supervisor::ContainerAdapter {
                            id: record.id,
                            project,
                        }))
                    }
                    _ => None,
                };
            if let Some(adapter) = adapter {
                tokio::spawn(app.supervisor.clone().run(adapter));
            }
        }
    }
    let mut interval = tokio::time::interval(Duration::from_secs(5));
    let mut swept_socket = None;
    let mut last_probe = std::time::Instant::now() - Duration::from_secs(86400);
    loop {
        interval.tick().await;
        if let Err(error) = configure(&app).await {
            tracing::debug!(%error, "connector configuration pending");
        }
        let s3_running = app
            .supervisor
            .status("fd-s3")
            .is_ok_and(|s| s.state == "Running");
        let socket_epoch = socket_epoch(&s3_socket(&app));
        if s3_running
            && socket_epoch.is_some()
            && (swept_socket != socket_epoch || last_probe.elapsed() >= Duration::from_secs(86400))
            && let Err(error) = crate::sweep_runtime::run(&app).await
        {
            tracing::warn!(code=%error.message(),"incomplete upload sweep failed");
        }
        if s3_running {
            swept_socket = socket_epoch;
        }
        if last_probe.elapsed() >= Duration::from_secs(86400) {
            if let Ok(rows) = app
                .store
                .list(ResourceKind::CloudConnection, None, usize::MAX / 2)
            {
                for row in rows {
                    if let Err(error) = test_connection(&app, &row.resource.id).await {
                        tracing::warn!(%error, connection = %row.resource.id, "daily connection probe failed");
                    }
                }
            }
            last_probe = std::time::Instant::now();
        }
    }
}

pub fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(&tmp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    std::fs::rename(tmp, path)
}
fn internal(error: impl std::fmt::Display) -> Status {
    Status::internal(error.to_string())
}

pub fn require_connector(
    app: &App,
    id: &str,
    connection: &str,
    usage: RouteUse,
) -> Result<(), Status> {
    let row = app.subsystems.get(id).map_err(internal)?;
    let state = app.supervisor.status(id).map_err(internal)?;
    if fd_connectors::client::require_running(row.as_ref().is_some_and(|r| r.enabled), &state.state)
        .is_err()
    {
        let _=app.alerts.raise(&format!("route:{connection}:{usage:?}"),None,"connector.route_unavailable",fd_alerts::Severity::Critical,&format!("Connector {id} is unavailable for connection {connection}, route {usage:?}. No fallback is used."));
        return Err(Status::unavailable("connector.unavailable"));
    }
    let _ = app.alerts.resolve(
        &format!("route:{connection}:{usage:?}"),
        None,
        "connector.route_unavailable",
    );
    Ok(())
}

/// Materialize credentials only for uses that actually reference a connection.
pub fn connection_uses(
    app: &App,
    id: &str,
) -> Result<std::collections::BTreeSet<RouteUse>, Status> {
    use fd_state::resources::{StateDestinationTarget, StorageMode};
    let mut uses = std::collections::BTreeSet::new();
    for row in app
        .store
        .list(ResourceKind::ManagedFolder, None, usize::MAX / 2)
        .map_err(internal)?
    {
        if let Ok(ResourceSpec::ManagedFolder(folder)) = row.parsed_spec() {
            if folder.reference.connection_id.as_deref() == Some(id) {
                uses.insert(if folder.storage_mode == StorageMode::CloudMount {
                    RouteUse::CloudMount
                } else {
                    RouteUse::OwnedObjectStorage
                });
            }
            if folder
                .reference
                .backup_destination
                .as_ref()
                .is_some_and(|d| d["connectionId"] == id)
            {
                uses.insert(RouteUse::BackupRepository);
            }
        }
    }
    for row in app
        .store
        .list(
            ResourceKind::ApplianceStateDestination,
            None,
            usize::MAX / 2,
        )
        .map_err(internal)?
    {
        if let Ok(ResourceSpec::ApplianceStateDestination(d)) = row.parsed_spec()
            && matches!(d.target,StateDestinationTarget::ObjectStorage{connection_id,..} if connection_id==id)
        {
            uses.insert(RouteUse::OwnedObjectStorage);
        }
    }
    if crate::state_runtime::locations(app)?
        .iter()
        .any(|l| l.connection_id == id)
    {
        uses.insert(RouteUse::OwnedObjectStorage);
    }
    Ok(uses)
}

pub fn proven_checksums(row: &fd_state::StoredResource, connector: &str) -> bool {
    row.status
        .as_ref()
        .and_then(|s| {
            s.get("testsByConnector")
                .and_then(|t| t.get(connector))
                .or_else(|| s.get("lastTest"))
        })
        .and_then(|v| serde_json::from_value::<fd_connectors::StoredTestResult>(v.clone()).ok())
        .is_some_and(|p| p.connector_id == connector && p.write_steps_run && p.provider_checksums)
}

// A restart can begin and end between five-second polls. Socket creation identifies
// the actual native process even when both sampled supervisor states are Running.
fn socket_epoch(path: &std::path::Path) -> Option<(u64, u64, i64, i64)> {
    use std::os::unix::fs::{FileTypeExt, MetadataExt};
    let metadata = std::fs::metadata(path).ok()?;
    metadata.file_type().is_socket().then_some((
        metadata.dev(),
        metadata.ino(),
        metadata.mtime(),
        metadata.mtime_nsec(),
    ))
}
#[cfg(test)]
mod socket_epoch_tests {
    #[test]
    fn detects_a_restart_between_running_samples() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("connector.sock");
        let first = std::os::unix::net::UnixListener::bind(&path).unwrap();
        let initial = super::socket_epoch(&path).unwrap();
        assert_eq!(super::socket_epoch(&path), Some(initial));
        drop(first);
        std::fs::remove_file(&path).unwrap();
        let _second = std::os::unix::net::UnixListener::bind(&path).unwrap();
        assert_ne!(super::socket_epoch(&path), Some(initial));
    }
}
