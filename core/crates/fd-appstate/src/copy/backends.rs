use crate::{Result, atomic_write, invalid, safe_relative};
use fd_connectors::ownership::WriterSession;
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CopyKind {
    LocalPool,
    LocalSystemDisk,
    RemoteObjectStorage,
    RemoteRemovableDisk,
}
#[derive(Clone)]
pub enum Backend {
    Local(PathBuf),
    ScopedLocal(PathBuf, Arc<tokio::sync::OwnedMutexGuard<()>>),
    Remote(Arc<WriterSession>),
}
#[derive(Clone)]
pub struct CopyTarget {
    pub id: String,
    pub kind: CopyKind,
    pub pool_id: String,
    pub description: String,
    pub available: bool,
    pub backend: Backend,
}
impl Backend {
    pub async fn scoped(&self) -> Self {
        static LOCKS: std::sync::LazyLock<
            std::sync::Mutex<std::collections::BTreeMap<PathBuf, Arc<tokio::sync::Mutex<()>>>>,
        > = std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::BTreeMap::new()));
        match self {
            Self::Local(root) => {
                let lock = LOCKS
                    .lock()
                    .unwrap()
                    .entry(root.clone())
                    .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
                    .clone();
                Self::ScopedLocal(root.clone(), Arc::new(lock.lock_owned().await))
            }
            other => other.clone(),
        }
    }
    async fn local<T: Send + 'static>(
        &self,
        action: impl FnOnce(PathBuf) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let (root, guard) = match self {
            Self::Local(root) => (root.clone(), None),
            Self::ScopedLocal(root, guard) => (root.clone(), Some(guard.clone())),
            _ => return Err(invalid("state.backend_invalid")),
        };
        tokio::task::spawn_blocking(move || {
            let _guard = guard;
            action(root)
        })
        .await
        .map_err(|_| invalid("state.copy_worker_failed"))?
    }
    pub async fn read(&self, key: &str) -> Result<Option<Vec<u8>>> {
        safe_relative(key)?;
        let key = key.to_owned();
        match self {
            Self::Remote(remote) => remote
                .io
                .read(&key)
                .await
                .map(|o| o.map(|v| v.bytes))
                .map_err(|e| invalid(&e.to_string())),
            _ => {
                self.local(move |root| {
                    let path = local_path(&root, &key)?;
                    if std::fs::metadata(&path).is_ok_and(|m| m.len() > 128 * 1024 * 1024) {
                        return Err(invalid("state.too_large"));
                    }
                    match std::fs::read(path) {
                        Ok(bytes) => Ok(Some(bytes)),
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                        Err(e) => Err(e.into()),
                    }
                })
                .await
            }
        }
    }
    pub async fn write(&self, key: &str, bytes: &[u8]) -> Result<()> {
        safe_relative(key)?;
        match self {
            Self::Remote(remote) => remote
                .put(key, bytes)
                .await
                .map_err(|e| invalid(&e.to_string())),
            _ => {
                let key = key.to_owned();
                let bytes = bytes.to_vec();
                self.local(move |root| atomic_write(&local_path(&root, &key)?, &bytes))
                    .await
            }
        }
    }
    pub async fn write_index(&self, key: &str, bytes: &[u8]) -> Result<()> {
        match self {
            Self::Remote(remote) => {
                use fd_connectors::ownership::{ClaimSafety, WriteCondition};
                remote.check().await.map_err(|e| invalid(&e.to_string()))?;
                let condition = if matches!(remote.location.claim_safety, ClaimSafety::Conditional)
                {
                    match remote
                        .io
                        .read(key)
                        .await
                        .map_err(|e| invalid(&e.to_string()))?
                    {
                        Some(object) => WriteCondition::IfMatch(object.etag),
                        None => WriteCondition::CreateOnly,
                    }
                } else {
                    WriteCondition::Overwrite
                };
                remote
                    .io
                    .put(key, bytes, condition)
                    .await
                    .map_err(|e| invalid(&e.to_string()))?;
                Ok(())
            }
            _ => self.write(key, bytes).await,
        }
    }
    pub async fn delete(&self, key: &str) -> Result<()> {
        safe_relative(key)?;
        match self {
            Self::Remote(remote) => remote
                .delete(key)
                .await
                .map_err(|e| invalid(&e.to_string())),
            _ => {
                let key = key.to_owned();
                self.local(move |root| {
                    let path = local_path(&root, &key)?;
                    match std::fs::remove_file(path) {
                        Ok(()) => Ok(()),
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                        Err(e) => Err(e.into()),
                    }
                })
                .await
            }
        }
    }
    pub async fn list(&self, prefix: &str) -> Result<Vec<String>> {
        safe_relative(prefix.trim_end_matches('/'))?;
        match self {
            Self::Remote(remote) => remote
                .io
                .list(prefix)
                .await
                .map_err(|e| invalid(&e.to_string())),
            _ => {
                let prefix = prefix.trim_end_matches('/').to_owned();
                self.local(move |root| {
                    let path = local_path(&root, &prefix)?;
                    if !path.exists() {
                        return Ok(vec![]);
                    }
                    let mut out = vec![];
                    walk(&root, &path, &mut out)?;
                    Ok(out)
                })
                .await
            }
        }
    }
    pub async fn finish(&self) -> Result<()> {
        match self {
            Self::Remote(remote) => remote.finish().await.map_err(|e| invalid(&e.to_string())),
            _ => Ok(()),
        }
    }
}

pub fn local_path(root: &Path, key: &str) -> Result<PathBuf> {
    safe_relative(key)?;
    let mut ancestor = PathBuf::new();
    for component in root.components() {
        if matches!(component, std::path::Component::ParentDir) {
            return Err(invalid("state.path_invalid"));
        }
        ancestor.push(component);
        if ancestor
            .symlink_metadata()
            .is_ok_and(|m| m.file_type().is_symlink())
            && !(cfg!(target_os = "macos")
                && (ancestor == Path::new("/tmp") || ancestor == Path::new("/var")))
        {
            return Err(invalid("state.symlink_refused"));
        }
    }
    let mut path = root.to_path_buf();
    if root
        .symlink_metadata()
        .is_ok_and(|m| m.file_type().is_symlink())
    {
        return Err(invalid("state.symlink_refused"));
    }
    for segment in key.split('/') {
        path.push(segment);
        if path
            .symlink_metadata()
            .is_ok_and(|m| m.file_type().is_symlink())
        {
            return Err(invalid("state.symlink_refused"));
        }
    }
    Ok(path)
}
fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<()> {
    if out.len() > 2048 {
        return Err(invalid("state.too_many_files"));
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            walk(root, &entry.path(), out)?;
        } else if ty.is_file() {
            out.push(
                entry
                    .path()
                    .strip_prefix(root)
                    .map_err(|_| invalid("state.path_invalid"))?
                    .to_string_lossy()
                    .into_owned(),
            );
        }
    }
    Ok(())
}
