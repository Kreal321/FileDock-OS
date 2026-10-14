//! Consistent configuration snapshots and recovery. Never backs up runtime history.
pub mod copy;
pub mod detect;
pub mod engine;
pub mod fdconfig;
pub mod format;
pub mod packages;
pub mod restore;
pub mod scheduler;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::Path,
};
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("state.io: {0}")]
    Io(#[from] std::io::Error),
    #[error("state.database: {0}")]
    Db(#[from] fd_state::DbError),
    #[error("state.json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Invalid(String),
}
pub type Result<T> = std::result::Result<T, Error>;
pub fn invalid(message: &str) -> Error {
    Error::Invalid(message.into())
}
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().ok_or_else(|| invalid("state.path_invalid"))?;
    fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(".{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        fs::rename(&temporary, path)?;
        fs::File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}
pub fn safe_relative(path: &str) -> Result<()> {
    if path.is_empty()
        || path.starts_with('/')
        || path.contains('\\')
        || path
            .split('/')
            .any(|s| s.is_empty() || s == "." || s == "..")
    {
        return Err(invalid("state.path_invalid"));
    }
    Ok(())
}
pub fn canonical(value: &serde_json::Value) -> Result<Vec<u8>> {
    Ok(serde_json::to_vec(value)?)
}

pub mod detect_linux;
