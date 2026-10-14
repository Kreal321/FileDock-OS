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
        for use_kind in [
            RouteUse::OwnedObjectStorage,
            RouteUse::BackupRepository,
            RouteUse::CloudMount,
        ] {
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
                    "access_key_ref": def.access_key_ref, "secret_key_ref": def.secret_key_ref, "checksums": false}));
        }
        if targets.contains("rclone") {
            rclone_connections.push((row.resource.id, connection));
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
    }
    drop(secrets);
    let conf = fd_connectors::rclone::conf::render_conf(&remotes);
    let path = app.config.socket.parent().unwrap().join("rclone.conf");
    let changed = std::fs::read_to_string(&path).ok().as_deref() != Some(conf.as_str());
    if changed {
        write_private(&path, conf.as_bytes()).map_err(internal)?;
    }
    #[cfg(target_os = "linux")]
    {
        if changed && app.supervisor.status("rclone").map_err(internal)?.state == "Running" {
            fd_subsystems::systemd::Systemd::default()
                .restart("filedock-rclone.service")
                .await
                .map_err(internal)?;
        }
    }
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
    {
        let mut temporary = app.temporary_s3.lock().await;
        temporary.retain(|_, (_, files)| {
            files
                .values()
                .all(|path| std::path::Path::new(path).is_file())
        });
        for (row, files) in temporary.values() {
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
    configure(app).await?;
    crate::state_runtime::inspect_locations(app).await?;
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
        RouteUse::OwnedObjectStorage,
    )
    .map_err(internal)?;
    if connector == "rclone" {
        return test_rclone(app, &reference, &connection).await;
    }
    let connector_socket = if connector == "fd-s3" {
        s3_socket(app)
    } else {
        crate::extension_runtime::socket(app, &connector)
    };
    let mut client =
        ext::cloud_remote_client::CloudRemoteClient::new(channel(&connector_socket).await?);
    let locations = crate::state_runtime::locations(app)?;
    let registered = locations.iter().find(|l| l.connection_id == id);
    let root_prefix = registered
        .map(|l| l.prefix.trim_end_matches('/'))
        .unwrap_or("");
    let discovery = fd_connectors::object_io::S3Objects {
        channel: channel(&connector_socket).await?,
        connection_id: id.into(),
        prefix: if root_prefix.is_empty() {
            String::new()
        } else {
            format!("{root_prefix}/")
        },
        cleanup_directory: None,
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
            connection_id: id.into(),
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
    save_probe(app, &reference, "fd-s3", result)
}

async fn test_rclone(
    app: &App,
    reference: &ResourceRef,
    connection: &fd_state::resources::CloudConnectionSpec,
) -> Result<fd_api::core::v1::ConnectionTestResult, Status> {
    let bucket = match &connection.definition {
        ConnectionDefinition::S3(s3) => &s3.bucket,
        _ => "",
    };
    let remote = if bucket.is_empty() {
        reference.id.clone()
    } else {
        format!("{}:{}", reference.id, bucket)
    };
    let adapter = fd_connectors::rclone::adapter::RcloneAdapter::new();
    // Fail closed on any ownership read error other than a known missing object.
    let owner = adapter.read(&remote, "_filedock/owner.json").await;
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
        "_filedock/probe/{}/{}",
        app.appliance_id,
        uuid::Uuid::now_v7()
    );
    adapter.list(&remote, "").await.map_err(internal)?;
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
    app.store
        .write_status(reference, &json!({"lastTest": stored}))
        .map_err(internal)?;
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
    let mut last_probe = std::time::Instant::now() - Duration::from_secs(86400);
    loop {
        interval.tick().await;
        if app
            .supervisor
            .status("fd-s3")
            .is_ok_and(|s| s.state == "Failed")
            || app
                .supervisor
                .status("rclone")
                .is_ok_and(|s| s.state == "Failed")
        {
            continue;
        }
        if let Err(error) = configure(&app).await {
            tracing::warn!(%error, "connector configuration pending");
            continue;
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
