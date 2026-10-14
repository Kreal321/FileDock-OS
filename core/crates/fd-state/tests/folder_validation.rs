use fd_state::{
    Change, ChangeOrigin, ResourceKind, ResourceRef, StateStore,
    db::{Db, MIGRATION_0001_INIT},
};
use serde_json::{Value, json};
fn base() -> (StateStore, String) {
    let s = StateStore::new(Db::open_in_memory(&[MIGRATION_0001_INIT]).unwrap());
    let id = fd_state::new_id();
    s.apply(
        ChangeOrigin::Restore,
        &[Change::Upsert {
            resource: ResourceRef::new(ResourceKind::Pool, &id),
            expected_generation: 0,
            spec: json!({"name":"data","kind":"Adopted","memberDisks":[{"serial":"fixture"}]}),
        }],
    )
    .unwrap();
    (s, id)
}
fn folder(pool: &str, name: &str, path: &str) -> Value {
    json!({"name":name,"filedockPath":format!("/storage/{name}"),"storageMode":"LocalOnly","reference":{"poolId":pool,"relativePath":path},"dataImportance":null,"access":[],"advanced":{}})
}
fn codes(s: &StateStore, v: Value) -> Vec<String> {
    s.validate(&[Change::Upsert {
        resource: ResourceRef::new(ResourceKind::ManagedFolder, ""),
        expected_generation: 0,
        spec: v,
    }])
    .unwrap()
    .into_iter()
    .map(|i| i.code)
    .collect()
}
#[test]
fn overlap_nesting_boundaries_and_reserved_locations() {
    let (s, p) = base();
    s.apply(
        ChangeOrigin::Restore,
        &[Change::Upsert {
            resource: ResourceRef::new(ResourceKind::ManagedFolder, ""),
            expected_generation: 0,
            spec: folder(&p, "movies", "/movies"),
        }],
    )
    .unwrap();
    for path in ["/movies", "/movies/kids"] {
        assert!(codes(&s, folder(&p, "other", path)).contains(&"reference.overlap".into()));
    }
    let mut f = folder(&p, "other", "/other");
    f["filedockPath"] = json!("/storage/movies/kids");
    assert!(codes(&s, f).contains(&"folder.path_overlap".into()));
    assert!(codes(&s, folder(&p, "movies2", "/movies2")).is_empty());
    for path in [
        "/",
        "/.filedock",
        "/.filedock/cache",
        "/.filedock-pool.json",
    ] {
        assert!(
            codes(&s, folder(&p, "reserved", path))
                .contains(&"reference.pool_root_reserved".into())
        );
    }
}
#[test]
fn importance_and_advanced_are_mode_specific() {
    let (s, p) = base();
    let mut f = folder(&p, "media", "/media");
    f["dataImportance"] = json!("Important");
    assert!(codes(&s, f.clone()).contains(&"folder.importance_not_allowed".into()));
    f["dataImportance"] = Value::Null;
    f["advanced"] = json!({"subfolderImportance":{"x":"Temporary"}});
    assert!(codes(&s, f).contains(&"folder.advanced_not_allowed".into()));
    let mut f = folder(&p, "hybrid", "/hybrid");
    f["storageMode"] = json!("HybridBackup");
    f["reference"]["backupDestination"] =
        json!({"connectionId":fd_state::new_id(),"remotePath":"backup"});
    assert!(codes(&s, f.clone()).contains(&"folder.importance_required".into()));
    f["dataImportance"] = json!("Critical");
    f["advanced"] = json!({"backupIntervalSeconds":1000});
    assert!(codes(&s, f.clone()).contains(&"folder.critical_interval".into()));
    f["advanced"] = json!({"subfolderImportance":{"../escape":"Temporary"}});
    assert!(codes(&s, f).contains(&"folder.override_path_invalid".into()));
}
#[test]
fn missing_mode_capabilities_are_named() {
    let (s, p) = base();
    let mut f = folder(&p, "tier", "/tier");
    f["storageMode"] = json!("TieredStorage");
    f["reference"]["connectionId"] = json!(fd_state::new_id());
    f["reference"]["remotePath"] = json!("cold");
    assert!(codes(&s, f).contains(&"mode.capability_missing".into()));
}
#[test]
fn defaults_follow_explicit_importance() {
    use fd_state::validate::folder::normalize;
    let c =
        normalize(json!({"storageMode":"HybridBackup","dataImportance":"Critical","advanced":{}}));
    assert_eq!(c["advanced"]["backupIntervalSeconds"], 300);
    let r = normalize(
        json!({"storageMode":"HybridBackup","dataImportance":"Rebuildable","advanced":{}}),
    );
    assert_eq!(r["advanced"]["backupEnabled"], false);
}

#[test]
fn local_reference_is_immutable_and_failed_system_pool_refuses_creation() {
    let (s, p) = base();
    let outcome = s
        .apply(
            ChangeOrigin::Restore,
            &[Change::Upsert {
                resource: ResourceRef::new(ResourceKind::ManagedFolder, ""),
                expected_generation: 0,
                spec: folder(&p, "original", "/original"),
            }],
        )
        .unwrap();
    let old = &outcome.committed[0];
    let mut next = old.spec.clone();
    next["reference"]["relativePath"] = json!("/moved");
    let issues = s
        .validate(&[Change::Upsert {
            resource: old.resource.clone(),
            expected_generation: old.generation,
            spec: next,
        }])
        .unwrap();
    assert!(
        issues
            .iter()
            .any(|i| i.code == "folder.reference_immutable")
    );
    let system = fd_state::new_id();
    s.apply(ChangeOrigin::Restore,&[Change::Upsert{resource:ResourceRef::new(ResourceKind::Pool,&system),expected_generation:0,spec:json!({"name":"system","kind":"System","systemLimits":{"maxUsagePercent":50,"reservedFreeBytes":100000000000_u64}})}]).unwrap();
    let c = codes(&s, folder(&system, "small-system", "/small"));
    assert!(c.contains(&"system_pool.shared_with_os".into()));
    assert!(c.contains(&"system_pool.unavailable".into()));
}
#[test]
fn cloud_mount_overlap_exception_and_tiered_and_backup_conflicts() {
    let (s, p) = base();
    let conn = fd_state::new_id();
    s.apply(ChangeOrigin::Restore,&[Change::Upsert{resource:ResourceRef::new(ResourceKind::CloudConnection,&conn),expected_generation:0,spec:json!({"name":"cloud","providerFamily":"s3-compatible","definition":{"type":"s3","endpoint":"https://s3.example.test","region":"us-east-1","addressingStyle":"Path","bucket":"test-bucket","accessKeyRef":"connection/cloud/access","secretKeyRef":"connection/cloud/secret"}})}]).unwrap();
    let cloud = json!({"name":"mount","filedockPath":"/storage/mount","storageMode":"CloudMount","reference":{"connectionId":conn,"remotePath":"remote/media"},"access":[],"advanced":{}});
    s.apply(
        ChangeOrigin::Restore,
        &[Change::Upsert {
            resource: ResourceRef::new(ResourceKind::ManagedFolder, ""),
            expected_generation: 0,
            spec: cloud.clone(),
        }],
    )
    .unwrap();
    let mut second = cloud.clone();
    second["name"] = json!("overlap");
    second["filedockPath"] = json!("/storage/overlap");
    second["reference"]["remotePath"] = json!("remote/media/movies");
    assert!(!codes(&s, second.clone()).contains(&"reference.cloud_overlap".into()));
    second["storageMode"] = json!("TieredStorage");
    second["reference"]["poolId"] = json!(p);
    second["reference"]["relativePath"] = json!("/tiered");
    assert!(codes(&s, second).contains(&"reference.cloud_overlap".into()));
    let mut backup = folder(&p, "backup", "/source");
    backup["storageMode"] = json!("HybridBackup");
    backup["dataImportance"] = json!("Critical");
    backup["reference"]["backupDestination"] =
        json!({"connectionId":conn,"remotePath":"remote/media/backups"});
    assert!(codes(&s, backup).contains(&"reference.backup_overlap".into()));
}
