use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{check_name, is_uuid};
use crate::ValidationIssue;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StorageMode {
    LocalOnly,
    CloudMount,
    HybridBackup,
    TieredStorage,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DataImportance {
    Critical,
    Important,
    Rebuildable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Access {
    Smb,
    Nfs,
    WebDav,
    Docker,
}

/// Where a folder's data lives. Which fields apply depends on the storage mode (data-model.md,
/// ReferencePath).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReferencePath {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pool_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relative_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backup_destination: Option<Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManagedFolderSpec {
    pub name: String,
    pub filedock_path: String,
    pub storage_mode: StorageMode,
    pub reference: ReferencePath,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_importance: Option<DataImportance>,
    #[serde(default)]
    pub access: BTreeSet<Access>,
    #[serde(default = "empty_object")]
    pub advanced: Value,
}

fn empty_object() -> Value {
    Value::Object(Default::default())
}

/// Normalizes a slash-separated path: one leading `/`, no empty, `.`, or `..` segments.
fn is_clean_absolute(path: &str) -> bool {
    path.starts_with('/')
        && path[1..]
            .split('/')
            .all(|seg| !seg.is_empty() && seg != "." && seg != "..")
        && !path.contains('\0')
}

impl ManagedFolderSpec {
    pub(crate) fn validate(&self, issues: &mut Vec<ValidationIssue>) {
        check_name("name", &self.name, 64, issues);

        if !(self.filedock_path.starts_with("/storage/") && is_clean_absolute(&self.filedock_path))
        {
            issues.push(ValidationIssue::error(
                "folder.path_invalid",
                "filedockPath",
                "must be a clean path under /storage/",
            ));
        }

        let needs_pool = matches!(
            self.storage_mode,
            StorageMode::LocalOnly | StorageMode::HybridBackup | StorageMode::TieredStorage
        );
        let needs_cloud = matches!(
            self.storage_mode,
            StorageMode::CloudMount | StorageMode::TieredStorage
        );
        let r = &self.reference;
        if needs_pool {
            match &r.pool_id {
                Some(id) if is_uuid(id) => {}
                _ => issues.push(ValidationIssue::error(
                    "reference.pool_required",
                    "reference.poolId",
                    "a pool id is required for this storage mode",
                )),
            }
            match r.relative_path.as_deref() {
                Some("/") => issues.push(ValidationIssue::error(
                    "reference.pool_root_reserved",
                    "reference.relativePath",
                    "a folder can't be the pool root",
                )),
                Some(path) if path == "/.filedock" || path.starts_with("/.filedock/") => issues
                    .push(ValidationIssue::error(
                        "reference.pool_root_reserved",
                        "reference.relativePath",
                        "/.filedock is reserved for FileDock",
                    )),
                Some("/.filedock-pool.json") => issues.push(ValidationIssue::error(
                    "reference.pool_root_reserved",
                    "reference.relativePath",
                    "the pool marker is reserved for FileDock",
                )),
                Some(path) if is_clean_absolute(path) => {}
                _ => issues.push(ValidationIssue::error(
                    "reference.path_invalid",
                    "reference.relativePath",
                    "must be a clean directory path below the pool root",
                )),
            }
        } else if r.pool_id.is_some() || r.relative_path.is_some() {
            issues.push(ValidationIssue::error(
                "reference.local_not_allowed",
                "reference",
                "this storage mode has no local reference path",
            ));
        }
        if needs_cloud {
            if r.connection_id.as_deref().is_none_or(|id| !is_uuid(id)) {
                issues.push(ValidationIssue::error(
                    "reference.connection_required",
                    "reference.connectionId",
                    "a connection id is required for this storage mode",
                ));
            }
            if r.remote_path.as_deref().is_none_or(str::is_empty) {
                issues.push(ValidationIssue::error(
                    "reference.remote_path_required",
                    "reference.remotePath",
                    "a remote path is required for this storage mode",
                ));
            }
        } else if r.connection_id.is_some() || r.remote_path.is_some() {
            issues.push(ValidationIssue::error(
                "reference.cloud_not_allowed",
                "reference",
                "this storage mode has no cloud reference",
            ));
        }
        if (self.storage_mode == StorageMode::HybridBackup) != r.backup_destination.is_some() {
            issues.push(ValidationIssue::error(
                "reference.backup_destination",
                "reference.backupDestination",
                "a backup destination is required for Hybrid Backup, and only for it",
            ));
        }

        match (self.storage_mode, self.data_importance) {
            (StorageMode::HybridBackup, None) => issues.push(ValidationIssue::error(
                "folder.importance_required",
                "dataImportance",
                "Hybrid Backup folders need a data importance",
            )),
            (StorageMode::HybridBackup, Some(_)) | (_, None) => {}
            (_, Some(_)) => issues.push(ValidationIssue::error(
                "folder.importance_not_allowed",
                "dataImportance",
                "data importance applies only to Hybrid Backup",
            )),
        }

        if self.storage_mode == StorageMode::LocalOnly
            && self.advanced.as_object().is_none_or(|map| !map.is_empty())
        {
            issues.push(ValidationIssue::error(
                "folder.advanced_not_allowed",
                "advanced",
                "Local Only folders have no advanced settings",
            ));
        }
    }

    /// The local `(pool id, relative path)`, for modes that keep data in a pool.
    pub fn local_reference(&self) -> Option<(&str, &str)> {
        Some((
            self.reference.pool_id.as_deref()?,
            self.reference.relative_path.as_deref()?,
        ))
    }
}

/// True when two clean absolute paths are equal or one contains the other.
pub fn paths_overlap(a: &str, b: &str) -> bool {
    fn within(inner: &str, outer: &str) -> bool {
        inner == outer
            || (inner.starts_with(outer) && inner.as_bytes().get(outer.len()) == Some(&b'/'))
    }
    within(a, b) || within(b, a)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local(path: &str) -> ManagedFolderSpec {
        ManagedFolderSpec {
            name: "Movies".into(),
            filedock_path: "/storage/movies".into(),
            storage_mode: StorageMode::LocalOnly,
            reference: ReferencePath {
                pool_id: Some("0192d3c4-0000-7000-8000-000000000001".into()),
                relative_path: Some(path.into()),
                ..Default::default()
            },
            data_importance: None,
            access: BTreeSet::new(),
            advanced: empty_object(),
        }
    }

    fn codes(spec: &ManagedFolderSpec) -> Vec<String> {
        let mut issues = Vec::new();
        spec.validate(&mut issues);
        issues.into_iter().map(|i| i.code).collect()
    }

    #[test]
    fn reserved_and_bad_paths() {
        assert!(codes(&local("/movies")).is_empty());
        assert_eq!(codes(&local("/")), ["reference.pool_root_reserved"]);
        assert_eq!(
            codes(&local("/.filedock/x")),
            ["reference.pool_root_reserved"]
        );
        assert_eq!(codes(&local("/a/../b")), ["reference.path_invalid"]);
        assert_eq!(codes(&local("movies")), ["reference.path_invalid"]);
    }

    #[test]
    fn importance_only_for_hybrid_backup() {
        let mut spec = local("/movies");
        spec.data_importance = Some(DataImportance::Critical);
        assert_eq!(codes(&spec), ["folder.importance_not_allowed"]);
    }

    #[test]
    fn overlap() {
        assert!(paths_overlap("/a", "/a"));
        assert!(paths_overlap("/a", "/a/b"));
        assert!(paths_overlap("/a/b", "/a"));
        assert!(!paths_overlap("/a", "/ab"));
        assert!(!paths_overlap("/a/b", "/a/c"));
    }
}
