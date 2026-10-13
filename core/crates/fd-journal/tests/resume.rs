//! A subprocess dies after its side effect, before the step result is durable.
use fd_journal::{
    Journal, JournalError, MIGRATION_0002_JOURNAL, Operation, OperationState, StepOutcome,
    StepRunner,
};
use fd_state::db::{Db, MIGRATION_0001_INIT};
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};
struct Runner {
    effect: PathBuf,
    barrier: Option<PathBuf>,
}
impl StepRunner for Runner {
    fn kind(&self) -> &'static str {
        "Sample"
    }
    fn steps(&self) -> &'static [&'static str] {
        &["first", "second"]
    }
    fn run_step(
        &self,
        _: &Operation,
        step: &str,
        intent: &serde_json::Value,
    ) -> Result<StepOutcome, JournalError> {
        assert_eq!(intent["step"], step);
        let path = self.effect.join(step);
        if !path.exists() {
            std::fs::write(&path, b"once").unwrap();
        }
        if let Some(barrier) = &self.barrier {
            std::fs::write(barrier, b"acting").unwrap();
            std::thread::sleep(Duration::from_secs(120));
        }
        Ok(StepOutcome::Done {
            result: serde_json::json!({"effect":step}),
            detail: String::new(),
        })
    }
}
fn journal(path: &Path) -> Journal {
    Journal::new(Db::open(path, &[MIGRATION_0001_INIT, MIGRATION_0002_JOURNAL]).unwrap())
}
#[test]
#[ignore]
fn child_step() {
    let db = std::env::var("FD_JOURNAL_DB").unwrap();
    let id = std::env::var("FD_JOURNAL_ID").unwrap();
    let dir = PathBuf::from(std::env::var("FD_JOURNAL_DIR").unwrap());
    journal(Path::new(&db))
        .run_next(
            &Runner {
                effect: dir.clone(),
                barrier: Some(dir.join("barrier")),
            },
            &id,
        )
        .unwrap();
}
#[test]
fn interrupted_side_effect_resumes_idempotently_in_order() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("core.db");
    let j = journal(&db);
    let op = j
        .start(
            "Sample",
            serde_json::json!({}),
            None,
            "test",
            Runner {
                effect: dir.path().into(),
                barrier: None,
            }
            .steps(),
        )
        .unwrap();
    drop(j);
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "child_step"])
        .env("FD_JOURNAL_DB", &db)
        .env("FD_JOURNAL_ID", &op.id)
        .env("FD_JOURNAL_DIR", dir.path())
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !dir.path().join("barrier").exists() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    child.kill().unwrap();
    child.wait().unwrap();
    let j = journal(&db);
    let acting = j.get(&op.id).unwrap();
    assert_eq!(acting.steps[0].state, "acting");
    assert!(acting.steps[0].intent.is_some());
    let runner = Runner {
        effect: dir.path().into(),
        barrier: None,
    };
    j.resume_all(&[&runner]).unwrap();
    let done = j.get(&op.id).unwrap();
    assert_eq!(done.state, OperationState::Succeeded);
    assert_eq!(done.steps[0].attempts, 2);
    assert_eq!(done.steps[0].intent.as_ref().unwrap()["attempt"], 1);
    assert_eq!(std::fs::read(dir.path().join("first")).unwrap(), b"once");
    assert!(dir.path().join("second").exists());
}
#[test]
fn unknown_runner_is_visible_and_retry_does_not_spin() {
    struct Retry;
    impl StepRunner for Retry {
        fn kind(&self) -> &'static str {
            "Retry"
        }
        fn steps(&self) -> &'static [&'static str] {
            &["one"]
        }
        fn run_step(
            &self,
            _: &Operation,
            _: &str,
            _: &serde_json::Value,
        ) -> Result<StepOutcome, JournalError> {
            Ok(StepOutcome::Retry {
                detail: "later".into(),
            })
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let j = journal(&dir.path().join("db"));
    let pending = j
        .start("Retry", serde_json::json!({}), None, "test", Retry.steps())
        .unwrap();
    let unknown = j
        .start("Missing", serde_json::json!({}), None, "test", &["one"])
        .unwrap();
    j.resume_all(&[&Retry]).unwrap();
    assert_eq!(j.get(&pending.id).unwrap().steps[0].attempts, 1);
    assert_eq!(j.get(&unknown.id).unwrap().state, OperationState::Failed);
}
