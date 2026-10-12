//! FileDock Core Service daemon.

mod api;
mod config;
mod proto_conv;
mod runtime;

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
    journal.resume_all(&[])?;

    let events = Arc::new(EventBus::new(db.clone()));
    let alerts = Arc::new(AlertStore::new(db.clone()));
    let secrets = SecretStore::open(&config.secrets, &config.dek)?;
    let recovery = RecoveryKeyStore::open(&config.recovery_keys, &secrets.dek_bytes())?;
    let subsystems = SubsystemRegistry::new(db.clone());
    if let Ok(manifests) = load_dir(&config.manifests) {
        for (_, manifest) in manifests {
            if let Err(error) = subsystems.upsert_from_manifest(&manifest) {
                tracing::warn!(id = %manifest.id, %error, "failed to register subsystem");
            }
        }
    }

    let reconciler = Arc::new(Reconciler::new(store.clone(), Vec::new()));
    let mut commits = store.subscribe();
    let rec = reconciler.clone();
    tokio::spawn(async move {
        while let Ok(notice) = commits.recv().await {
            rec.enqueue_commit(notice).await;
        }
    });
    tokio::spawn(reconciler.clone().run());

    api::serve(
        config, store, journal, events, alerts, secrets, recovery, subsystems,
    )
    .await
}
