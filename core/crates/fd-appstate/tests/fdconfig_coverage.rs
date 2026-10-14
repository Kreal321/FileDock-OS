use fd_appstate::fdconfig::{section_for, serialize};
use fd_state::{Db, ResourceKind, db::MIGRATION_0001_INIT};
use serde_json::json;
#[test]
fn every_resource_kind_has_a_declared_configuration_section() {
    for kind in ResourceKind::ALL {
        assert!(!section_for(kind).is_empty());
    }
}
#[test]
fn disk_identity_and_all_runtime_history_are_excluded() {
    let db = Db::open_in_memory(&[MIGRATION_0001_INIT]).unwrap();
    db.with(|c| c.execute("INSERT INTO resource(kind,id,generation,spec,status,deleted,change_seq,created_at,updated_at) VALUES('Pool','pool1',1,?1,?2,0,1,'now','now')",rusqlite::params![json!({"name":"media","kind":"Lvm","layers":["lvm"],"memberDisks":[{"serial":"PRIVATE-DISK"}]}).to_string(),json!({"capacity":123}).to_string()])).unwrap();
    let document = serialize(
        &db,
        "appliance1",
        json!({"administrators":[],"apiTokens":[],"settings":{}}),
    )
    .unwrap();
    let text = document.to_string();
    assert!(!text.contains("PRIVATE-DISK"));
    assert!(!text.contains("capacity"));
    assert_eq!(document["schema"], "fdconfig/1");
    assert_eq!(document["core"]["pools"][0]["id"], "pool1");
    for forbidden in [
        "auditEntries",
        "sessions",
        "setupState",
        "operations",
        "alerts",
        "snapshotRecords",
        "connectionTestResults",
    ] {
        assert!(document.get(forbidden).is_none());
    }
}
