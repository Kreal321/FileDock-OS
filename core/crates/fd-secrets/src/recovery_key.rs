//! Recovery key generation, wrapping, and download acknowledgement (FR-032 to FR-034, R11).

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

const SNAPSHOT_KEY_LEN: usize = 32;
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 24;

#[derive(Debug, thiserror::Error)]
pub enum RecoveryError {
    #[error("{0}")]
    Failed(String),
    #[error("recovery key {0} was not found")]
    NotFound(String),
    #[error("recovery key acknowledgement does not match")]
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
    #[serde(default)]
    pub previous_wrapped_snapshot_keys: BTreeMap<String, String>,
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
}

#[derive(Clone, Serialize, Deserialize)]
struct FileBody {
    snapshot_key: String,
    keys: Vec<RecoveryKeyInfo>,
    pending_key_id: Option<String>,
    #[serde(default)]
    archived_snapshot_keys: BTreeMap<String, String>,
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
                pending_key_id: None,
                archived_snapshot_keys: BTreeMap::new(),
            }
        };
        Ok(Self {
            path,
            dek: *dek,
            body,
        })
    }

    pub fn generate(&mut self, regenerate: bool) -> Result<GeneratedKey, RecoveryError> {
        let mut candidate = Self {
            path: self.path.clone(),
            dek: self.dek,
            body: self.body.clone(),
        };
        let generated = candidate.generate_candidate(regenerate)?;
        std::mem::swap(&mut self.body, &mut candidate.body);
        Ok(generated)
    }
    fn generate_candidate(&mut self, regenerate: bool) -> Result<GeneratedKey, RecoveryError> {
        if !regenerate
            && self.body.pending_key_id.is_none()
            && self.body.keys.iter().any(|k| k.superseded_at.is_none())
        {
            return Err(RecoveryError::Failed(
                "a recovery key already exists; pass regenerate to replace it".into(),
            ));
        }
        if !regenerate && self.body.pending_key_id.is_some() {
            self.body.keys.retain(|k| k.superseded_at.is_some());
            self.body.pending_key_id = None;
        }
        if regenerate && self.body.archived_snapshot_keys.len() >= 256 {
            return Err(RecoveryError::Failed(
                "recovery.key_history_limit; current key remains unchanged".into(),
            ));
        }
        if regenerate {
            if let Some(current) = self.current().cloned() {
                let master: [u8; 32] = hex::decode(&self.body.snapshot_key)
                    .map_err(|_| RecoveryError::Crypto("snapshot key".into()))?
                    .try_into()
                    .map_err(|_| RecoveryError::Crypto("snapshot key".into()))?;
                for (id, key) in Self::unlocked_keys(&current, master)? {
                    self.body
                        .archived_snapshot_keys
                        .insert(id, hex::encode(key));
                }
            }
            self.body.snapshot_key = hex::encode(random_bytes::<SNAPSHOT_KEY_LEN>());
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
            previous_wrapped_snapshot_keys: self.wrap_previous()?,
        };
        self.body.keys.push(info);
        self.body.pending_key_id = Some(key_id.clone());
        self.persist()?;
        Ok(GeneratedKey {
            key_id,
            recovery_key: formatted,
        })
    }

    pub fn confirm(&mut self, key_id: &str) -> Result<(), RecoveryError> {
        let mut candidate = Self {
            path: self.path.clone(),
            dek: self.dek,
            body: self.body.clone(),
        };
        candidate.confirm_candidate(key_id)?;
        std::mem::swap(&mut self.body, &mut candidate.body);
        Ok(())
    }
    fn confirm_candidate(&mut self, key_id: &str) -> Result<(), RecoveryError> {
        if self.body.pending_key_id.is_none() && self.current().is_some_and(|k| k.key_id == key_id)
        {
            return Ok(());
        }
        if self.body.pending_key_id.as_deref() != Some(key_id) {
            return Err(RecoveryError::ConfirmMismatch);
        }
        self.body
            .keys
            .iter()
            .find(|k| k.key_id == key_id)
            .ok_or_else(|| RecoveryError::NotFound(key_id.into()))?;
        self.body.pending_key_id = None;
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

    pub fn snapshot_key(&self) -> Result<[u8; 32], RecoveryError> {
        if self.body.pending_key_id.is_some() || self.current().is_none() {
            return Err(RecoveryError::Failed("recovery.key_unconfirmed".into()));
        }
        hex::decode(&self.body.snapshot_key)
            .map_err(|_| RecoveryError::Crypto("snapshot key".into()))?
            .try_into()
            .map_err(|_| RecoveryError::Crypto("snapshot key".into()))
    }
    pub fn unlock(info: &RecoveryKeyInfo, key: &str) -> Result<[u8; 32], RecoveryError> {
        if info.kdf_params.memory_kib > 256 * 1024
            || info.kdf_params.iterations > 3
            || info.kdf_params.parallelism > 1
        {
            return Err(RecoveryError::Crypto(
                "recovery metadata exceeds limits".into(),
            ));
        }
        if info.salt.len() != 32
            || info.nonce.len() != 48
            || info.wrapped_snapshot_key.len() != 96
            || info.key_id.len() != 16
            || !info.key_id.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(RecoveryError::Crypto("recovery metadata".into()));
        }
        let compact: String = key
            .trim()
            .chars()
            .filter(|c| *c != '-')
            .flat_map(char::to_uppercase)
            .collect();
        if compact.len() != KEY_LEN || !compact.bytes().all(|b| ALPHABET.contains(&b)) {
            return Err(RecoveryError::Crypto("recovery.key_invalid".into()));
        }
        let raw: [u8; KEY_LEN] = compact.as_bytes().try_into().unwrap();
        let formatted = format_key(&raw);
        let salt = hex::decode(&info.salt)
            .map_err(|_| RecoveryError::Crypto("recovery metadata".into()))?;
        let mut wrap = derive_wrap(&formatted, &salt, &info.kdf_params)?;
        let cipher = XChaCha20Poly1305::new(&Key::from(wrap));
        wrap.zeroize();
        let nonce = hex::decode(&info.nonce)
            .map_err(|_| RecoveryError::Crypto("recovery metadata".into()))?;
        let encrypted = hex::decode(&info.wrapped_snapshot_key)
            .map_err(|_| RecoveryError::Crypto("recovery metadata".into()))?;
        let decoded = cipher
            .decrypt(
                &XNonce::try_from(nonce.as_slice())
                    .map_err(|_| RecoveryError::Crypto("recovery metadata".into()))?,
                encrypted.as_slice(),
            )
            .map_err(|_| RecoveryError::Crypto("recovery.key_invalid".into()))?;
        decoded
            .try_into()
            .map_err(|_| RecoveryError::Crypto("snapshot key".into()))
    }
    pub fn install(
        path: impl AsRef<Path>,
        dek: &[u8; 32],
        info: RecoveryKeyInfo,
        snapshot_key: [u8; 32],
    ) -> Result<Self, RecoveryError> {
        let store = Self {
            path: path.as_ref().into(),
            dek: *dek,
            body: FileBody {
                snapshot_key: hex::encode(snapshot_key),
                keys: vec![info],
                pending_key_id: None,
                archived_snapshot_keys: BTreeMap::new(),
            },
        };
        store.persist()?;
        Ok(store)
    }
    fn wrap_previous(&self) -> Result<BTreeMap<String, String>, RecoveryError> {
        let master: [u8; 32] = hex::decode(&self.body.snapshot_key)
            .map_err(|_| RecoveryError::Crypto("snapshot key".into()))?
            .try_into()
            .map_err(|_| RecoveryError::Crypto("snapshot key".into()))?;
        let cipher = XChaCha20Poly1305::new(&Key::from(master));
        let mut output = BTreeMap::new();
        for (id, old) in &self.body.archived_snapshot_keys {
            let nonce = random_bytes::<24>();
            let plain =
                hex::decode(old).map_err(|_| RecoveryError::Crypto("snapshot key".into()))?;
            let mut data = nonce.to_vec();
            data.extend(
                cipher
                    .encrypt(&XNonce::from(nonce), plain.as_slice())
                    .map_err(|_| RecoveryError::Crypto("snapshot key".into()))?,
            );
            output.insert(id.clone(), hex::encode(data));
        }
        Ok(output)
    }
    pub fn unlocked_keys(
        info: &RecoveryKeyInfo,
        master: [u8; 32],
    ) -> Result<BTreeMap<String, [u8; 32]>, RecoveryError> {
        let mut output = BTreeMap::from([(info.key_id.clone(), master)]);
        let cipher = XChaCha20Poly1305::new(&Key::from(master));
        if info.previous_wrapped_snapshot_keys.len() > 256 {
            return Err(RecoveryError::Crypto(
                "recovery metadata exceeds limits".into(),
            ));
        }
        for (id, wrapped) in &info.previous_wrapped_snapshot_keys {
            let data =
                hex::decode(wrapped).map_err(|_| RecoveryError::Crypto("snapshot key".into()))?;
            if data.len() != 72 {
                return Err(RecoveryError::Crypto("snapshot key".into()));
            }
            let plain = cipher
                .decrypt(
                    &XNonce::try_from(&data[..24])
                        .map_err(|_| RecoveryError::Crypto("snapshot key".into()))?,
                    &data[24..],
                )
                .map_err(|_| RecoveryError::Crypto("snapshot key".into()))?;
            output.insert(
                id.clone(),
                plain
                    .try_into()
                    .map_err(|_| RecoveryError::Crypto("snapshot key".into()))?,
            );
        }
        Ok(output)
    }
    pub fn snapshot_keys(&self) -> Result<BTreeMap<String, [u8; 32]>, RecoveryError> {
        Self::unlocked_keys(
            self.current()
                .ok_or_else(|| RecoveryError::Failed("recovery.key_unconfirmed".into()))?,
            self.snapshot_key()?,
        )
    }
    fn persist(&self) -> Result<(), RecoveryError> {
        let plaintext = zeroize::Zeroizing::new(serde_json::to_vec(&self.body)?);
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
        if let Some(parent) = self.path.parent() {
            fs::File::open(parent)?.sync_all()?;
        }
        Ok(())
    }
}

fn decrypt_body(bytes: &[u8], dek: &[u8; SNAPSHOT_KEY_LEN]) -> Result<FileBody, RecoveryError> {
    if bytes.len() < NONCE_LEN {
        return Err(RecoveryError::Crypto("truncated recovery store".into()));
    }
    let (nonce, rest) = bytes.split_at(NONCE_LEN);
    let cipher = XChaCha20Poly1305::new(&Key::from(*dek));
    let plaintext = zeroize::Zeroizing::new(
        cipher
            .decrypt(
                &XNonce::try_from(nonce).map_err(|e| RecoveryError::Crypto(e.to_string()))?,
                rest,
            )
            .map_err(|e| RecoveryError::Crypto(e.to_string()))?,
    );
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

fn random_bytes<const N: usize>() -> [u8; N] {
    let mut buf = [0u8; N];
    getrandom::fill(&mut buf).expect("entropy");
    buf
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
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
        store.confirm(&generated.key_id).unwrap();
        store.confirm(&generated.key_id).unwrap();
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
        assert!(store.confirm(&lost.key_id).is_err());
        store.confirm(&retry.key_id).unwrap();
        assert!(store.generate(false).is_err());
    }

    #[test]
    fn wrong_key_id_is_rejected() {
        unsafe { std::env::set_var("FD_RECOVERY_ARGON_KIB", "8") };
        let dir = tempfile::tempdir().unwrap();
        let dek = [3u8; 32];
        let mut store = RecoveryKeyStore::open(dir.path().join("rk.enc"), &dek).unwrap();
        let generated = store.generate(false).unwrap();
        assert!(store.confirm("unknown-key").is_err());
        assert!(store.confirm(&generated.key_id).is_ok());
    }
}

#[cfg(test)]
mod rotation_tests {
    use super::*;
    #[test]
    fn regeneration_rotates_snapshot_keys_and_requires_confirmation() {
        unsafe { std::env::set_var("FD_RECOVERY_ARGON_KIB", "8") };
        let dir = tempfile::tempdir().unwrap();
        let mut store = RecoveryKeyStore::open(dir.path().join("keys"), &[1; 32]).unwrap();
        let first = store.generate(false).unwrap();
        store.confirm(&first.key_id).unwrap();
        let old = store.snapshot_key().unwrap();
        let next = store.generate(true).unwrap();
        assert!(store.snapshot_key().is_err());
        store.confirm(&next.key_id).unwrap();
        let new = store.snapshot_key().unwrap();
        assert_ne!(old, new);
        let info = store.current().unwrap();
        assert!(RecoveryKeyStore::unlock(info, &first.recovery_key).is_err());
        assert_eq!(
            RecoveryKeyStore::unlock(info, &next.recovery_key).unwrap(),
            new
        );
    }
}

impl Drop for RecoveryKeyStore {
    fn drop(&mut self) {
        self.dek.zeroize();
        self.body.snapshot_key.zeroize();
        for key in self.body.archived_snapshot_keys.values_mut() {
            key.zeroize();
        }
    }
}
