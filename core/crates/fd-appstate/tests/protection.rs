use fd_appstate::{
    copy::{backends::CopyKind, index::SnapshotEntry, retention},
    engine::{CopyRecord, Engine},
};
use fd_state::{
    Db,
    db::{self, MIGRATION_0001_INIT},
};
use serde_json::json;
#[test]
fn blocked_copy_becomes_stale_from_actual_change_time() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("db"), &[MIGRATION_0001_INIT]).unwrap();
    let record = CopyRecord {
        id: "remote".into(),
        kind: CopyKind::RemoteObjectStorage,
        pool_id: String::new(),
        description: "remote".into(),
        last_snapshot_at: Some(chrono::Utc::now().to_rfc3339()),
        last_snapshot_id: Some(uuid::Uuid::new_v4().to_string()),
        covered_fingerprint: "old".into(),
        covered_core_seq: 1,
        covered_management_seq: 1,
        last_error: "Unavailable".into(),
        available: false,
    };
    db.with(|c| {
        db::put_setting(c, "state.copies", &json!([record]))?;
        db::put_setting(c, "state.latest_fingerprint", &json!("new"))?;
        db::put_setting(
            c,
            "state.latest_change_at",
            &json!((chrono::Utc::now() - chrono::Duration::seconds(299)).to_rfc3339()),
        )
    })
    .unwrap();
    let status = Engine::protection(&db).unwrap();
    assert_eq!(status["copies"][0]["stale"], false);
    assert_eq!(status["warning"], true);
    db.with(|c| {
        db::put_setting(
            c,
            "state.latest_change_at",
            &json!((chrono::Utc::now() - chrono::Duration::seconds(301)).to_rfc3339()),
        )
    })
    .unwrap();
    let status = Engine::protection(&db).unwrap();
    assert_eq!(status["copies"][0]["stale"], true);
    assert_eq!(status["machineLossCovered"], false);
    db.with(|c| db::put_setting(c, "state.latest_fingerprint", &json!("old")))
        .unwrap();
    assert_eq!(Engine::protection(&db).unwrap()["machineLossCovered"], true);
}
#[test]
fn retention_keeps_recent_hourly_and_daily_recovery_points() {
    let now = chrono::Utc::now();
    let entries = (0..120)
        .map(|i| SnapshotEntry {
            snapshot_id: i.to_string(),
            taken_at: (now - chrono::Duration::hours(i)).to_rfc3339(),
            file: String::new(),
            size_bytes: 0,
            sha256: String::new(),
            key_id: String::new(),
            retention_class: String::new(),
            packages: vec![],
            release_version: String::new(),
        })
        .collect::<Vec<_>>();
    let kept = retention::retained(&entries);
    for i in 0..30 {
        assert!(kept.contains(&i.to_string()));
    }
    assert!(kept.len() > 30);
    assert!(kept.len() <= 84);
    let reversed = entries.into_iter().rev().collect::<Vec<_>>();
    assert_eq!(kept, retention::retained(&reversed));
}
