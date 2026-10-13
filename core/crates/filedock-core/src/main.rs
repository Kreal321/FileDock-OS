//! FileDock Core Service daemon.

mod api;
mod config;
mod proto_conv;
mod runtime;
mod startup;

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
    let recovery = RecoveryKeyStore::open(&config.recovery_keys, &secrets.dek_bytes())?;
    let subsystems = SubsystemRegistry::new(db.clone());
    for (_, manifest) in load_dir(&config.manifests)? {
        subsystems.upsert_from_manifest(&manifest)?;
    }

    let controllers: Vec<Arc<dyn fd_reconcile::Controller>> = [
        fd_state::ResourceKind::Pool,
        fd_state::ResourceKind::ManagedFolder,
        fd_state::ResourceKind::Export,
        fd_state::ResourceKind::ContainerApp,
        fd_state::ResourceKind::Extension,
    ]
    .into_iter()
    .map(|kind| {
        Arc::new(fd_reconcile::adopt::AdoptionController {
            kind,
            store: store.clone(),
            events: events.clone(),
        }) as Arc<dyn fd_reconcile::Controller>
    })
    .collect();
    let reconciler = Arc::new(Reconciler::new(store.clone(), controllers));
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
                Ok(event) if event.r#type.starts_with("subsystem.") => {
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
