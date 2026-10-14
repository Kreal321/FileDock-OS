//! Adopt existing independent runtimes by stable name. Provisioning belongs to each story's controller.
use crate::{Controller, ReconcileError};
use fd_state::{
    ResourceKind, ResourceRef, StateStore,
    resources::{ExportProtocol, ResourceSpec},
};
use fd_subsystems::{SubsystemRegistry, Systemd};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Runtime {
    Unit(String),
    ComposeProject(String),
}
impl Runtime {
    /// A native-process manifest is still hosted by systemd, never spawned by the Core.
    pub fn from_manifest(
        id: &str,
        manifest: &serde_json::Value,
    ) -> Result<Option<Self>, ReconcileError> {
        let runtime = &manifest["runtime"];
        match runtime["type"].as_str() {
            Some("systemd-service") => runtime["unit"]
                .as_str()
                .map(|u| Some(Self::Unit(u.into())))
                .ok_or_else(|| ReconcileError::Failed("runtime.unit missing".into())),
            Some("native-process") if id == "kopia" => Ok(None), // on-demand tool, not a daemon
            Some("native-process") => Ok(Some(Self::Unit(match id {
                "fd-s3" => "filedock-s3.service".into(),
                "rclone" => "filedock-rclone.service".into(),
                _ => format!("filedock-extension-{id}.service"),
            }))),
            Some("docker-service") => Ok(Some(Self::ComposeProject(format!("fd-extension-{id}")))),
            _ => Err(ReconcileError::Failed(
                "runtime must be managed by systemd or Docker".into(),
            )),
        }
    }
    pub async fn ensure_running(&self) -> Result<(), ReconcileError> {
        match self {
            Self::Unit(unit) => {
                let status = tokio::process::Command::new("systemctl")
                    .args(["is-active", "--quiet", unit])
                    .kill_on_drop(true)
                    .status()
                    .await
                    .map_err(failed)?;
                if !status.success() {
                    Systemd::default().start(unit).await.map_err(failed)?;
                }
            }
            Self::ComposeProject(project) => {
                let ids = tokio::process::Command::new("docker")
                    .args([
                        "ps",
                        "-aq",
                        "--filter",
                        &format!("label=com.docker.compose.project={project}"),
                    ])
                    .kill_on_drop(true)
                    .output()
                    .await
                    .map_err(failed)?;
                if !ids.status.success() {
                    return Err(failed("Docker inspection failed"));
                }
                let ids = String::from_utf8_lossy(&ids.stdout)
                    .split_whitespace()
                    .map(str::to_owned)
                    .collect::<Vec<_>>();
                if ids.is_empty() {
                    return Err(failed(format!(
                        "{project}: runtime has not been provisioned"
                    )));
                }
                // start is idempotent and never recreates an already running container.
                let status = tokio::process::Command::new("docker")
                    .arg("start")
                    .args(ids)
                    .kill_on_drop(true)
                    .status()
                    .await
                    .map_err(failed)?;
                if !status.success() {
                    return Err(failed("container adoption failed"));
                }
            }
        }
        Ok(())
    }
}
fn failed(error: impl std::fmt::Display) -> ReconcileError {
    ReconcileError::Failed(error.to_string())
}
async fn mount_unit(path: &str) -> Result<Runtime, ReconcileError> {
    let output = tokio::process::Command::new("systemd-escape")
        .args(["--path", "--suffix=mount", path])
        .kill_on_drop(true)
        .output()
        .await
        .map_err(failed)?;
    if !output.status.success() {
        return Err(failed("mount unit name could not be resolved"));
    }
    Ok(Runtime::Unit(
        String::from_utf8_lossy(&output.stdout).trim().into(),
    ))
}
pub struct AdoptionController {
    pub kind: ResourceKind,
    pub store: Arc<StateStore>,
    pub events: Arc<fd_observe::EventBus>,
}
#[async_trait::async_trait]
impl Controller for AdoptionController {
    fn kind(&self) -> ResourceKind {
        self.kind
    }
    async fn reconcile(&self, resource: &ResourceRef) -> Result<(), ReconcileError> {
        let Some(row) = self.store.get(resource).map_err(failed)? else {
            return Ok(());
        };
        if row.deleted {
            return Ok(());
        }
        let runtime = match row.parsed_spec().map_err(|e| failed(e.message))? {
            ResourceSpec::Pool(_) => {
                Some(mount_unit(&format!("/mnt/filedock/pools/{}", resource.id)).await?)
            }
            ResourceSpec::ManagedFolder(folder) => Some(mount_unit(&folder.filedock_path).await?),
            ResourceSpec::Export(export) => Some(Runtime::Unit(match export.protocol {
                ExportProtocol::Smb => "smbd.service".into(),
                ExportProtocol::Nfs => "nfs-server.service".into(),
                ExportProtocol::WebDav => format!("filedock-webdav-{}.service", resource.id),
            })),
            ResourceSpec::ContainerApp(app) => {
                Some(Runtime::ComposeProject(format!("fd-{}", app.name)))
            }
            // The extension lifecycle owns sandboxed activation and supervision.
            ResourceSpec::Extension(_) => return Ok(()),
            _ => None,
        };
        if let Some(runtime) = runtime {
            if let Runtime::Unit(unit) = &runtime {
                let registry = SubsystemRegistry::new(self.store.db().clone());
                for subsystem in registry.list().map_err(failed)? {
                    if Runtime::from_manifest(&subsystem.id, &subsystem.manifest)
                        .ok()
                        .flatten()
                        == Some(Runtime::Unit(unit.clone()))
                        && (!subsystem.enabled
                            || matches!(
                                subsystem.state.as_str(),
                                "Failed" | "Backoff" | "Starting"
                            ))
                    {
                        return Err(failed(format!(
                            "{}: supervision {}",
                            subsystem.id, subsystem.state
                        )));
                    }
                }
            }
            let outcome =
                tokio::time::timeout(std::time::Duration::from_secs(10), runtime.ensure_running())
                    .await
                    .map_err(|_| failed("runtime adoption timed out"))?;
            let status = serde_json::json!({"state":if outcome.is_ok(){"Ready"}else{"Failed"},"runtime":format!("{runtime:?}")});
            if row.status.as_ref() != Some(&status) {
                self.store.write_status(resource, &status).map_err(failed)?;
                let event = match resource.kind {
                    ResourceKind::Pool | ResourceKind::ManagedFolder => {
                        if outcome.is_ok() {
                            "mount.ready"
                        } else {
                            "mount.failed"
                        }
                    }
                    _ => {
                        if outcome.is_ok() {
                            "resource.reconciled"
                        } else {
                            "resource.failed"
                        }
                    }
                };
                self.events
                    .emit(event, Some(resource.clone()), status)
                    .map_err(failed)?;
            }
            outcome?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_data_plane_runtime_has_an_independent_owner() {
        assert_eq!(
            Runtime::from_manifest(
                "sample",
                &serde_json::json!({"runtime":{"type":"native-process","executable":"/bin/false"}})
            )
            .unwrap(),
            Some(Runtime::Unit("filedock-extension-sample.service".into()))
        );
        assert_eq!(
            Runtime::from_manifest(
                "sample",
                &serde_json::json!({"runtime":{"type":"docker-service"}})
            )
            .unwrap(),
            Some(Runtime::ComposeProject("fd-extension-sample".into()))
        );
        assert!(
            Runtime::from_manifest(
                "sample",
                &serde_json::json!({"runtime":{"type":"child-process"}})
            )
            .is_err()
        );
    }
}
