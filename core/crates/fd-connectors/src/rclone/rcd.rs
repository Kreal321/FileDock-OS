//! `rclone rcd` transient unit on `/run/filedock/rclone.sock`.

use std::path::PathBuf;

use fd_subsystems::systemd::Systemd;

pub const SOCKET: &str = "/run/filedock/rclone.sock";

pub struct RcloneDaemon {
    pub socket: PathBuf,
    pub config: PathBuf,
    systemd: Systemd,
}

impl RcloneDaemon {
    pub fn new(systemd: Systemd, config: impl Into<PathBuf>) -> Self {
        Self {
            socket: PathBuf::from(SOCKET),
            config: config.into(),
            systemd,
        }
    }

    pub async fn ensure_running(&self) -> Result<(), String> {
        let mut props = std::collections::HashMap::new();
        props.insert("Type".into(), "notify".into());
        props.insert(
            "ExecStart".into(),
            format!(
                "/usr/bin/rclone rcd --rc-addr unix://{} --config {} --rc-no-auth",
                self.socket.display(),
                self.config.display()
            ),
        );
        self.systemd
            .start_transient("filedock-rclone.service", props)
            .await
            .map_err(|e| e.to_string())
    }
}
