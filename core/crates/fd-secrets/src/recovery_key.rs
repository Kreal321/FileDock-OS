//! Recovery key generation, wrapping, and group confirmation (FR-032 to FR-034, R11).

use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::Zeroize;

use crate::store::StoreError;

/// Crockford base32 without I, L, O, U.
pub const ALPHABET: &[u8] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
pub const KEY_LEN: usize = 25;
pub const GROUP_LEN: usize = 5;
pub const GROUP_COUNT: usize = 5;

const SNAPSHOT_KEY_LEN: usize = 32;
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 24;

#[derive(Debug, thiserror::Error)]
pub enum RecoveryError {
    #[error("{0}")]
    Failed(String),
    #[error("recovery key {0} was not found")]
    NotFound(String),
    #[error("confirmation does not match")]
    ConfirmMismatch,
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("crypto: {0}")]
    Crypto(String),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

impl From<StoreError> for RecoveryError {
    fn from(value: StoreError) -> Self {
        RecoveryError::Failed(value.to_string())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecoveryKeyInfo {
    pub key_id: String,
    pub salt: String,
    pub kdf_params: KdfParams,
    pub wrapped_snapshot_key: String,
    pub nonce: String,
    pub created_at: String,
    pub superseded_at: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KdfParams {
    pub memory_kib: u32,
    pub iterations: u32,
    pub parallelism: u32,
}

#[derive(Clone, Debug)]
pub struct GeneratedKey {
    pub key_id: String,
    pub recovery_key: String,
    pub confirm_group_indexes: Vec<u32>,
}

#[derive(Serialize, Deserialize)]
struct FileBody {
    snapshot_key: String,
    keys: Vec<RecoveryKeyInfo>,
    pending: Option<PendingConfirm>,
}

#[derive(Serialize, Deserialize)]
struct PendingConfirm {
    key_id: String,
    groups: BTreeMap<u32, String>,
}

pub struct RecoveryKeyStore {
    path: PathBuf,
    dek: [u8; SNAPSHOT_KEY_LEN],
    body: FileBody,
}

impl RecoveryKeyStore {
    pub fn open(
        path: impl AsRef<Path>,
        dek: &[u8; SNAPSHOT_KEY_LEN],
    ) -> Result<Self, RecoveryError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let body = if path.exists() {
            decrypt_body(&fs::read(&path)?, dek)?
        } else {
            FileBody {
                snapshot_key: hex::encode(random_bytes::<SNAPSHOT_KEY_LEN>()),
                keys: Vec::new(),
                pending: None,
            }
        };
        Ok(Self {
            path,
            dek: *dek,
            body,
        })
    }

    pub fn generate(&mut self, regenerate: bool) -> Result<GeneratedKey, RecoveryError> {
        if !regenerate
            && self.body.pending.is_none()
            && self.body.keys.iter().any(|k| k.superseded_at.is_none())
        {
            return Err(RecoveryError::Failed(
                "a recovery key already exists; pass regenerate to replace it".into(),
            ));
        }
        if !regenerate && self.body.pending.is_some() {
            self.body.keys.retain(|k| k.superseded_at.is_some());
            self.body.pending = None;
        }
        if regenerate {
            let now = now_rfc3339();
            for key in &mut self.body.keys {
                if key.superseded_at.is_none() {
                    key.superseded_at = Some(now.clone());
                }
            }
        }

        let raw = random_key();
        let formatted = format_key(&raw);
        let salt = random_bytes::<SALT_LEN>();
        let params = kdf_params();
        let wrap = derive_wrap(&formatted, &salt, &params)?;
        let nonce = random_bytes::<NONCE_LEN>();
        let snapshot = hex::decode(&self.body.snapshot_key)
            .map_err(|e| RecoveryError::Crypto(e.to_string()))?;
        let cipher = XChaCha20Poly1305::new(&Key::from(wrap));
        let wrapped = cipher
            .encrypt(&XNonce::from(nonce), snapshot.as_ref())
            .map_err(|e| RecoveryError::Crypto(e.to_string()))?;
        let key_id = hex::encode(&Sha256::digest(formatted.as_bytes())[..8]);
        let info = RecoveryKeyInfo {
            key_id: key_id.clone(),
            salt: hex::encode(salt),
            kdf_params: params,
            wrapped_snapshot_key: hex::encode(wrapped),
            nonce: hex::encode(nonce),
            created_at: now_rfc3339(),
            superseded_at: None,
        };
        let indexes = confirm_indexes();
        let groups = groups_of(&formatted);
        let mut pending = BTreeMap::new();
        for index in &indexes {
            pending.insert(*index, groups[*index as usize].clone());
        }
        self.body.keys.push(info);
        self.body.pending = Some(PendingConfirm {
            key_id: key_id.clone(),
            groups: pending,
        });
        self.persist()?;
        Ok(GeneratedKey {
            key_id,
            recovery_key: formatted,
            confirm_group_indexes: indexes,
        })
    }

    pub fn confirm(
        &mut self,
        key_id: &str,
        groups: &BTreeMap<u32, String>,
    ) -> Result<(), RecoveryError> {
        if self.body.pending.is_none() && self.current().is_some_and(|k| k.key_id == key_id) {
            return Ok(());
        }
        let pending = self
            .body
            .pending
            .as_ref()
            .ok_or(RecoveryError::ConfirmMismatch)?;
        if pending.key_id != key_id {
            return Err(RecoveryError::ConfirmMismatch);
        }
        if pending.groups.len() != groups.len() {
            return Err(RecoveryError::ConfirmMismatch);
        }
        for (index, expected) in &pending.groups {
            let got = groups.get(index).ok_or(RecoveryError::ConfirmMismatch)?;
            if normalize_group(got) != normalize_group(expected) {
                return Err(RecoveryError::ConfirmMismatch);
            }
        }
        self.body
            .keys
            .iter()
            .find(|k| k.key_id == key_id)
            .ok_or_else(|| RecoveryError::NotFound(key_id.into()))?;
        self.body.pending = None;
        self.persist()?;
        Ok(())
    }

    pub fn current(&self) -> Option<&RecoveryKeyInfo> {
        self.body
            .keys
            .iter()
            .rev()
            .find(|k| k.superseded_at.is_none())
    }

    fn persist(&self) -> Result<(), RecoveryError> {
        let plaintext = serde_json::to_vec(&self.body)?;
        let nonce = random_bytes::<NONCE_LEN>();
        let cipher = XChaCha20Poly1305::new(&Key::from(self.dek));
        let ciphertext = cipher
            .encrypt(&XNonce::from(nonce), plaintext.as_ref())
            .map_err(|e| RecoveryError::Crypto(e.to_string()))?;
        let mut file = Vec::new();
        file.extend_from_slice(&nonce);
        file.extend_from_slice(&ciphertext);
        let mut out = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(self.path.with_extension("tmp"))?;
        out.write_all(&file)?;
        out.sync_all()?;
        fs::rename(self.path.with_extension("tmp"), &self.path)?;
        Ok(())
    }
}

fn decrypt_body(bytes: &[u8], dek: &[u8; SNAPSHOT_KEY_LEN]) -> Result<FileBody, RecoveryError> {
    if bytes.len() < NONCE_LEN {
        return Err(RecoveryError::Crypto("truncated recovery store".into()));
    }
    let (nonce, rest) = bytes.split_at(NONCE_LEN);
    let cipher = XChaCha20Poly1305::new(&Key::from(*dek));
    let plaintext = cipher
        .decrypt(
            &XNonce::try_from(nonce).map_err(|e| RecoveryError::Crypto(e.to_string()))?,
            rest,
        )
        .map_err(|e| RecoveryError::Crypto(e.to_string()))?;
    Ok(serde_json::from_slice(&plaintext)?)
}

fn kdf_params() -> KdfParams {
    let memory_kib = std::env::var("FD_RECOVERY_ARGON_KIB")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(256 * 1024);
    KdfParams {
        memory_kib,
        iterations: 3,
        parallelism: 1,
    }
}

fn derive_wrap(key: &str, salt: &[u8], params: &KdfParams) -> Result<[u8; 32], RecoveryError> {
    let argon_params = Params::new(
        params.memory_kib,
        params.iterations,
        params.parallelism,
        Some(32),
    )
    .map_err(|e| RecoveryError::Crypto(e.to_string()))?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, argon_params);
    let mut wrap = [0u8; 32];
    argon
        .hash_password_into(key.as_bytes(), salt, &mut wrap)
        .map_err(|e| RecoveryError::Crypto(e.to_string()))?;
    Ok(wrap)
}

fn random_key() -> [u8; KEY_LEN] {
    let mut bytes = random_bytes::<KEY_LEN>();
    let mut out = [0u8; KEY_LEN];
    for (i, b) in bytes.iter().enumerate() {
        out[i] = ALPHABET[(*b as usize) % ALPHABET.len()];
    }
    bytes.zeroize();
    out
}

pub fn format_key(raw: &[u8; KEY_LEN]) -> String {
    raw.chunks(GROUP_LEN)
        .map(|c| std::str::from_utf8(c).unwrap_or("?????").to_string())
        .collect::<Vec<_>>()
        .join("-")
}

fn groups_of(formatted: &str) -> Vec<String> {
    formatted.split('-').map(str::to_owned).collect()
}

fn normalize_group(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

fn confirm_indexes() -> Vec<u32> {
    let mut bytes = random_bytes::<2>();
    let first = u32::from(bytes[0]) % GROUP_COUNT as u32;
    let mut second = u32::from(bytes[1]) % GROUP_COUNT as u32;
    if second == first {
        second = (first + 1) % GROUP_COUNT as u32;
    }
    bytes.zeroize();
    let mut indexes = vec![first, second];
    indexes.sort_unstable();
    indexes
}

fn random_bytes<const N: usize>() -> [u8; N] {
    let mut buf = [0u8; N];
    getrandom::fill(&mut buf).expect("entropy");
    buf
}

fn now_rfc3339() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{secs}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_and_confirm() {
        unsafe { std::env::set_var("FD_RECOVERY_ARGON_KIB", "8") };
        let dir = tempfile::tempdir().unwrap();
        let dek = [7u8; 32];
        let mut store = RecoveryKeyStore::open(dir.path().join("rk.enc"), &dek).unwrap();
        let generated = store.generate(false).unwrap();
        assert_eq!(
            generated.recovery_key.chars().filter(|c| *c != '-').count(),
            25
        );
        assert_eq!(generated.confirm_group_indexes.len(), 2);
        let groups = groups_of(&generated.recovery_key);
        let mut provided = BTreeMap::new();
        for index in &generated.confirm_group_indexes {
            provided.insert(*index, groups[*index as usize].clone());
        }
        store.confirm(&generated.key_id, &provided).unwrap();
        store.confirm(&generated.key_id, &provided).unwrap();
        assert!(store.current().is_some());
    }

    #[test]
    fn lost_response_replaces_only_unconfirmed_key() {
        unsafe { std::env::set_var("FD_RECOVERY_ARGON_KIB", "8") };
        let dir = tempfile::tempdir().unwrap();
        let mut store = RecoveryKeyStore::open(dir.path().join("rk.enc"), &[3u8; 32]).unwrap();
        let lost = store.generate(false).unwrap();
        let retry = store.generate(false).unwrap();
        assert_ne!(lost.key_id, retry.key_id);
        let groups = groups_of(&retry.recovery_key);
        let provided = retry
            .confirm_group_indexes
            .iter()
            .map(|i| (*i, groups[*i as usize].clone()))
            .collect();
        assert!(store.confirm(&lost.key_id, &provided).is_err());
        store.confirm(&retry.key_id, &provided).unwrap();
        assert!(store.generate(false).is_err());
    }

    #[test]
    fn wrong_group_is_rejected() {
        unsafe { std::env::set_var("FD_RECOVERY_ARGON_KIB", "8") };
        let dir = tempfile::tempdir().unwrap();
        let dek = [3u8; 32];
        let mut store = RecoveryKeyStore::open(dir.path().join("rk.enc"), &dek).unwrap();
        let generated = store.generate(false).unwrap();
        let mut provided = BTreeMap::new();
        for index in &generated.confirm_group_indexes {
            provided.insert(*index, "XXXXX".into());
        }
        assert!(store.confirm(&generated.key_id, &provided).is_err());
    }
}
