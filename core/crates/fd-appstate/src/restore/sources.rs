use crate::{
    Result,
    copy::{backends::local_path, index::Index},
    format::{
        fdbackup,
        fdstate::{self, SnapshotPayload},
    },
    invalid,
};
use fd_connectors::ownership::ObjectIo;
use fd_secrets::{RecoveryKeyStore, recovery_key::RecoveryKeyInfo};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use zeroize::Zeroizing;
#[derive(Clone)]
pub enum ReadBackend {
    Local(PathBuf),
    Remote(Arc<dyn ObjectIo>),
    Files(Arc<BTreeMap<String, Vec<u8>>>),
}
impl ReadBackend {
    pub async fn read(&self, path: &str) -> Result<Option<Vec<u8>>> {
        crate::safe_relative(path)?;
        let data = match self {
            Self::Local(root) => {
                let p = local_path(root, path)?;
                if std::fs::metadata(&p).is_ok_and(|m| m.len() > fdbackup::MAX_BACKUP_BYTES as u64)
                {
                    return Err(invalid("state.too_large"));
                }
                match std::fs::read(p) {
                    Ok(b) => Some(b),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                    Err(e) => return Err(e.into()),
                }
            }
            Self::Remote(io) => io
                .read(path)
                .await
                .map_err(|e| invalid(&e.to_string()))?
                .map(|o| o.bytes),
            Self::Files(files) => files.get(path).cloned(),
        };
        if data
            .as_ref()
            .is_some_and(|v| v.len() > fdbackup::MAX_BACKUP_BYTES)
        {
            return Err(invalid("state.too_large"));
        }
        Ok(data)
    }
    pub async fn indices(&self) -> Result<Vec<String>> {
        match self {
            Self::Files(files) => Ok(files
                .keys()
                .filter(|p| p.ends_with("/index.json"))
                .cloned()
                .collect()),
            Self::Local(root) => {
                let mut paths = Vec::new();
                let folder = root.join("appliance");
                if folder.is_dir() {
                    for d in std::fs::read_dir(folder)? {
                        let d = d?;
                        if d.file_type()?.is_dir() {
                            let id = d.file_name().to_string_lossy().into_owned();
                            if uuid::Uuid::parse_str(&id).is_ok()
                                && d.path().join("index.json").is_file()
                            {
                                paths.push(format!("appliance/{id}/index.json"));
                            }
                        }
                    }
                }
                Ok(paths)
            }
            Self::Remote(io) => Ok(io
                .list("appliance/")
                .await
                .map_err(|e| invalid(&e.to_string()))?
                .into_iter()
                .filter(|p| p.ends_with("/index.json"))
                .collect()),
        }
    }
}
pub struct Source {
    pub id: String,
    pub backend: ReadBackend,
    pub index: Index,
    pub key: RecoveryKeyInfo,
    pub masters: BTreeMap<String, Zeroizing<[u8; 32]>>,
    pub local: bool,
    pub created: Instant,
    pub recorded_remote_source_id: Option<String>,
    pub alternate_source_ids: Vec<String>,
}
impl Source {
    pub async fn open(backend: ReadBackend, recovery_key: &str, local: bool) -> Result<Self> {
        static KDF: std::sync::LazyLock<tokio::sync::Semaphore> =
            std::sync::LazyLock::new(|| tokio::sync::Semaphore::new(1));
        let _permit = KDF
            .acquire()
            .await
            .map_err(|_| invalid("restore.key_busy"))?;
        for path in backend.indices().await?.into_iter().take(16) {
            let Some(raw) = backend.read(&path).await? else {
                continue;
            };
            if raw.len() > 2 * 1024 * 1024 {
                continue;
            }
            let Ok(index) = serde_json::from_slice::<Index>(&raw) else {
                continue;
            };
            if index.schema_version > 1 {
                return Err(invalid("backup_file.too_new"));
            }
            let root = format!("appliance/{}/", index.appliance_id);
            if path != format!("{root}index.json") {
                continue;
            }
            for key_id in std::iter::once(index.signing_key_id.clone()) {
                let key_path = format!("{root}keys/{key_id}.json");
                let Some(raw) = backend.read(&key_path).await? else {
                    continue;
                };
                if raw.len() > 256 * 1024 {
                    continue;
                }
                let Ok(info) = serde_json::from_slice::<RecoveryKeyInfo>(&raw) else {
                    continue;
                };
                let metadata = info.clone();
                let key = recovery_key.to_owned();
                let Ok(Ok(master)) =
                    tokio::task::spawn_blocking(move || RecoveryKeyStore::unlock(&metadata, &key))
                        .await
                else {
                    continue;
                };
                if index.verify(&master).is_err() {
                    continue;
                }
                let masters = RecoveryKeyStore::unlocked_keys(&info, master)
                    .map_err(|_| invalid("recovery.key_invalid"))?;
                return Ok(Self {
                    id: uuid::Uuid::new_v4().to_string(),
                    backend,
                    index,
                    key: info,
                    masters: masters
                        .into_iter()
                        .map(|(id, key)| (id, Zeroizing::new(key)))
                        .collect(),
                    local,
                    created: Instant::now(),
                    recorded_remote_source_id: None,
                    alternate_source_ids: Vec::new(),
                });
            }
        }
        Err(invalid("recovery.key_invalid"))
    }
    pub fn valid(&self) -> bool {
        self.created.elapsed() < Duration::from_secs(15 * 60)
    }
    pub async fn snapshot(&self, id: &str) -> Result<SnapshotPayload> {
        if !self.valid() {
            return Err(invalid("restore.session_expired"));
        }
        let entry = self
            .index
            .snapshots
            .iter()
            .find(|e| e.snapshot_id == id)
            .ok_or_else(|| invalid("restore.snapshot_not_found"))?;
        let bytes = self
            .backend
            .read(&format!(
                "appliance/{}/{}",
                self.index.appliance_id, entry.file
            ))
            .await?
            .ok_or_else(|| invalid("restore.snapshot_missing"))?;
        if bytes.len() as u64 != entry.size_bytes
            || hex::encode(Sha256::digest(&bytes)) != entry.sha256
        {
            return Err(invalid("restore.integrity_failed"));
        }
        let master = self
            .masters
            .get(&entry.key_id)
            .ok_or_else(|| invalid("restore.key_epoch_unavailable"))?;
        let payload = fdstate::decode(&bytes, master)?;
        crate::fdconfig::validate(&payload.config)?;
        if payload.manifest["applianceId"].as_str() != Some(&self.index.appliance_id)
            || payload.config["appliance"]["id"].as_str() != Some(&self.index.appliance_id)
        {
            return Err(invalid("restore.identity_mismatch"));
        }
        Ok(payload)
    }
    pub async fn snapshots(&self) -> Result<serde_json::Value> {
        let mut records = Vec::new();
        for entry in &self.index.snapshots {
            let valid = self.snapshot(&entry.snapshot_id).await.is_ok();
            records.push(serde_json::json!({"snapshotId":entry.snapshot_id,"takenAt":entry.taken_at,"applianceName":self.index.appliance_name,"integrityOk":valid,"keyId":entry.key_id,"restoreSourceId":self.id,"newestValid":false}));
        }
        records.sort_by(|a, b| b["takenAt"].as_str().cmp(&a["takenAt"].as_str()));
        if let Some(newest) = records.iter_mut().find(|r| r["integrityOk"] == true) {
            newest["newestValid"] = true.into();
        }
        Ok(serde_json::json!({"snapshots":records}))
    }
    pub async fn package(&self, hash: &str) -> Result<Option<Vec<u8>>> {
        if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(invalid("restore.package_invalid"));
        }
        let Some(raw) = self
            .backend
            .read(&format!(
                "appliance/{}/packages/{hash}.fdpkg",
                self.index.appliance_id
            ))
            .await?
        else {
            return Ok(None);
        };
        for master in self.masters.values() {
            if let Ok(plain) = fdstate::decrypt_bytes(&raw, master, b"FDPKG\0\0\0")
                && hex::encode(Sha256::digest(&plain)) == hash
            {
                return Ok(Some(plain));
            }
        }
        Ok(None)
    }
}
