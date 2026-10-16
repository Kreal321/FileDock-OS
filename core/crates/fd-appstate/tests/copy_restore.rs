use fd_appstate::{
    copy::backends::{Backend, CopyKind, CopyTarget},
    engine::{Engine, SnapshotInput},
    format::fdbackup,
    restore::sources::{ReadBackend, Source},
};
use fd_secrets::{RecoveryKeyStore, SecretStore};
use fd_state::{Db, StateStore, db::MIGRATION_0001_INIT};
use serde_json::json;
use std::{collections::BTreeMap, sync::Arc};
#[tokio::test]
async fn durable_copies_export_restore_and_tamper_detection() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("core.db"), &[MIGRATION_0001_INIT]).unwrap();
    let store = StateStore::new(db.clone());
    store.seed_defaults().unwrap();
    let secret = SecretStore::open(dir.path().join("secrets"), dir.path().join("dek")).unwrap();
    let mut recovery =
        RecoveryKeyStore::open(dir.path().join("keys"), &secret.dek_bytes()).unwrap();
    let generated = recovery.generate(false).unwrap();
    recovery.confirm(&generated.key_id).unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let config = fd_appstate::fdconfig::serialize(&db, &id, json!({"configSeq":1})).unwrap();
    let input = SnapshotInput {
        appliance_id: id.clone(),
        config,
        core_seq: store.change_seq().unwrap(),
        management_seq: 1,
        changed_at: None,
        secrets: secret.encrypted_bytes().unwrap(),
        dek: secret.dek_bytes(),
        key: recovery.current().unwrap().clone(),
        masters: recovery.snapshot_keys().unwrap(),
        packages: BTreeMap::new(),
        release_version: "test".into(),
    };
    let target = CopyTarget {
        id: "system".into(),
        kind: CopyKind::LocalSystemDisk,
        pool_id: "".into(),
        description: "System".into(),
        available: true,
        backend: Backend::Local(dir.path().join("copy")),
    };
    let mut engine = Engine::new(db.clone(), dir.path().join("spool")).unwrap();
    // Hold one destination's session lock. The other copy must publish progress first.
    let locked = target.backend.scoped().await;
    let other = CopyTarget {
        id: "independent".into(),
        backend: Backend::Local(dir.path().join("other")),
        ..target.clone()
    };
    let targets = vec![target.clone(), other];
    let job = tokio::spawn(async move {
        assert!(engine.tick(&input, &targets, true).await.unwrap());
        (engine, input)
    });
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            if Engine::records(&db)
                .unwrap()
                .iter()
                .any(|r| r.id == "independent" && r.last_snapshot_at.is_some())
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("A blocked copy must not delay another durable copy's status");
    assert!(
        Engine::records(&db)
            .unwrap()
            .iter()
            .any(|r| r.id == "system" && r.last_snapshot_at.is_none())
    );
    drop(locked);
    let (engine, input) = job.await.unwrap();
    let records = Engine::records(&db).unwrap();
    assert!(records[0].last_snapshot_at.is_some());
    let archive = engine.export(&input).unwrap();
    let files = fdbackup::decode(&archive).unwrap();
    let source = Source::open(
        ReadBackend::Files(Arc::new(files.clone())),
        &generated.recovery_key,
        false,
    )
    .await
    .unwrap();
    let snapshot = source.index.snapshots[0].snapshot_id.clone();
    assert_eq!(
        source.snapshot(&snapshot).await.unwrap().config,
        input.config
    );
    assert!(
        Source::open(
            ReadBackend::Files(Arc::new(files.clone())),
            "00000-00000-00000-00000-00000",
            false
        )
        .await
        .is_err()
    );
    let mut files = files;
    let file = format!("appliance/{}/{}", id, source.index.snapshots[0].file);
    files.get_mut(&file).unwrap()[120] ^= 1;
    let source = Source::open(
        ReadBackend::Files(Arc::new(files)),
        &generated.recovery_key,
        false,
    )
    .await
    .unwrap();
    assert!(source.snapshot(&snapshot).await.is_err());
    assert!(
        !Engine::protection(&db).unwrap()["machineLossCovered"]
            .as_bool()
            .unwrap()
    );
}
#[test]
fn uploaded_archive_refuses_traversal_and_unknown_schema() {
    assert!(fdbackup::encode(&BTreeMap::from([("appliance/../evil".into(), vec![1])])).is_err());
    assert!(
        fd_appstate::fdconfig::validate(
            &json!({"schema":"fdconfig/2","appliance":{},"core":{},"management":{}})
        )
        .is_err()
    );
}

#[test]
fn fresh_restore_allows_automatic_system_pool_but_refuses_user_configuration() {
    let dir = tempfile::tempdir().unwrap();
    let source = Db::open(&dir.path().join("source.db"), &[MIGRATION_0001_INIT]).unwrap();
    let source_store = StateStore::new(source.clone());
    source_store.seed_defaults().unwrap();
    source_store
        .apply(
            fd_state::ChangeOrigin::Console {
                actor: "test".into(),
            },
            &[fd_state::Change::Upsert {
                resource: fd_state::ResourceRef::singleton(fd_state::ResourceKind::Settings),
                expected_generation: 0,
                spec: json!({"applianceName":"test"}),
            }],
        )
        .unwrap();
    let config = fd_appstate::fdconfig::serialize(
        &source,
        &uuid::Uuid::new_v4().to_string(),
        json!({"configSeq":1}),
    )
    .unwrap();
    let target = Db::open(&dir.path().join("target.db"), &[MIGRATION_0001_INIT]).unwrap();
    let store = StateStore::new(target.clone());
    store.seed_defaults().unwrap();
    let system = fd_state::ResourceRef::new(
        fd_state::ResourceKind::Pool,
        "00000000-0000-7000-8000-000000000001",
    );
    store.apply(fd_state::ChangeOrigin::System{reason:"automatic System Pool".into()}, &[fd_state::Change::Upsert{resource:system,expected_generation:0,spec:json!({"name":"system","kind":"System","memberDisks":[],"layers":["loop","ext4"],"systemLimits":{"maxUsagePercent":50,"reservedFreeBytes":100000000000_u64}})}]).unwrap();
    fd_appstate::restore::apply::import_config(&target, &config, "restore-test", &[]).unwrap();
    fd_appstate::restore::apply::import_config(&target, &config, "restore-test", &[]).unwrap();
    store.apply(fd_state::ChangeOrigin::Console{actor:"test".into()}, &[fd_state::Change::Upsert{resource:fd_state::ResourceRef::new(fd_state::ResourceKind::ShareUser,uuid::Uuid::new_v4().to_string()),expected_generation:0,spec:json!({"username":"alice","passwordSecret":"share/alice/password","enabled":true})}]).unwrap();
    assert!(
        fd_appstate::restore::apply::import_config(&target, &config, "another-restore", &[])
            .is_err()
    );
}
