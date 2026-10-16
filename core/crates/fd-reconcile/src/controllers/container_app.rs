//! Compose owns the running containers; Core is never in their I/O path.
use crate::{Controller, ReconcileError};
use fd_state::{ResourceKind, ResourceRef, StateStore};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
pub struct ContainerAppController {
    pub store: Arc<StateStore>,
    pub events: Arc<fd_observe::EventBus>,
    pub root: PathBuf,
    pub secrets_path: PathBuf,
    pub dek_path: PathBuf,
    pub secrets_root: PathBuf,
    pub enabled: bool,
}
fn failed(e: impl std::fmt::Display) -> ReconcileError {
    ReconcileError::Failed(e.to_string())
}
async fn compose(dir: &Path, name: &str, args: &[&str]) -> Result<String, ReconcileError> {
    let output = tokio::process::Command::new("docker")
        .args(["compose", "--project-directory"])
        .arg(dir)
        .args(["-p", &format!("fd-{name}")])
        .args(args)
        .kill_on_drop(true)
        .output()
        .await
        .map_err(failed)?;
    if !output.status.success() {
        return Err(failed("app.compose_failed"));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into())
}
pub fn warnings(compose: &str) -> Result<Vec<String>, String> {
    let v: Value = serde_saphyr::from_str(compose).map_err(|_| "app.compose_invalid")?;
    let mut warnings = Vec::new();
    let services = v["services"].as_object().ok_or("app.services_required")?;
    for (name, service) in services {
        if service["privileged"] == true
            || service["pid"] == "host"
            || service["network_mode"] == "host"
            || service["devices"].is_array()
        {
            warnings.push(format!("{name} requests host-level access"));
        }
        for vol in service["volumes"].as_array().into_iter().flatten() {
            let source = vol
                .as_str()
                .and_then(|s| s.split(':').next())
                .or_else(|| vol["source"].as_str())
                .unwrap_or("");
            if !source.starts_with("/storage/") {
                warnings.push(format!("{name}: {source} is outside managed folders; its data is not protected by FileDock"));
            }
        }
    }
    Ok(warnings)
}
#[async_trait::async_trait]
impl Controller for ContainerAppController {
    fn kind(&self) -> ResourceKind {
        ResourceKind::ContainerApp
    }
    async fn reconcile(&self, r: &ResourceRef) -> Result<(), ReconcileError> {
        if r.id.len() != 36 || !r.id.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-') {
            return Err(failed("app.id_invalid"));
        }
        let Some(row) = self.store.get_including_deleted(r).map_err(failed)? else {
            return Ok(());
        };
        if !self.enabled {
            return Ok(());
        }
        let name = row.spec["name"]
            .as_str()
            .ok_or_else(|| failed("app.name_missing"))?;
        let dir = self.root.join(&r.id);
        if row.deleted {
            if dir.exists() {
                compose(&dir, name, &["down"]).await?;
                std::fs::remove_dir_all(dir).map_err(failed)?;
                let secrets = self.secrets_root.join(format!("app-{}", r.id));
                if secrets.exists() {
                    std::fs::remove_dir_all(secrets).map_err(failed)?;
                }
            }
            return Ok(());
        }
        let text = row.spec["compose"].as_str().unwrap_or("");
        let warnings = warnings(text).map_err(failed)?;
        // Validate normalized Compose after Docker interpolation, not just raw YAML text.
        std::fs::create_dir_all(&dir).map_err(failed)?;
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).map_err(failed)?;
        let env = row.spec["environment"]
            .as_object()
            .map(|m| {
                m.iter()
                    .map(|(k, v)| format!("{k}={}\n", v.as_str().unwrap_or("").replace('\n', "")))
                    .collect::<String>()
            })
            .unwrap_or_default();
        let mut document: Value = serde_saphyr::from_str(text).map_err(failed)?;
        let refs = row.spec["secrets"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let vault =
            fd_secrets::SecretStore::open(&self.secrets_path, &self.dek_path).map_err(failed)?;
        let files =
            fd_secrets::materialize(&vault, &self.secrets_root, &format!("app-{}", r.id), &refs)
                .map_err(failed)?;
        let mut names = std::collections::BTreeSet::new();
        for (reference, path) in refs.iter().zip(files.files) {
            let name = reference.replace('/', "__");
            if !names.insert(name.clone()) {
                return Err(failed("app.secret_name_conflict"));
            }
            document["secrets"][name] = json!({"file":path});
        }
        fd_storage::atomic(
            &dir.join("compose.yaml"),
            &serde_json::to_vec(&document).map_err(failed)?,
            0o600,
        )
        .map_err(failed)?;
        fd_storage::atomic(&dir.join(".env"), env.as_bytes(), 0o600).map_err(failed)?;
        let normalized: Value =
            serde_json::from_str(&compose(&dir, name, &["config", "--format", "json"]).await?)
                .map_err(failed)?;
        let mut cursor = None;
        let mut folders = Vec::new();
        loop {
            let rows = self
                .store
                .list(ResourceKind::ManagedFolder, cursor.as_deref(), 500)
                .map_err(failed)?;
            if rows.is_empty() {
                break;
            }
            cursor = rows.last().map(|r| r.resource.id.clone());
            folders.extend(rows);
        }
        for service in normalized["services"]
            .as_object()
            .ok_or_else(|| failed("app.services_required"))?
            .values()
        {
            for volume in service["volumes"].as_array().into_iter().flatten() {
                let source = volume["source"].as_str().unwrap_or("");
                if source.starts_with("/storage/") {
                    let Some(folder) = folders.iter().find(|f| f.spec["filedockPath"] == source)
                    else {
                        return Err(failed("app.folder_not_managed"));
                    };
                    if !folder.spec["access"]
                        .as_array()
                        .is_some_and(|a| a.iter().any(|v| v == "Docker"))
                    {
                        return Err(failed("app.docker_access_required"));
                    }
                    if folder.status.as_ref().is_none_or(|s| {
                        s["state"] != "Ready" || s["observedGeneration"] != folder.generation
                    }) {
                        // Keep the control plane available so this dependency can be repaired.
                        self.store.write_status(r,&json!({"state":"Unavailable","detail":"app.folder_unavailable","observedGeneration":row.generation})).map_err(failed)?;
                        return Ok(());
                    }
                }
            }
        }
        let stopped = self
            .store
            .db()
            .with(|c| fd_state::db::get_setting(c, &format!("app.{}.stopped", r.id)))
            .map_err(failed)?
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if stopped {
            compose(&dir, name, &["stop"]).await?;
        } else {
            compose(&dir, name, &["up", "-d", "--remove-orphans"]).await?;
        }
        let status = json!({"state":if stopped{"Stopped"}else{"Running"},"observedGeneration":row.generation,"bindingWarnings":warnings});
        if row.status.as_ref() != Some(&status) {
            self.store.write_status(r, &status).map_err(failed)?;
            self.events
                .emit("app.state.changed", Some(r.clone()), status)
                .map_err(failed)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn external_persistence_is_visible_and_invalid_compose_is_not_accepted() {
        assert!(
            !warnings(
                "services:\n  app:\n    image: sample\n    volumes:\n      - appdata:/data\n"
            )
            .unwrap()
            .is_empty()
        );
        assert!(warnings("services:\n  app:\n    image: sample\n    volumes:\n      - /storage/appdata:/data\n").unwrap().is_empty());
        assert!(warnings("not: [valid").is_err());
    }
}
