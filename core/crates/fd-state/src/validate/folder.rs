//! Managed-folder cross-resource rules run inside the desired-state transaction.
use crate::resources::{PoolKind, ResourceSpec, StorageMode, paths_overlap};
use crate::{Change, ResourceKind, ResourceRef, StateView, ValidationIssue};
use serde_json::{Value, json};
pub fn suggested_path(name: &str) -> String {
    let slug = name
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>();
    format!("/storage/{}", slug.trim_matches('-'))
}
pub fn required(mode: StorageMode) -> Vec<&'static str> {
    match mode {
        StorageMode::LocalOnly => vec!["storage.local"],
        StorageMode::HybridBackup => vec!["backup.snapshot", "backup.restore", "backup.retention"],
        StorageMode::CloudMount => vec!["cloud.mount", "cache.media", "cache.generic"],
        StorageMode::TieredStorage => vec!["tier.storage"],
    }
}
pub fn normalize(mut value: Value) -> Value {
    if !value.is_object() {
        return value;
    }
    if value["filedockPath"].as_str().is_none_or(str::is_empty) {
        value["filedockPath"] = json!(suggested_path(value["name"].as_str().unwrap_or("")));
    }
    if value["advanced"].is_null() {
        value["advanced"] = json!({})
    }
    if value["storageMode"] == "HybridBackup" {
        let importance = value["dataImportance"].as_str().unwrap_or("");
        let defaults = match importance {
            "Critical" => {
                json!({"backupEnabled":true,"backupIntervalSeconds":300,"retention":{"keepLast":30},"subfolderImportance":{}})
            }
            "Important" => {
                json!({"backupEnabled":true,"backupIntervalSeconds":900,"retention":{"keepLast":30},"subfolderImportance":{}})
            }
            "Rebuildable" => {
                json!({"backupEnabled":false,"backupIntervalSeconds":3600,"retention":{"keepLast":30},"subfolderImportance":{}})
            }
            _ => json!({}),
        };
        if let (Some(out), Some(defaults)) =
            (value["advanced"].as_object_mut(), defaults.as_object())
        {
            for (k, v) in defaults {
                out.entry(k.clone()).or_insert_with(|| v.clone());
            }
        }
    } else if value.get("dataImportance").is_none() {
        value["dataImportance"] = Value::Null;
    }
    if value["storageMode"] == "CloudMount"
        && let Some(a) = value["advanced"].as_object_mut()
    {
        a.entry("cache")
            .or_insert(json!({"profile":"Generic","capacityGb":100}));
        a.entry("consistency").or_insert(json!("Balanced"));
    }
    if value["storageMode"] == "TieredStorage"
        && let Some(a) = value["advanced"].as_object_mut()
    {
        a.entry("tiering").or_insert(json!({"coldDays":30,"capacityGb":500,"highWatermarkPercent":85,"lowWatermarkPercent":70}));
    }
    value
}
pub fn validate(view: &StateView<'_>, changes: &[Change]) -> Vec<ValidationIssue> {
    let mut issues = vec![];
    let folders = view.list(ResourceKind::ManagedFolder).collect::<Vec<_>>();
    for row in &folders {
        let Ok(ResourceSpec::ManagedFolder(f)) = row.parsed_spec() else {
            continue;
        };
        if let Some(old) = view.original_folders.get(&row.resource.id) {
            if old.spec["reference"]["poolId"] != row.spec["reference"]["poolId"]
                || old.spec["reference"]["relativePath"] != row.spec["reference"]["relativePath"]
            {
                issues.push(ValidationIssue::error(
                    "folder.reference_immutable",
                    "reference",
                    "The local reference path cannot change after creation",
                ));
            }
            if old.spec["storageMode"] != row.spec["storageMode"]
                && (old.spec["storageMode"] == "CloudMount"
                    || row.spec["storageMode"] == "CloudMount")
            {
                issues.push(ValidationIssue::error(
                    "folder.cloud_mode_change_unsupported",
                    "storageMode",
                    "Create a new folder to change into or out of Cloud Mount",
                ));
            }
            if old.spec["storageMode"] == "TieredStorage"
                && row.spec["storageMode"] != "TieredStorage"
                && old
                    .status
                    .as_ref()
                    .is_none_or(|s| s["modeObservation"]["allDataLocal"] != true)
            {
                issues.push(ValidationIssue::error(
                    "folder.recall_required",
                    "storageMode",
                    "Recall all tiered data locally before leaving Tiered Storage",
                ));
            }
        }
        if view.enforce_capabilities && changes.iter().any(|c| c.resource() == &row.resource) {
            for capability in required(f.storage_mode) {
                if !view.capabilities.contains(capability) {
                    issues.push(ValidationIssue::error(
                        "mode.capability_missing",
                        "storageMode",
                        format!("Missing capability: {capability}"),
                    ));
                }
            }
            for access in &f.access {
                let cap = match access {
                    crate::resources::Access::Smb => "protocol.smb",
                    crate::resources::Access::Nfs => "protocol.nfs",
                    crate::resources::Access::WebDav => "protocol.webdav",
                    crate::resources::Access::Docker => "runtime.container",
                };
                if !view.capabilities.contains(cap) {
                    issues.push(ValidationIssue::error(
                        "mode.capability_missing",
                        "access",
                        format!("Missing capability: {cap}"),
                    ));
                }
            }
        }
        if let Some((pool_id, rel)) = f.local_reference() {
            if let Some(pool) = view.get(&ResourceRef::new(ResourceKind::Pool, pool_id))
                && let Ok(ResourceSpec::Pool(p)) = pool.parsed_spec()
                && p.kind == PoolKind::System
            {
                issues.push(ValidationIssue::warning("system_pool.shared_with_os","reference.poolId","This folder shares the OS disk. Disk failure or an erased reinstall loses local data. Prefer a data pool for large or unprotected data."));
                if view.enforce_capabilities
                    && changes.iter().any(|c| c.resource() == &row.resource)
                    && pool.status.as_ref().is_none_or(|s| {
                        !matches!(s["state"].as_str(), Some("Available" | "Healthy" | "Ready"))
                    })
                {
                    issues.push(ValidationIssue::error(
                        "system_pool.unavailable",
                        "reference.poolId",
                        "The System Pool is unavailable",
                    ));
                }
            }
            let physical = format!("/mnt/filedock/pools/{pool_id}{rel}");
            for other in &folders {
                let otherpath = other.spec["filedockPath"].as_str().unwrap_or("");
                if !otherpath.is_empty() && paths_overlap(&physical, otherpath) {
                    issues.push(
                        ValidationIssue::error(
                            "reference.overlap",
                            "reference.relativePath",
                            "Reference overlaps a FileDock path",
                        )
                        .with_conflicting(other.resource.clone()),
                    );
                }
            }
        }
        if let Some(connection) = &f.reference.connection_id
            && view
                .get(&ResourceRef::new(ResourceKind::CloudConnection, connection))
                .is_none()
        {
            issues.push(ValidationIssue::error(
                "reference.connection_missing",
                "reference.connectionId",
                "Cloud connection does not exist",
            ));
        }
        for other in &folders {
            if other.resource == row.resource {
                continue;
            }
            let Ok(ResourceSpec::ManagedFolder(g)) = other.parsed_spec() else {
                continue;
            };
            if let (Some(a), Some(b), Some(ap), Some(bp)) = (
                &f.reference.connection_id,
                &g.reference.connection_id,
                &f.reference.remote_path,
                &g.reference.remote_path,
            ) && a == b
                && !(f.storage_mode == StorageMode::CloudMount
                    && g.storage_mode == StorageMode::CloudMount)
                && remote_overlap(ap, bp)
            {
                issues.push(
                    ValidationIssue::error(
                        "reference.cloud_overlap",
                        "reference.remotePath",
                        format!("Remote reference overlaps folder {}", g.name),
                    )
                    .with_conflicting(other.resource.clone()),
                );
            }
        }
        if let Some(destination) = &f.reference.backup_destination {
            if let Some(pool) = destination["poolId"].as_str()
                && view
                    .get(&ResourceRef::new(ResourceKind::Pool, pool))
                    .is_none()
            {
                issues.push(ValidationIssue::error(
                    "reference.backup_pool_missing",
                    "reference.backupDestination",
                    "Backup pool does not exist",
                ));
            }
            if let Some(connection) = destination["connectionId"].as_str()
                && view
                    .get(&ResourceRef::new(ResourceKind::CloudConnection, connection))
                    .is_none()
            {
                issues.push(ValidationIssue::error(
                    "reference.backup_connection_missing",
                    "reference.backupDestination",
                    "Backup connection does not exist",
                ));
            }
            if let (Some(pool), Some(path)) = (
                destination["poolId"].as_str(),
                destination["relativePath"].as_str(),
            ) {
                for other in &folders {
                    if other.spec["reference"]["poolId"] == pool
                        && other.spec["reference"]["relativePath"]
                            .as_str()
                            .is_some_and(|r| paths_overlap(r, path))
                    {
                        issues.push(
                            ValidationIssue::error(
                                "reference.backup_overlap",
                                "reference.backupDestination",
                                format!(
                                    "Backup destination overlaps folder {}",
                                    other.spec["name"]
                                ),
                            )
                            .with_conflicting(other.resource.clone()),
                        );
                    }
                }
            }
            if let (Some(connection), Some(path)) = (
                destination["connectionId"].as_str(),
                destination["remotePath"].as_str(),
            ) {
                for other in &folders {
                    if other.spec["reference"]["connectionId"]
                        .as_str()
                        .is_some_and(|id| {
                            cloud_identity(view, id) == cloud_identity(view, connection)
                        })
                        && other.spec["reference"]["remotePath"]
                            .as_str()
                            .is_some_and(|r| remote_overlap(r, path))
                    {
                        issues.push(
                            ValidationIssue::error(
                                "reference.backup_overlap",
                                "reference.backupDestination",
                                "Backup destination overlaps a managed cloud reference",
                            )
                            .with_conflicting(other.resource.clone()),
                        );
                    }
                }
            }
        }
    }
    for app in view.list(ResourceKind::ContainerApp) {
        let compose = app.spec["compose"].as_str().unwrap_or("");
        let sources = compose_sources(compose, &app.spec["environment"]);
        if serde_saphyr::from_str::<Value>(compose).is_err() {
            issues.push(ValidationIssue::error(
                "app.compose_invalid",
                "compose",
                "Compose must be valid YAML",
            ));
        }
        for source in &sources {
            if source.contains('$') {
                issues.push(ValidationIssue::error("app.bind_interpolation_unsupported", "compose", "Use explicit volume sources so folder bindings can be checked before changing access"));
            }
            if source.starts_with("/storage/") {
                match folders.iter().find(|f| f.spec["filedockPath"] == *source) {
                    Some(folder)
                        if folder.spec["access"]
                            .as_array()
                            .is_some_and(|a| a.iter().any(|v| v == "Docker")) => {}
                    _ => issues.push(
                        ValidationIssue::error(
                            "app.docker_access_required",
                            "compose",
                            "Managed bind sources must name folders with Docker access",
                        )
                        .with_conflicting(app.resource.clone()),
                    ),
                }
            } else {
                issues.push(ValidationIssue::warning(
                    "app.data_outside_managed_folders",
                    "compose",
                    format!("{source} is outside FileDock protection"),
                ));
            }
        }
        for old in view.original_folders.values() {
            if old.spec["filedockPath"]
                .as_str()
                .is_some_and(|path| sources.iter().any(|source| paths_overlap(source, path)))
            {
                let now = view.get(&old.resource);
                if now.is_none()
                    || now.is_some_and(|r| r.spec["filedockPath"] != old.spec["filedockPath"])
                    || !now.is_some_and(|r| {
                        r.spec["access"]
                            .as_array()
                            .is_some_and(|a| a.iter().any(|v| v == "Docker"))
                    })
                {
                    issues.push(
                        ValidationIssue::error(
                            "folder.bound_to_app",
                            "access",
                            "A container app still binds this folder",
                        )
                        .with_conflicting(app.resource.clone()),
                    );
                }
            }
        }
    }
    issues
}
fn remote_overlap(a: &str, b: &str) -> bool {
    let a = format!("/{}", a.trim_matches('/'));
    let b = format!("/{}", b.trim_matches('/'));
    a == "/" || b == "/" || paths_overlap(&a, &b)
}

fn cloud_identity(view: &StateView<'_>, id: &str) -> String {
    if let Some(row) = view.get(&ResourceRef::new(ResourceKind::CloudConnection, id))
        && let Ok(ResourceSpec::CloudConnection(c)) = row.parsed_spec()
        && let crate::resources::ConnectionDefinition::S3(s) = c.definition
    {
        return format!("s3:{}:{}", s.endpoint.trim_end_matches('/'), s.bucket);
    }
    id.to_owned()
}

pub fn compose_sources(text: &str, _environment: &Value) -> Vec<String> {
    let Ok(document) = serde_saphyr::from_str::<Value>(text) else {
        return vec![];
    };
    document["services"]
        .as_object()
        .into_iter()
        .flat_map(|s| s.values())
        .flat_map(|s| s["volumes"].as_array().into_iter().flatten())
        .filter_map(|v| {
            let source = if let Some(short) = v.as_str() {
                short.split(':').next().map(str::to_owned)
            } else {
                v["source"].as_str().map(str::to_owned)
            }?;
            Some(source)
        })
        .collect()
}
#[cfg(test)]
mod compose_tests {
    use super::*;
    #[test]
    fn volume_sources_do_not_confuse_sibling_prefixes_or_comments() {
        let sources = compose_sources(
            "# /storage/media\nservices:\n  app:\n    volumes:\n      - /storage/media2:/data\n      - type: bind\n        source: /storage/appdata\n        target: /config\n",
            &json!({}),
        );
        assert_eq!(sources, vec!["/storage/media2", "/storage/appdata"]);
        assert!(!sources.iter().any(|s| paths_overlap(s, "/storage/media")));
    }
}
