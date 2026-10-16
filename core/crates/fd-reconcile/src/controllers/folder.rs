//! Mount units belong to systemd, so existing folder I/O survives Core/Management outages.
use crate::{Controller, ReconcileError};
use fd_state::{
    ResourceKind, ResourceRef, StateStore,
    resources::{ManagedFolderSpec, ResourceSpec, StorageMode},
};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
fn failed(e: impl std::fmt::Display) -> ReconcileError {
    ReconcileError::Failed(e.to_string())
}
pub struct FolderController {
    pub store: Arc<StateStore>,
    pub events: Arc<fd_observe::EventBus>,
    pub pools_root: PathBuf,
    pub units: PathBuf,
    pub metadata: PathBuf,
    pub enabled: bool,
    pub gate: tokio::sync::Mutex<()>,
}
pub fn protection(
    folder: &ManagedFolderSpec,
    observed: &Value,
    now: chrono::DateTime<chrono::Utc>,
) -> (&'static str, String) {
    match folder.storage_mode {
        StorageMode::LocalOnly => (
            "Unprotected",
            "All data is kept only on this appliance and may be lost if its pool fails.".into(),
        ),
        StorageMode::TieredStorage => (
            "Unprotected",
            "The local-only hot tier may be lost until verified cloud copies exist.".into(),
        ),
        StorageMode::CloudMount => {
            if observed["uploadStatusKnown"] == true && observed["overdueUploads"] == 0 {
                ("Protected","The cloud is authoritative; pending local writes after the durable cloud point may be lost.".into())
            } else {
                ("AtRisk","Upload status is unavailable or uploads are overdue. Unsynced local changes may be lost.".into())
            }
        }
        StorageMode::HybridBackup => {
            if folder.data_importance == Some(fd_state::resources::DataImportance::Rebuildable)
                && folder.advanced["backupEnabled"] != true
            {
                return ("NotRequired","Rebuildable data has no backup by choice and must be reconnected or rebuilt after loss.".into());
            }
            let target = folder.advanced["backupIntervalSeconds"]
                .as_i64()
                .unwrap_or(900);
            let last = observed["lastDurableRecoveryPoint"]
                .as_str()
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok());
            if last.is_some_and(|t| {
                let age = now.signed_duration_since(t).num_seconds();
                age >= 0 && age <= target
            }) && observed["backupFailed"] != true
            {
                ("Protected","Backup is within target. Changes after the latest durable recovery point may still be lost.".into())
            } else {
                ("AtRisk","A recent durable backup is unavailable. Changes after the last successful recovery point may be lost.".into())
            }
        }
    }
}
pub fn mount_name(path: &str) -> String {
    let mut name = String::new();
    for (i, b) in path.trim_matches('/').bytes().enumerate() {
        if b == b'/' {
            name.push('-')
        } else if b.is_ascii_alphanumeric() || b == b'_' || b == b':' || (b == b'.' && i > 0) {
            name.push(b as char)
        } else {
            name.push_str(&format!("\\x{b:02x}"))
        }
    }
    format!("{name}.mount")
}
fn quote(path: &Path) -> String {
    // Mount What/Where are scalar paths, not shell commands. Outer quotes become literal bytes.
    path.to_string_lossy().replace('%', "%%")
}
pub fn unit(id: &str, source: &Path, target: &Path, pool: &Path) -> String {
    let parent = mount_name(&pool.to_string_lossy());
    format!(
        "# FileDock managed folder {id}\n[Unit]\nDescription=FileDock managed folder {id}\nBindsTo={parent}\nAfter={parent}\nRequiresMountsFor={}\n[Mount]\nWhat={}\nWhere={}\nType=none\nOptions=bind\n[Install]\nWantedBy=multi-user.target\n",
        quote(pool),
        quote(source),
        quote(target)
    )
}
async fn command(program: &str, args: &[&str]) -> Result<bool, ReconcileError> {
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(20),
        tokio::process::Command::new(program)
            .args(args)
            .kill_on_drop(true)
            .output(),
    )
    .await
    .map_err(|_| failed("folder.runtime_timeout"))?
    .map_err(failed)?;
    Ok(result.status.success())
}
fn atomic(path: &Path, bytes: &[u8]) -> Result<(), ReconcileError> {
    use std::io::Write;
    std::fs::create_dir_all(path.parent().unwrap()).map_err(failed)?;
    let tmp = path.with_extension("tmp");
    let mut f = std::fs::File::create(&tmp).map_err(failed)?;
    f.write_all(bytes)
        .and_then(|_| f.sync_all())
        .map_err(failed)?;
    std::fs::rename(tmp, path).map_err(failed)?;
    std::fs::File::open(path.parent().unwrap())
        .and_then(|f| f.sync_all())
        .map_err(failed)
}
impl FolderController {
    async fn status(
        &self,
        row: &fd_state::StoredResource,
        state: &str,
        detail: &str,
        folder: &ManagedFolderSpec,
    ) -> Result<(), ReconcileError> {
        let observed = row
            .status
            .as_ref()
            .and_then(|s| s.get("modeObservation"))
            .cloned()
            .unwrap_or(json!({}));
        let (p, exposure) = protection(folder, &observed, chrono::Utc::now());
        let status = json!({"observedGeneration":row.generation,"state":state,"detail":detail,"protectionState":p,"exposure":exposure,"modeObservation":observed});
        if row.status.as_ref() != Some(&status) {
            self.store
                .write_status(&row.resource, &status)
                .map_err(failed)?;
            self.events
                .emit("folder.status", Some(row.resource.clone()), status)
                .map_err(failed)?;
        }
        Ok(())
    }
    async fn cleanup(&self, names: &[String]) -> Result<(), ReconcileError> {
        if names.is_empty() {
            return Ok(());
        }
        for name in names {
            let path = self.units.join(name);
            if !name.ends_with(".mount")
                || name.contains('/')
                || !std::fs::read_to_string(&path)
                    .unwrap_or_default()
                    .starts_with("# FileDock managed folder ")
            {
                continue;
            }
            if !command("systemctl", &["stop", name]).await? {
                return Err(failed("folder.unmount_failed"));
            }
            std::fs::remove_file(path).map_err(failed)?;
        }
        command("systemctl", &["daemon-reload"]).await?;
        Ok(())
    }
}
#[async_trait::async_trait]
impl Controller for FolderController {
    fn kind(&self) -> ResourceKind {
        ResourceKind::ManagedFolder
    }
    async fn reconcile(&self, r: &ResourceRef) -> Result<(), ReconcileError> {
        let _guard = self.gate.lock().await;
        if r.id.len() != 36 || !r.id.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-') {
            return Err(failed("folder.id_invalid"));
        }
        let Some(row) = self.store.get_including_deleted(r).map_err(failed)? else {
            return Ok(());
        };
        let ResourceSpec::ManagedFolder(folder) =
            row.parsed_spec().map_err(|i| failed(i.message))?
        else {
            return Ok(());
        };
        let record = self.metadata.join(format!("{}.json", r.id));
        let mut known = std::fs::read(&record)
            .ok()
            .and_then(|b| serde_json::from_slice::<Vec<String>>(&b).ok())
            .unwrap_or_default();
        if !self.enabled {
            if !row.deleted {
                self.status(
                    &row,
                    "Pending",
                    "Mount application requires the Linux runtime",
                    &folder,
                )
                .await?
            }
            return Ok(());
        }
        if row.deleted {
            self.cleanup(&known).await?;
            if record.exists() {
                std::fs::remove_file(record).map_err(failed)?;
            }
            return Ok(());
        }
        let target = Path::new(&folder.filedock_path);
        let name = mount_name(&folder.filedock_path);
        let old = known
            .iter()
            .filter(|n| **n != name)
            .cloned()
            .collect::<Vec<_>>();
        self.cleanup(&old).await?;
        if folder.storage_mode != StorageMode::LocalOnly {
            self.status(
                &row,
                "NotReady",
                "The storage-mode provider must supply its mount and protection observations",
                &folder,
            )
            .await?;
            return Ok(());
        }
        let Some((pool_id, relative)) = folder.local_reference() else {
            return Err(failed("folder.reference_missing"));
        };
        let pool = self.pools_root.join(pool_id);
        let poolrow = self
            .store
            .get(&ResourceRef::new(ResourceKind::Pool, pool_id))
            .map_err(failed)?;
        if poolrow.as_ref().is_none_or(|p| {
            p.status
                .as_ref()
                .is_some_and(|s| matches!(s["state"].as_str(), Some("Failed" | "Unavailable")))
        }) || !pool.join(".filedock-pool.json").is_file()
            || !command(
                "findmnt",
                &[
                    "--mountpoint",
                    pool.to_str()
                        .ok_or_else(|| failed("folder.pool_path_invalid"))?,
                    "--noheadings",
                ],
            )
            .await?
        {
            let _ = command("systemctl", &["stop", &name]).await;
            self.status(
                &row,
                "Unavailable",
                "Pool is missing, failed, or not mounted. No fallback directory is used.",
                &folder,
            )
            .await?;
            return Ok(());
        }
        if relative
            .chars()
            .any(|c| matches!(c, '\n' | '\r' | '\\' | '"' | '%'))
        {
            return Err(failed("reference.path_invalid"));
        }
        let source = pool.join(relative.trim_start_matches('/'));
        // Refuse symlink components before creating directories; never follow a reference out of a pool.
        let mut walk = pool.clone();
        for segment in relative.trim_start_matches('/').split('/') {
            walk.push(segment);
            if std::fs::symlink_metadata(&walk).is_ok_and(|m| m.file_type().is_symlink()) {
                return Err(failed("reference.symlink_forbidden"));
            }
            if !walk.exists() {
                std::fs::create_dir(&walk).map_err(failed)?;
            }
        }
        let actual = std::fs::canonicalize(&source).map_err(failed)?;
        if !actual.starts_with(std::fs::canonicalize(&pool).map_err(failed)?) {
            return Err(failed("reference.outside_pool"));
        }
        let mut target_component = PathBuf::from("/storage");
        for component in target
            .strip_prefix("/storage")
            .map_err(failed)?
            .components()
        {
            target_component.push(component);
            if std::fs::symlink_metadata(&target_component)
                .is_ok_and(|m| m.file_type().is_symlink())
            {
                return Err(failed("folder.target_symlink_forbidden"));
            }
        }
        let existing_unit = self.units.join(&name);
        if existing_unit.exists()
            && !std::fs::read_to_string(&existing_unit)
                .unwrap_or_default()
                .starts_with(&format!("# FileDock managed folder {}\n", r.id))
        {
            return Err(failed("folder.unit_already_owned"));
        }
        let mounted = command(
            "findmnt",
            &[
                "--mountpoint",
                target
                    .to_str()
                    .ok_or_else(|| failed("folder.target_invalid"))?,
                "--noheadings",
            ],
        )
        .await?;
        if mounted && !known.contains(&name) {
            return Err(failed("folder.target_already_mounted"));
        }
        if !mounted
            && target.is_dir()
            && std::fs::read_dir(target).map_err(failed)?.next().is_some()
        {
            return Err(failed("folder.target_not_empty"));
        }
        std::fs::create_dir_all(target).map_err(failed)?;
        if !known.contains(&name) {
            known.push(name.clone())
        }
        atomic(&record, &serde_json::to_vec(&known).map_err(failed)?)?;
        let body = unit(&r.id, &source, target, &pool);
        let unit_path = self.units.join(&name);
        if std::fs::read_to_string(&unit_path).ok().as_deref() != Some(&body) {
            atomic(&unit_path, body.as_bytes())?;
            if !command("systemctl", &["daemon-reload"]).await? {
                return Err(failed("folder.unit_reload_failed"));
            }
        }
        if !command("systemctl", &["start", &name]).await? {
            self.status(
                &row,
                "Unavailable",
                "The folder bind mount could not be started",
                &folder,
            )
            .await?;
            return Err(failed("folder.mount_failed"));
        }
        // A successful systemd start is insufficient: reject an old or misdirected bind mount.
        let source_meta = std::fs::metadata(&source).map_err(failed)?;
        let target_meta = std::fs::metadata(target).map_err(failed)?;
        use std::os::unix::fs::MetadataExt;
        if source_meta.dev() != target_meta.dev() || source_meta.ino() != target_meta.ino() {
            let _ = command("systemctl", &["stop", &name]).await;
            self.status(
                &row,
                "Unavailable",
                "Mount source verification failed; writes are stopped",
                &folder,
            )
            .await?;
            return Err(failed("folder.mount_source_mismatch"));
        }
        atomic(&record, &serde_json::to_vec(&vec![name]).map_err(failed)?)?;
        self.status(&row, "Ready", "Local pool bind mount is active", &folder)
            .await?;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn systemd_names_and_mount_ownership_are_stable() {
        assert_eq!(
            mount_name("/storage/my-files"),
            "storage-my\\x2dfiles.mount"
        );
        let u = unit(
            "id",
            Path::new("/mnt/filedock/pools/id/data"),
            Path::new("/storage/data"),
            Path::new("/mnt/filedock/pools/id"),
        );
        assert!(u.contains("BindsTo=mnt-filedock-pools-id.mount"));
        assert!(u.contains("Options=bind"));
        assert!(u.contains("\nWhat=/mnt/filedock/pools/id/data\n"));
        assert!(u.contains("\nWhere=/storage/data\n"));
        assert!(!u.contains("What=\""));
    }
    #[test]
    fn local_exposure_is_never_reported_protected() {
        let f:ManagedFolderSpec=serde_json::from_value(json!({"name":"a","filedockPath":"/storage/a","storageMode":"LocalOnly","reference":{"poolId":"p","relativePath":"/a"}})).unwrap();
        assert_eq!(
            protection(&f, &json!({}), chrono::Utc::now()).0,
            "Unprotected"
        );
    }
}
#[cfg(test)]
mod protection_tests {
    use super::*;
    #[test]
    fn protection_requires_a_recent_durable_point_or_observed_upload_status() {
        let now = chrono::Utc::now();
        let mut folder:ManagedFolderSpec=serde_json::from_value(json!({"name":"data","filedockPath":"/storage/data","storageMode":"HybridBackup","reference":{"poolId":"p","relativePath":"/data","backupDestination":{"poolId":"p","relativePath":"/backup"}},"dataImportance":"Important","advanced":{"backupEnabled":true,"backupIntervalSeconds":900}})).unwrap();
        assert_eq!(protection(&folder, &json!({}), now).0, "AtRisk");
        assert_eq!(
            protection(
                &folder,
                &json!({"lastDurableRecoveryPoint":now.to_rfc3339()}),
                now
            )
            .0,
            "Protected"
        );
        assert_eq!(protection(&folder,&json!({"lastDurableRecoveryPoint":(now-chrono::Duration::minutes(30)).to_rfc3339()}),now).0,"AtRisk");
        folder.data_importance = Some(fd_state::resources::DataImportance::Rebuildable);
        folder.advanced["backupEnabled"] = json!(false);
        assert_eq!(protection(&folder, &json!({}), now).0, "NotRequired");
        folder.storage_mode = StorageMode::CloudMount;
        assert_eq!(protection(&folder, &json!({}), now).0, "AtRisk");
        assert_eq!(
            protection(
                &folder,
                &json!({"uploadStatusKnown":true,"overdueUploads":0}),
                now
            )
            .0,
            "Protected"
        );
    }
}
