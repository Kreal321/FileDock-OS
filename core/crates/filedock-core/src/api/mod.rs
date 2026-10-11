pub mod connectors;
pub mod control;
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
    pub store: Arc<StateStore>,
    pub journal: Arc<Journal>,
    pub events: Arc<EventBus>,
    pub alerts: Arc<AlertStore>,
    pub secrets: Arc<Mutex<SecretStore>>,
    pub recovery: Arc<Mutex<RecoveryKeyStore>>,
    pub subsystems: SubsystemRegistry,
}

pub async fn serve(
    config: Config,
    store: Arc<StateStore>,
    journal: Arc<Journal>,
    events: Arc<EventBus>,
    alerts: Arc<AlertStore>,
    secrets: SecretStore,
    recovery: RecoveryKeyStore,
    subsystems: SubsystemRegistry,
) -> Result<(), Box<dyn std::error::Error>> {
    let listener = bind_socket(&config.socket)?;
    let app = Arc::new(App {
        config: config.clone(),
        store,
        journal,
        events,
        alerts,
        secrets: Arc::new(Mutex::new(secrets)),
        recovery: Arc::new(Mutex::new(recovery)),
        subsystems,
    });
    let interceptor = InterfaceInterceptor {
        version: config.interface_version,
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
