//! Kill a separate process while the real ApplyChange transaction has partial SQL writes.
use fd_state::db::{Db, MIGRATION_0001_INIT};
use fd_state::{Change, ChangeOrigin, ResourceKind, ResourceRef, StateStore};
use std::{
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
const POOL: &str = "0192d000-0000-7000-8000-000000000001";
fn store(path: &Path) -> StateStore {
    StateStore::new(Db::open(path, &[MIGRATION_0001_INIT]).unwrap())
}
fn batch() -> Vec<Change> {
    vec![
        Change::Upsert {
            resource: ResourceRef::singleton(ResourceKind::Settings),
            expected_generation: 1,
            spec: serde_json::json!({"applianceName":"after"}),
        },
        Change::Upsert {
            resource: ResourceRef::new(ResourceKind::Pool, POOL),
            expected_generation: 0,
            spec: serde_json::json!({"name":"data","kind":"Adopted","memberDisks":[{"serial":"sample"}]}),
        },
    ]
}
#[test]
#[ignore]
fn child_apply() {
    let path = std::env::var("FD_ATOMICITY_DB").unwrap();
    let state = store(Path::new(&path));
    if let Ok(marker) = std::env::var("FD_ATOMICITY_MARKER") {
        state.db().with(|conn| {
   conn.create_scalar_function("crash_barrier",0,rusqlite::functions::FunctionFlags::SQLITE_UTF8,move |_| { std::fs::write(&marker,b"uncommitted").unwrap(); std::thread::sleep(Duration::from_secs(120)); Ok(1) }).unwrap();
   conn.execute_batch(&format!("CREATE TEMP TRIGGER crash_after_partial_write AFTER INSERT ON resource WHEN NEW.id='{POOL}' BEGIN SELECT crash_barrier(); END;")).unwrap();
  });
    }
    state
        .apply(
            ChangeOrigin::Console {
                actor: "test".into(),
            },
            &batch(),
        )
        .unwrap();
}
#[test]
fn killed_batch_is_all_or_nothing_and_acknowledged_commit_survives_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("core.db");
    let marker = dir.path().join("marker");
    let state = store(&db);
    state
        .apply(
            ChangeOrigin::Console {
                actor: "test".into(),
            },
            &[Change::Upsert {
                resource: ResourceRef::singleton(ResourceKind::Settings),
                expected_generation: 0,
                spec: serde_json::json!({"applianceName":"before"}),
            }],
        )
        .unwrap();
    drop(state);
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "child_apply"])
        .env("FD_ATOMICITY_DB", &db)
        .env("FD_ATOMICITY_MARKER", &marker)
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !marker.exists() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    child.kill().unwrap();
    child.wait().unwrap();
    let state = store(&db);
    assert_eq!(
        state
            .get(&ResourceRef::singleton(ResourceKind::Settings))
            .unwrap()
            .unwrap()
            .spec["applianceName"],
        "before"
    );
    assert!(
        state
            .get(&ResourceRef::new(ResourceKind::Pool, POOL))
            .unwrap()
            .is_none()
    );
    assert_eq!(state.change_seq().unwrap(), 1);
    drop(state);
    assert!(
        Command::new(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", "child_apply"])
            .env("FD_ATOMICITY_DB", &db)
            .stdout(Stdio::null())
            .status()
            .unwrap()
            .success()
    );
    let state = store(&db);
    assert_eq!(
        state
            .get(&ResourceRef::singleton(ResourceKind::Settings))
            .unwrap()
            .unwrap()
            .spec["applianceName"],
        "after"
    );
    assert!(
        state
            .get(&ResourceRef::new(ResourceKind::Pool, POOL))
            .unwrap()
            .is_some()
    );
    assert_eq!(state.change_seq().unwrap(), 2);
}
