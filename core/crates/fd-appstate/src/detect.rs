//! Unauthenticated discovery reads metadata only; authenticity is established after key entry.
use crate::{Result, copy::index::Index};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Detected {
    pub appliance_id: String,
    pub appliance_name: String,
    pub newest_snapshot_at: Option<String>,
    pub found_on: Vec<String>,
    pub local_source_token: String,
    #[serde(skip)]
    pub root: PathBuf,
}
pub fn scan(roots: &[(PathBuf, String)]) -> Result<Vec<Detected>> {
    let mut found = Vec::new();
    for (root, label) in roots {
        let appliances = root.join("appliance");
        if !appliances.is_dir() {
            continue;
        }
        for dir in std::fs::read_dir(appliances)? {
            let dir = dir?;
            if !dir.file_type()?.is_dir() {
                continue;
            }
            let id = dir.file_name().to_string_lossy().to_string();
            if uuid::Uuid::parse_str(&id).is_err() {
                continue;
            }
            let file = dir.path().join("index.json");
            if std::fs::symlink_metadata(&file)
                .is_ok_and(|m| m.file_type().is_symlink() || m.len() > 2 * 1024 * 1024)
            {
                continue;
            }
            let Ok(raw) = std::fs::read(file) else {
                continue;
            };
            let Ok(index) = serde_json::from_slice::<Index>(&raw) else {
                continue;
            };
            if index.appliance_id != id || index.schema_version != 1 || index.snapshots.len() > 256
            {
                continue;
            }
            found.push(Detected {
                appliance_id: id,
                appliance_name: index.appliance_name,
                newest_snapshot_at: index.snapshots.iter().map(|s| s.taken_at.clone()).max(),
                found_on: vec![label.clone()],
                local_source_token: uuid::Uuid::new_v4().to_string(),
                root: root.clone(),
            });
        }
    }
    Ok(found)
}
/// Discover mounted pools only when their durable marker agrees with their UUID directory.
pub fn pool_roots(parent: &std::path::Path) -> Result<Vec<(PathBuf, String)>> {
    let mut roots = Vec::new();
    if !parent.is_dir() {
        return Ok(roots);
    }
    for dir in std::fs::read_dir(parent)? {
        let dir = dir?;
        if !dir.file_type()?.is_dir() {
            continue;
        }
        let id = dir.file_name().to_string_lossy().to_string();
        if uuid::Uuid::parse_str(&id).is_err() {
            continue;
        }
        let marker = dir.path().join(".filedock-pool.json");
        if std::fs::symlink_metadata(&marker)
            .is_ok_and(|m| m.file_type().is_symlink() || m.len() > 65536)
        {
            continue;
        }
        let Ok(raw) = std::fs::read(marker) else {
            continue;
        };
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(&raw) else {
            continue;
        };
        if value["poolId"].as_str() != Some(&id) {
            continue;
        }
        roots.push((
            dir.path().join(".filedock/appliance-state"),
            format!("pool {}", value["name"].as_str().unwrap_or(&id)),
        ));
    }
    Ok(roots)
}
