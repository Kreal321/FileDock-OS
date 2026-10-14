//! Encrypted local package cache. Copy publication/retention is handled by Engine.
use crate::{Result, atomic_write, format::fdstate, invalid};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
pub fn path(root: &Path, hash: &str) -> Result<PathBuf> {
    if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(invalid("extension.package_hash_invalid"));
    }
    Ok(root.join("packages").join(format!("{hash}.fdpkg")))
}
pub fn store(root: &Path, bytes: &[u8], dek: &[u8; 32]) -> Result<String> {
    let hash = hex::encode(Sha256::digest(bytes));
    let file = path(root, &hash)?;
    if let Some(existing) = load(root, &hash, dek)?
        && existing == bytes
    {
        return Ok(hash);
    }
    let encrypted = fdstate::encrypt_bytes(bytes, dek, b"FDPKG\0\0\0")?;
    atomic_write(&file, &encrypted)?;
    Ok(hash)
}
pub fn load(root: &Path, hash: &str, dek: &[u8; 32]) -> Result<Option<Vec<u8>>> {
    let path = path(root, hash)?;
    let raw = match std::fs::read(path) {
        Ok(v) => v,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let bytes = fdstate::decrypt_bytes(&raw, dek, b"FDPKG\0\0\0")?;
    if hex::encode(Sha256::digest(&bytes)) != hash {
        return Err(invalid("extension.package_integrity_failed"));
    }
    Ok(Some(bytes))
}
pub fn require_recorded(
    bytes: &[u8],
    hash: &str,
    version: &str,
    actual_version: &str,
) -> Result<()> {
    if actual_version != version || hex::encode(Sha256::digest(bytes)) != hash {
        return Err(invalid("extension.recorded_package_required"));
    }
    Ok(())
}
