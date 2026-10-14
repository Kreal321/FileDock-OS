//! Encrypted secret store (FR-028 to FR-030, research R10).

use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

const DEK_LEN: usize = 32;
const NONCE_LEN: usize = 24;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("secret {0} was not found")]
    NotFound(String),
    #[error("secret {0} is invalid")]
    InvalidRef(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("crypto: {0}")]
    Crypto(String),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SecretInfo {
    pub r#ref: String,
    pub masked_hint: String,
    pub consumers: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize)]
struct Record {
    r#ref: String,
    ciphertext: String,
    nonce: String,
    consumers: Vec<String>,
    masked_hint: String,
}

#[derive(Default, Serialize, Deserialize)]
struct FileBody {
    records: BTreeMap<String, Record>,
}

pub struct SecretStore {
    path: PathBuf,
    dek_path: PathBuf,
    dek: Zeroizing<[u8; DEK_LEN]>,
    body: FileBody,
}

impl SecretStore {
    pub fn open(path: impl AsRef<Path>, dek_path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let path = path.as_ref().to_path_buf();
        let dek_path = dek_path.as_ref().to_path_buf();
        if let Some(parent) = dek_path.parent() {
            fs::create_dir_all(parent)?;
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let dek = load_or_create_dek(&dek_path)?;
        let body = if path.exists() {
            serde_json::from_slice(&fs::read(&path)?)?
        } else {
            FileBody::default()
        };
        Ok(Self {
            path,
            dek_path,
            dek,
            body,
        })
    }

    pub fn encrypted_bytes(&self) -> Result<Vec<u8>, StoreError> {
        Ok(serde_json::to_vec(&self.body)?)
    }
    /// Validate every encrypted record before replacing any live file.
    pub fn read_snapshot(
        bytes: &[u8],
        dek: &[u8; 32],
        reference: &str,
    ) -> Result<Zeroizing<Vec<u8>>, StoreError> {
        let body: FileBody = serde_json::from_slice(bytes)?;
        Self {
            path: PathBuf::new(),
            dek_path: PathBuf::new(),
            dek: Zeroizing::new(*dek),
            body,
        }
        .get(reference)
    }
    pub fn validate_snapshot(bytes: &[u8], dek: &[u8; 32]) -> Result<(), StoreError> {
        let body: FileBody = serde_json::from_slice(bytes)?;
        let store = Self {
            path: PathBuf::new(),
            dek_path: PathBuf::new(),
            dek: Zeroizing::new(*dek),
            body,
        };
        for reference in store.body.records.keys() {
            if !fd_state_ref_ok(reference) {
                return Err(StoreError::InvalidRef(reference.clone()));
            }
            store.get(reference)?;
        }
        Ok(())
    }
    pub fn dek_path(&self) -> &Path {
        &self.dek_path
    }

    pub fn dek_bytes(&self) -> [u8; 32] {
        *self.dek
    }

    pub fn put(
        &mut self,
        reference: &str,
        plaintext: &[u8],
        consumers: Vec<String>,
    ) -> Result<SecretInfo, StoreError> {
        if !fd_state_ref_ok(reference) {
            return Err(StoreError::InvalidRef(reference.into()));
        }
        let key = Key::try_from(self.dek.as_ref()).expect("32-byte DEK");
        let cipher = XChaCha20Poly1305::new(&key);
        let mut nonce_bytes = [0u8; NONCE_LEN];
        fill_random(&mut nonce_bytes);
        let nonce = XNonce::try_from(nonce_bytes.as_slice()).expect("24-byte nonce");
        let ciphertext = cipher
            .encrypt(&nonce, plaintext)
            .map_err(|e| StoreError::Crypto(e.to_string()))?;
        let info = SecretInfo {
            r#ref: reference.to_owned(),
            masked_hint: mask(plaintext),
            consumers: consumers.clone(),
        };
        self.body.records.insert(
            reference.to_owned(),
            Record {
                r#ref: reference.to_owned(),
                ciphertext: hex::encode(ciphertext),
                nonce: hex::encode(nonce_bytes),
                consumers,
                masked_hint: info.masked_hint.clone(),
            },
        );
        self.flush()?;
        Ok(info)
    }

    pub fn get(&self, reference: &str) -> Result<Zeroizing<Vec<u8>>, StoreError> {
        let record = self
            .body
            .records
            .get(reference)
            .ok_or_else(|| StoreError::NotFound(reference.into()))?;
        let key = Key::try_from(self.dek.as_ref()).expect("32-byte DEK");
        let cipher = XChaCha20Poly1305::new(&key);
        let nonce = hex::decode(&record.nonce).map_err(|e| StoreError::Crypto(e.to_string()))?;
        let ciphertext =
            hex::decode(&record.ciphertext).map_err(|e| StoreError::Crypto(e.to_string()))?;
        let nonce =
            XNonce::try_from(nonce.as_slice()).map_err(|_| StoreError::Crypto("nonce".into()))?;
        let plain = cipher
            .decrypt(&nonce, ciphertext.as_ref())
            .map_err(|e| StoreError::Crypto(e.to_string()))?;
        Ok(Zeroizing::new(plain))
    }

    pub fn delete(&mut self, reference: &str) -> Result<(), StoreError> {
        self.body
            .records
            .remove(reference)
            .ok_or_else(|| StoreError::NotFound(reference.into()))?;
        self.flush()
    }

    pub fn list(&self) -> Vec<SecretInfo> {
        self.body
            .records
            .values()
            .map(|r| SecretInfo {
                r#ref: r.r#ref.clone(),
                masked_hint: r.masked_hint.clone(),
                consumers: r.consumers.clone(),
            })
            .collect()
    }

    pub fn consumers_of(&self, subsystem: &str) -> Vec<(String, Zeroizing<Vec<u8>>)> {
        self.body
            .records
            .values()
            .filter(|r| r.consumers.iter().any(|c| c == subsystem))
            .filter_map(|r| {
                self.get(&r.r#ref)
                    .ok()
                    .map(|plain| (r.r#ref.clone(), plain))
            })
            .collect()
    }

    fn flush(&self) -> Result<(), StoreError> {
        let data = serde_json::to_vec_pretty(&self.body)?;
        atomic_write(&self.path, &data, 0o600)
    }
}

fn fd_state_ref_ok(reference: &str) -> bool {
    let bytes = reference.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 255
        && bytes[0].is_ascii_alphanumeric()
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._/-".contains(b))
        && !reference.contains("..")
        && !reference.ends_with('/')
}

fn mask(plaintext: &[u8]) -> String {
    let digest = Sha256::digest(plaintext);
    format!("••••{:02x}{:02x}", digest[0], digest[1])
}

fn load_or_create_dek(path: &Path) -> Result<Zeroizing<[u8; DEK_LEN]>, StoreError> {
    if path.exists() {
        let bytes = fs::read(path)?;
        if bytes.len() != DEK_LEN {
            return Err(StoreError::Crypto("local.key has the wrong length".into()));
        }
        let mut dek = [0u8; DEK_LEN];
        dek.copy_from_slice(&bytes);
        return Ok(Zeroizing::new(dek));
    }
    let mut dek = Zeroizing::new([0u8; DEK_LEN]);
    fill_random(dek.as_mut());
    atomic_write(path, dek.as_ref(), 0o600)?;
    Ok(dek)
}

fn fill_random(buf: &mut [u8]) {
    getrandom::fill(buf).expect("entropy");
}

fn atomic_write(path: &Path, bytes: &[u8], mode: u32) -> Result<(), StoreError> {
    let tmp = path.with_extension("tmp");
    {
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(mode)
            .open(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    fs::rename(tmp, path)?;
    if let Some(parent) = path.parent() {
        fs::File::open(parent)?.sync_all()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn put_is_write_only_and_list_is_masked() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = SecretStore::open(
            dir.path().join("secrets.enc"),
            dir.path().join("keys/local.key"),
        )
        .unwrap();
        let info = store
            .put(
                "connection/1/secret-key",
                b"super-secret",
                vec!["fd-s3".into()],
            )
            .unwrap();
        assert_eq!(info.masked_hint.chars().take(4).collect::<String>(), "••••");
        assert!(!info.masked_hint.contains("super"));
        assert_eq!(store.list()[0].masked_hint, info.masked_hint);
        assert_eq!(
            store.get("connection/1/secret-key").unwrap().as_slice(),
            b"super-secret"
        );
        store.delete("connection/1/secret-key").unwrap();
        assert!(store.get("connection/1/secret-key").is_err());
    }
}
