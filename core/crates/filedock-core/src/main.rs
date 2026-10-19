//! FileDock Core Service daemon.

mod api;
mod app_runtime;
mod config;
mod connection_runtime;
mod extension_runtime;
mod notification_runtime;
mod oauth_runtime;
mod pool_recovery;
mod proto_conv;
mod restore_runtime;
mod runtime;
mod session_runtime;
mod sharing_runtime;
mod startup;
mod state_runtime;
mod storage_runtime;
mod sweep_runtime;

use std::sync::Arc;

use fd_alerts::AlertStore;
use fd_alerts::MIGRATION_0004_ALERTS;
use fd_journal::Journal;
use fd_journal::MIGRATION_0002_JOURNAL;
use fd_observe::MIGRATION_0003_EVENTS;
use fd_observe::{EventBus, init_logging};
use fd_reconcile::Reconciler;
use fd_secrets::{RecoveryKeyStore, SecretStore};
use fd_state::StateStore;
use fd_state::db::{Db, MIGRATION_0001_INIT};
use fd_subsystems::MIGRATION_0005_SUBSYSTEMS;
use fd_subsystems::{SubsystemRegistry, load_dir};

use crate::config::Config;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let command = std::env::args().nth(1);
    if command.as_deref() == Some("system-pool-loop") {
        if unsafe { libc::geteuid() } != 0 {
            return Err("root required".into());
        }
        fd_storage::system_pool::image::attach_loop(&storage_runtime::image())?;
        return Ok(());
    }
    if command.as_deref() == Some("detect-state") {
        let config = Config::from_env();
        let mut roots = vec![(config.state_root, "system disk".into())];
        roots.extend(fd_appstate::detect::pool_roots(&config.pools_root)?);
        let mut discovery = None;
        if cfg!(target_os = "linux") && unsafe { libc::geteuid() } == 0 {
            let output = std::path::PathBuf::from("/run/filedock")
                .join(format!("detect-cli-{}", uuid::Uuid::new_v4()));
            let child = std::process::Command::new("unshare")
                .args(["--mount", "--propagation", "private"])
                .arg(std::env::current_exe()?)
                .arg("detect-state-private")
                .env("FD_DETECT_OUTPUT", &output)
                .output();
            if let Ok(child) = child
                && child.status.success()
                && let Ok(found) =
                    serde_json::from_slice::<Vec<(std::path::PathBuf, String)>>(&child.stdout)
            {
                roots.extend(found);
            }
            discovery = Some(output);
        }
        println!(
            "{}",
            serde_json::to_string(&fd_appstate::detect::scan(&roots)?)?
        );
        if let Some(root) = discovery {
            let _ = std::fs::remove_dir_all(root);
        }
        return Ok(());
    }
    if command.as_deref() == Some("detect-state-private") {
        let output = std::env::var_os("FD_DETECT_OUTPUT").ok_or("FD_DETECT_OUTPUT missing")?;
        println!(
            "{}",
            serde_json::to_string(&fd_appstate::detect_linux::scan_unmounted(
                std::path::Path::new(&output)
            )?)?
        );
        return Ok(());
    }
    init_logging();
    let config = Config::from_env();
    tracing::info!(socket = %config.socket.display(), "starting FileDock Core");

    let db = Db::open(
        &config.core_db,
        &[
            MIGRATION_0001_INIT,
            MIGRATION_0002_JOURNAL,
            MIGRATION_0003_EVENTS,
            MIGRATION_0004_ALERTS,
            MIGRATION_0005_SUBSYSTEMS,
        ],
    )?;
    let store = Arc::new(StateStore::new(db.clone()));
    store.seed_defaults()?;

    let journal = Arc::new(Journal::new(db.clone()));

    let events = Arc::new(EventBus::new(db.clone()));
    let alerts = Arc::new(AlertStore::new(db.clone()));
    let secrets = SecretStore::open(&config.secrets, &config.dek)?;
    let recovery = match RecoveryKeyStore::open(&config.recovery_keys, &secrets.dek_bytes()) {
        Ok(store) => store,
        Err(original) => restore_runtime::recover_key_on_start(&config, &db, &secrets.dek_bytes())?
            .ok_or(original)?,
    };
    let subsystems = SubsystemRegistry::new(db.clone());
    for (_, manifest) in load_dir(&config.manifests)? {
        subsystems.upsert_from_manifest(&manifest)?;
    }

    let mut controllers: Vec<Arc<dyn fd_reconcile::Controller>> =
        [fd_state::ResourceKind::Extension]
            .into_iter()
            .map(|kind| {
                Arc::new(fd_reconcile::adopt::AdoptionController {
                    kind,
                    store: store.clone(),
                    events: events.clone(),
                }) as Arc<dyn fd_reconcile::Controller>
            })
            .collect();
    controllers.push(Arc::new(
        fd_reconcile::controllers::folder::FolderController {
            store: store.clone(),
            events: events.clone(),
            pools_root: config.pools_root.clone(),
            units: "/etc/systemd/system".into(),
            metadata: config.core_db.parent().unwrap().join("folder-mounts"),
            enabled: cfg!(target_os = "linux")
                && std::env::var("FD_RUNTIME_ENABLED").as_deref() != Ok("0"),
            gate: tokio::sync::Mutex::new(()),
        },
    ));
    controllers.push(Arc::new(
        fd_reconcile::controllers::container_app::ContainerAppController {
            store: store.clone(),
            events: events.clone(),
            root: config.apps_root.clone(),
            secrets_path: config.secrets.clone(),
            dek_path: config.dek.clone(),
            secrets_root: config.secrets_run.clone(),
            enabled: cfg!(target_os = "linux")
                && std::env::var("FD_RUNTIME_ENABLED").as_deref() != Ok("0"),
        },
    ));
    let reconciler = Arc::new(Reconciler::new(store.clone(), controllers));
    let folder_poll = reconciler.clone();
    let folder_store = store.clone();
    tokio::spawn(async move {
        loop {
            let mut cursor = None;
            loop {
                match folder_store.list(
                    fd_state::ResourceKind::ManagedFolder,
                    cursor.as_deref(),
                    500,
                ) {
                    Ok(rows) if !rows.is_empty() => {
                        cursor = rows.last().map(|r| r.resource.id.clone());
                        folder_poll
                            .queue()
                            .enqueue_all(rows.into_iter().map(|r| r.resource))
                            .await;
                    }
                    _ => break,
                }
            }
            if let Ok(rows) = folder_store.deleted(fd_state::ResourceKind::ManagedFolder) {
                folder_poll
                    .queue()
                    .enqueue_all(rows.into_iter().map(|r| r.resource))
                    .await;
            }
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
        }
    });
    let mut commits = store.subscribe();
    let rec = reconciler.clone();
    let commit_events = events.clone();
    tokio::spawn(async move {
        loop {
            match commits.recv().await {
                Ok(notice) => {
                    for resource in &notice.resources {
                        let _ = commit_events.emit(
                            "resource.changed",
                            Some(resource.clone()),
                            serde_json::json!({"changeSeq":notice.change_seq}),
                        );
                    }
                    rec.enqueue_commit(notice).await;
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                    rec.enqueue_all_kinds().await
                }
                Err(_) => break,
            }
        }
    });
    let mut observed = events.subscribe();
    let event_reconciler = reconciler.clone();
    tokio::spawn(async move {
        loop {
            match observed.recv().await {
                Ok(event)
                    if event.r#type.starts_with("subsystem.")
                        || event.r#type == "folder.status"
                        || event.r#type == "pool.health.changed" =>
                {
                    event_reconciler.enqueue_all_kinds().await
                }
                Ok(_) => {}
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                    event_reconciler.enqueue_all_kinds().await
                }
                Err(_) => break,
            }
        }
    });
    tokio::spawn(reconciler.clone().run());

    api::serve(
        config,
        store,
        journal,
        events,
        alerts,
        secrets,
        recovery,
        api::RuntimeServices {
            registry: subsystems,
            reconciler,
        },
    )
    .await
}
