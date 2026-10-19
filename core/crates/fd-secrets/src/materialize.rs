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
        // Preserve the inode and its consumer ACL when the credential has not changed.
        // Atomic replacement is still required when rotating a secret.
        if fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.file_type().is_file())
            && fs::read(&path)
                .map(zeroize::Zeroizing::new)
                .is_ok_and(|current| current.as_slice() == plaintext.as_slice())
        {
            files.push(path);
            continue;
        }
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o400)
            .open(path.with_extension("tmp"))?;
        file.write_all(&plaintext)?;
        file.sync_all()?;
        let mut perms = file.metadata()?.permissions();
        perms.set_mode(0o400);
        fs::set_permissions(path.with_extension("tmp"), perms)?;
        fs::rename(path.with_extension("tmp"), &path)?;
        files.push(path);
    }
    if dir.exists() {
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            // Session-owned subdirectories have their own lifetime and Drop cleanup.
            if entry.file_type()?.is_dir()
                && (name.starts_with("session-") || name.starts_with("restore-"))
            {
                continue;
            }
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
        use std::os::unix::fs::MetadataExt;
        let key_path = root.join("fd-s3/connection__1__access-key");
        let inode = fs::metadata(&key_path).unwrap().ino();
        materialize(&store, &root, "fd-s3", &["connection/1/access-key".into()]).unwrap();
        assert_eq!(fs::metadata(&key_path).unwrap().ino(), inode);
        store
            .put("connection/1/access-key", b"ROTATED", vec!["fd-s3".into()])
            .unwrap();
        materialize(&store, &root, "fd-s3", &["connection/1/access-key".into()]).unwrap();
        assert_eq!(fs::read(&key_path).unwrap(), b"ROTATED");
        assert_ne!(fs::metadata(&key_path).unwrap().ino(), inode);
        assert!(!root.join("fd-s3/connection__1__secret-key").exists());
        assert!(root.join("fd-s3/connection__1__access-key").exists());
    }
    #[test]
    fn route_cleanup_preserves_active_session_directories() {
        let dir = tempfile::tempdir().unwrap();
        let store = SecretStore::open(dir.path().join("s.enc"), dir.path().join("k")).unwrap();
        let root = dir.path().join("run");
        let session = root.join("fd-s3/session-test");
        fs::create_dir_all(&session).unwrap();
        fs::write(session.join("access"), b"active").unwrap();
        fs::write(root.join("fd-s3/obsolete"), b"obsolete").unwrap();
        materialize(&store, &root, "fd-s3", &[]).unwrap();
        assert!(session.join("access").is_file());
        assert!(!root.join("fd-s3/obsolete").exists());
    }
}
