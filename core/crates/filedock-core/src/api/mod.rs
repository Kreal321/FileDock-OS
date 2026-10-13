pub mod bridge;
pub mod connectors;
pub mod control;
pub mod events;
pub mod interceptors;
pub mod observability;
pub mod recovery;
pub mod secrets;
pub mod status;

use std::sync::Arc;

use fd_alerts::AlertStore;
use fd_api::core::v1::{
    connectors_server::ConnectorsServer, core_control_server::CoreControlServer,
    observability_server::ObservabilityServer, recovery_server::RecoveryServer,
    secrets_server::SecretsServer,
};
use fd_journal::Journal;
use fd_observe::EventBus;
use fd_secrets::{RecoveryKeyStore, SecretStore};
use fd_state::StateStore;
use fd_subsystems::SubsystemRegistry;
use tokio::net::UnixListener;
use tokio::sync::Mutex;
use tokio_stream::wrappers::UnixListenerStream;
use tonic::transport::Server;

use crate::config::Config;
use interceptors::InterfaceInterceptor;

use self::connectors::ConnectorsSvc;
use self::control::CoreControlSvc;
use self::observability::ObservabilitySvc;
use self::recovery::RecoverySvc;
use self::secrets::SecretsSvc;

pub struct App {
    pub config: Config,
    pub appliance_id: String,
    pub extension_token: String,
    pub store: Arc<StateStore>,
    pub journal: Arc<Journal>,
    pub events: Arc<EventBus>,
    pub alerts: Arc<AlertStore>,
    pub secrets: Arc<Mutex<SecretStore>>,
    pub recovery: Arc<Mutex<RecoveryKeyStore>>,
    pub subsystems: SubsystemRegistry,
    pub reconciler: Arc<fd_reconcile::Reconciler>,
    pub supervisor: Arc<fd_subsystems::supervisor::Supervisor>,
}

pub struct RuntimeServices {
    pub registry: SubsystemRegistry,
    pub reconciler: Arc<fd_reconcile::Reconciler>,
}

pub async fn serve(
    config: Config,
    store: Arc<StateStore>,
    journal: Arc<Journal>,
    events: Arc<EventBus>,
    alerts: Arc<AlertStore>,
    secrets: SecretStore,
    recovery: RecoveryKeyStore,
    runtime: RuntimeServices,
) -> Result<(), Box<dyn std::error::Error>> {
    let listener = bind_socket(&config.socket)?;
    let appliance_id = store.db().with(|conn| {
        if let Some(id) = fd_state::db::get_setting(conn, "appliance_id")? {
            return Ok::<_, fd_state::db::DbError>(id.as_str().unwrap_or_default().to_owned());
        }
        let id = uuid::Uuid::now_v7().to_string();
        fd_state::db::put_setting(conn, "appliance_id", &serde_json::json!(id))?;
        Ok(id)
    })?;
    let extension_token =
        std::fs::read_to_string(config.socket.parent().unwrap().join("fd-s3.env"))
            .ok()
            .and_then(|s| {
                s.lines()
                    .find_map(|line| line.strip_prefix("FD_EXTENSION_TOKEN=").map(str::to_owned))
            })
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let supervisor = Arc::new(fd_subsystems::supervisor::Supervisor::new(
        store.db().clone(),
        events.clone(),
    ));
    let app = Arc::new(App {
        config: config.clone(),
        appliance_id,
        extension_token,
        store,
        journal,
        events,
        alerts,
        secrets: Arc::new(Mutex::new(secrets)),
        recovery: Arc::new(Mutex::new(recovery)),
        subsystems: runtime.registry,
        reconciler: runtime.reconciler,
        supervisor,
    });
    let bridge_path = config.socket.parent().unwrap().join("extensions.sock");
    let _ = std::fs::remove_file(&bridge_path);
    let bridge_listener = UnixListener::bind(&bridge_path)?;
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&bridge_path, std::fs::Permissions::from_mode(0o600))?;
    let bridge_app = app.clone();
    tokio::spawn(async move {
        if let Err(error) = Server::builder()
            .add_service(
                fd_api::extension::v1::core_bridge_server::CoreBridgeServer::new(bridge::Bridge {
                    app: bridge_app,
                }),
            )
            .serve_with_incoming(UnixListenerStream::new(bridge_listener))
            .await
        {
            tracing::error!(%error, "CoreBridge stopped");
        }
    });
    if cfg!(target_os = "linux") && std::env::var("FD_RUNTIME_ENABLED").as_deref() != Ok("0") {
        let env_path = config.socket.parent().unwrap().join("fd-s3.env");
        crate::runtime::write_private(
            &env_path,
            format!(
                "FD_EXTENSION_TOKEN={}\nFD_CORE_SOCKET={}\nFD_S3_SOCKET={}\n",
                app.extension_token,
                bridge_path.display(),
                crate::runtime::s3_socket(&app).display()
            )
            .as_bytes(),
        )?;
        tokio::spawn(crate::runtime::run(app.clone()));
    }
    let startup_app = app.clone();
    tokio::spawn(async move {
        if let Err(error) = crate::startup::run(
            startup_app.store.db().clone(),
            startup_app.journal.clone(),
            startup_app.reconciler.clone(),
            startup_app.events.clone(),
            cfg!(target_os = "linux") && std::env::var("FD_RUNTIME_ENABLED").as_deref() != Ok("0"),
            &startup_app.subsystems,
            &startup_app.supervisor,
        )
        .await
        {
            tracing::error!(%error,"startup reconciliation failed");
        }
    });
    let interceptor = InterfaceInterceptor {
        mgmt_uid: config.mgmt_uid,
    };
    Server::builder()
        .add_service(CoreControlServer::with_interceptor(
            CoreControlSvc { app: app.clone() },
            interceptor.clone(),
        ))
        .add_service(SecretsServer::with_interceptor(
            SecretsSvc { app: app.clone() },
            interceptor.clone(),
        ))
        .add_service(ObservabilityServer::with_interceptor(
            ObservabilitySvc { app: app.clone() },
            interceptor.clone(),
        ))
        .add_service(ConnectorsServer::with_interceptor(
            ConnectorsSvc { app: app.clone() },
            interceptor.clone(),
        ))
        .add_service(RecoveryServer::with_interceptor(
            RecoverySvc { app: app.clone() },
            interceptor,
        ))
        .serve_with_incoming(UnixListenerStream::new(listener))
        .await?;
    Ok(())
}

fn bind_socket(path: &std::path::Path) -> Result<UnixListener, Box<dyn std::error::Error>> {
    if let Ok(fds) = std::env::var("LISTEN_FDS")
        && fds == "1"
    {
        use std::os::fd::FromRawFd;
        let std_listener = unsafe { std::os::unix::net::UnixListener::from_raw_fd(3) };
        std_listener.set_nonblocking(true)?;
        return Ok(UnixListener::from_std(std_listener)?);
    }
    let _ = std::fs::remove_file(path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    Ok(UnixListener::bind(path)?)
}
