use fd_subsystems::{
    lifecycle::approve,
    package::Package,
    sandbox::{Binding, Sandbox},
};
use serde_json::json;
use std::collections::BTreeMap;
fn archive(files: BTreeMap<String, Vec<u8>>) -> Vec<u8> {
    let mut tar = tar::Builder::new(Vec::new());
    for (name, data) in files {
        let mut h = tar::Header::new_gnu();
        h.set_size(data.len() as u64);
        h.set_mode(0o755);
        h.set_cksum();
        tar.append_data(&mut h, name, &data[..]).unwrap();
    }
    zstd::stream::encode_all(&tar.into_inner().unwrap()[..], 1).unwrap()
}
fn package(extra: bool) -> Vec<u8> {
    use sha2::{Digest, Sha256};
    let manifest=b"schemaVersion: 1\nid: sample-native\nname: Sample\nversion: 1.0.0\nclass: extension\ncapabilities: [cache.generic]\nruntime: {type: native-process, executable: ./sample}\nhealth: {type: grpc}\npermissions: [core.events.read]\ncompatibility: {appliance: '>=0.1.0 <0.2.0'}\n".to_vec();
    let payload = b"sample executable".to_vec();
    let sums = format!(
        "{}  manifest.yaml\n{}  sample\n",
        hex::encode(Sha256::digest(&manifest)),
        hex::encode(Sha256::digest(&payload))
    );
    let mut files = BTreeMap::from([
        ("manifest.yaml".into(), manifest),
        ("sample".into(), payload),
        ("SHA256SUMS".into(), sums.into_bytes()),
    ]);
    if extra {
        files.insert("undeclared".into(), vec![1]);
    }
    archive(files)
}
#[test]
fn package_integrity_compatibility_and_exact_approval() {
    let d = tempfile::tempdir().unwrap();
    let p = Package::read(&package(false), d.path(), "0.1.0").unwrap();
    assert_eq!(p.manifest.id, "sample-native");
    assert!(!p.signed);
    assert!(Package::read(&package(true), d.path(), "0.1.0").is_err());
    assert!(Package::read(&package(false), d.path(), "0.3.0").is_err());
    assert!(approve(&p, &[], true, &BTreeMap::new(), &BTreeMap::new(), false).is_err());
    assert!(
        approve(
            &p,
            &["core.events.read".into()],
            false,
            &BTreeMap::new(),
            &BTreeMap::new(),
            false
        )
        .is_err()
    );
    approve(
        &p,
        &["core.events.read".into()],
        true,
        &BTreeMap::new(),
        &BTreeMap::new(),
        false,
    )
    .unwrap();
}
#[test]
fn sandbox_denies_host_writes_and_maps_only_bound_folders() {
    let m=fd_subsystems::manifest::parse_manifest("schemaVersion: 1\nid: sample-data\nname: Data\nversion: 1.0.0\nclass: extension\ncapabilities: [cache.generic]\nruntime: {type: docker-service, image: 'sample@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'}\nhealth: {type: grpc}\ndata: [{name: data, mountPath: /data}]\ncompatibility: {appliance: '>=0.1.0'}").unwrap();
    let folders = BTreeMap::from([(
        "data".into(),
        Binding {
            folder_id: "folder".into(),
            source: "/storage/test".into(),
            protected: false,
        },
    )]);
    let s = Sandbox::derive(&m, &folders).unwrap();
    let docker = s
        .docker(
            "65532",
            "/run/fd-test",
            "/run/filedock/extensions.sock",
            "token",
        )
        .unwrap();
    assert_eq!(docker["HostConfig"]["ReadonlyRootfs"], true);
    assert_eq!(docker["HostConfig"]["CapDrop"], json!(["ALL"]));
    assert_eq!(docker["HostConfig"]["NetworkMode"], "none");
    assert!(docker.get("Volumes").is_none());
    assert_eq!(s.bindings[0].target, "/data");
    let mut malicious = m.clone();
    malicious.raw["data"][0]["mountPath"] = json!("/etc");
    assert!(Sandbox::derive(&malicious, &folders).is_err());
}
#[test]
fn capability_defaults_require_a_capable_enabled_provider_and_cloud_routes() {
    use fd_state::{Db, db::MIGRATION_0001_INIT};
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(
        &dir.path().join("db"),
        &[
            MIGRATION_0001_INIT,
            fd_subsystems::MIGRATION_0005_SUBSYSTEMS,
        ],
    )
    .unwrap();
    let registry = fd_subsystems::SubsystemRegistry::new(db);
    let p = Package::read(&package(false), dir.path(), "0.1.0").unwrap();
    registry.upsert_from_manifest(&p.manifest).unwrap();
    registry
        .record_capabilities(&p.manifest.id, &p.manifest.capabilities)
        .unwrap();
    assert_eq!(
        fd_subsystems::capabilities::resolve(&registry, "cache.generic").unwrap(),
        "sample-native"
    );
    let mut other = p.manifest.clone();
    other.id = "other-native".into();
    other.raw["id"] = json!(other.id);
    registry.upsert_from_manifest(&other).unwrap();
    registry
        .record_capabilities(&other.id, &other.capabilities)
        .unwrap();
    assert!(fd_subsystems::capabilities::resolve(&registry, "cache.generic").is_err());
    fd_subsystems::capabilities::set_default(&registry, "cache.generic", "sample-native").unwrap();
    assert_eq!(
        fd_subsystems::capabilities::resolve(&registry, "cache.generic").unwrap(),
        "sample-native"
    );
    assert!(
        fd_subsystems::capabilities::set_default(&registry, "cloud.remote", "sample-native")
            .is_err()
    );
    registry.set_enabled("sample-native", false).unwrap();
    assert!(fd_subsystems::capabilities::resolve(&registry, "cache.generic").is_err());
}
#[test]
fn unsafe_paths_and_unprotected_bindings_are_refused() {
    for path in ["../escape", "/etc/shadow", "data//x", "data/./x", "data\\x"] {
        assert!(fd_subsystems::package::relative(path).is_err());
    }
    let d = tempfile::tempdir().unwrap();
    let mut p = Package::read(&package(false), d.path(), "0.1.0").unwrap();
    p.manifest.raw["data"] = json!([{"name":"data","mountPath":"/data"}]);
    let folders = BTreeMap::from([(
        "folder-id".into(),
        Binding {
            folder_id: "folder-id".into(),
            source: "/storage/example".into(),
            protected: false,
        },
    )]);
    let bindings = BTreeMap::from([("data".into(), "folder-id".into())]);
    assert!(
        approve(
            &p,
            &["core.events.read".into()],
            true,
            &bindings,
            &folders,
            false
        )
        .is_err()
    );
    approve(
        &p,
        &["core.events.read".into()],
        true,
        &bindings,
        &folders,
        true,
    )
    .unwrap();
}
#[test]
fn bound_folder_cannot_be_deleted() {
    use fd_state::{
        Change, ChangeOrigin, Db, ResourceKind, ResourceRef, StateStore, db::MIGRATION_0001_INIT,
    };
    let d = tempfile::tempdir().unwrap();
    let db = Db::open(&d.path().join("db"), &[MIGRATION_0001_INIT]).unwrap();
    let store = StateStore::new(db.clone());
    let folder = uuid::Uuid::new_v4().to_string();
    let extension = uuid::Uuid::new_v4().to_string();
    db.with(|c|{let now="2026-10-14T00:00:00Z";let spec=json!({"subsystemId":"sample-native","version":"1.0.0","packageSha256":"a".repeat(64),"config":{},"approvedPermissions":[],"signature":{"unsigned":{"acknowledged":true}},"dataBindings":{"data":folder}});
 c.execute("INSERT INTO resource(kind,id,generation,spec,status,deleted,change_seq,created_at,updated_at) VALUES('ManagedFolder',?1,1,'{}',NULL,0,1,?2,?2)",rusqlite::params![folder,now])?;
 c.execute("INSERT INTO resource(kind,id,generation,spec,status,deleted,change_seq,created_at,updated_at) VALUES('Extension',?1,1,?2,NULL,0,2,?3,?3)",rusqlite::params![extension,spec.to_string(),now])?;Ok::<_,fd_state::DbError>(())}).unwrap();
    let error = store
        .apply(
            ChangeOrigin::System {
                reason: "test".into(),
            },
            &[Change::Delete {
                resource: ResourceRef::new(ResourceKind::ManagedFolder, &folder),
                expected_generation: 1,
            }],
        )
        .unwrap_err();
    assert!(
        matches!(error,fd_state::ApplyError::Invalid(ref issues) if issues.iter().any(|i|i.code=="folder.bound_to_extension"))
    );
    assert!(
        store
            .get(&ResourceRef::new(ResourceKind::ManagedFolder, folder))
            .unwrap()
            .is_some()
    );
}

#[test]
fn systemd_templates_cannot_bypass_the_generated_sandbox() {
    assert!(
        fd_subsystems::runtime::service_command("[Service]\nExecStart=./sample\nUser=root\n")
            .is_err()
    );
    assert!(fd_subsystems::runtime::service_command("[Service]\nExecStart=/bin/sh\n").is_err());
    assert_eq!(
        fd_subsystems::runtime::service_command(
            "[Service]\nType=simple\nExecStart=./sample --flag\n"
        )
        .unwrap(),
        vec!["./sample", "--flag"]
    );
}
