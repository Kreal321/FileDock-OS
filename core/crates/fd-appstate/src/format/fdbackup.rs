//! One-snapshot encrypted copy root. Never extracts untrusted paths to the host.
use crate::{Result, invalid, safe_relative};
use std::{
    collections::BTreeMap,
    io::{Cursor, Read},
};
pub const MAX_BACKUP_BYTES: usize = 128 * 1024 * 1024;
pub fn encode(files: &BTreeMap<String, Vec<u8>>) -> Result<Vec<u8>> {
    let mut tar = tar::Builder::new(Vec::new());
    let mut total = 0;
    for (name, bytes) in files {
        safe_relative(name)?;
        total += bytes.len();
        if total > MAX_BACKUP_BYTES {
            return Err(invalid("backup_file.too_large"));
        }
        let mut h = tar::Header::new_gnu();
        h.set_mode(0o600);
        h.set_size(bytes.len() as u64);
        h.set_cksum();
        tar.append_data(&mut h, name, bytes.as_slice())?;
    }
    Ok(tar.into_inner()?)
}
pub fn decode(bytes: &[u8]) -> Result<BTreeMap<String, Vec<u8>>> {
    if bytes.len() > MAX_BACKUP_BYTES {
        return Err(invalid("backup_file.too_large"));
    }
    let mut files = BTreeMap::new();
    let mut archive = tar::Archive::new(Cursor::new(bytes));
    let mut total = 0usize;
    for e in archive.entries()? {
        let e = e?;
        let path = e.path()?.to_string_lossy().into_owned();
        safe_relative(&path)?;
        if !e.header().entry_type().is_file()
            || !path.starts_with("appliance/")
            || files.contains_key(&path)
            || files.len() > 256
        {
            return Err(invalid("backup_file.invalid"));
        }
        let mut data = Vec::new();
        e.take((MAX_BACKUP_BYTES + 1) as u64)
            .read_to_end(&mut data)?;
        total = total
            .checked_add(data.len())
            .ok_or_else(|| invalid("backup_file.too_large"))?;
        if total > MAX_BACKUP_BYTES {
            return Err(invalid("backup_file.too_large"));
        }
        files.insert(path, data);
    }
    if files.is_empty() {
        return Err(invalid("backup_file.invalid"));
    }
    Ok(files)
}
