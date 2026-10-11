//! Per-subsystem tmpfs files (FR-031, FR-116, FR-119).

use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use crate::store::{SecretStore, StoreError};

#[derive(Clone, Debug)]
pub struct Materialized {
    pub subsystem: String,
    pub files: Vec<PathBuf>,
}

/// Writes only the secrets `references` names into `/run/filedock/secrets/<subsystem>/`.
/// Files that are no longer referenced are removed.
///
/// When a connection's `BackupRepository` route targets `backup-engine`, the Core includes that
/// connection's credentials in `references` for `backup-engine`, and never for a connector
/// (FR-116, FR-119).
pub fn materialize(
    store: &SecretStore,
    root: impl AsRef<Path>,
    subsystem: &str,
    references: &[String],
) -> Result<Materialized, StoreError> {
    let dir = root.as_ref().join(subsystem);
    fs::create_dir_all(&dir)?;
    let mut keep = BTreeSet::new();
    let mut files = Vec::new();
    for reference in references {
        let name = file_name(reference);
        keep.insert(name.clone());
        let path = dir.join(&name);
        let plaintext = store.get(reference)?;
        let _ = fs::remove_file(&path);
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o400)
            .open(&path)?;
        file.write_all(&plaintext)?;
        file.sync_all()?;
        let mut perms = file.metadata()?.permissions();
        perms.set_mode(0o400);
        fs::set_permissions(&path, perms)?;
        files.push(path);
    }
    if dir.exists() {
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if !keep.contains(&name) {
                fs::remove_file(entry.path())?;
            }
        }
    }
    Ok(Materialized {
        subsystem: subsystem.into(),
        files,
    })
}

fn file_name(reference: &str) -> String {
    reference.replace('/', "__")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::SecretStore;

    #[test]
    fn writes_and_removes_unreferenced() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = SecretStore::open(dir.path().join("s.enc"), dir.path().join("k")).unwrap();
        store
            .put("connection/1/access-key", b"AK", vec!["fd-s3".into()])
            .unwrap();
        store
            .put("connection/1/secret-key", b"SK", vec!["fd-s3".into()])
            .unwrap();
        let root = dir.path().join("run");
        materialize(
            &store,
            &root,
            "fd-s3",
            &[
                "connection/1/access-key".into(),
                "connection/1/secret-key".into(),
            ],
        )
        .unwrap();
        assert_eq!(
            std::fs::read(root.join("fd-s3/connection__1__access-key")).unwrap(),
            b"AK"
        );
        materialize(&store, &root, "fd-s3", &["connection/1/access-key".into()]).unwrap();
        assert!(!root.join("fd-s3/connection__1__secret-key").exists());
        assert!(root.join("fd-s3/connection__1__access-key").exists());
    }
}
