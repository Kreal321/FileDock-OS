//! systemd helper over D-Bus (research R7).

use std::collections::HashMap;
use std::path::Path;

use crate::adapter::AdapterError;

/// Talks to systemd the way `systemctl` and `systemd-run` do.
#[derive(Clone, Default)]
pub struct Systemd {
    unit_dir: std::path::PathBuf,
}

impl Systemd {
    pub fn new(unit_dir: impl AsRef<Path>) -> Self {
        Self {
            unit_dir: unit_dir.as_ref().to_path_buf(),
        }
    }

    pub async fn start(&self, unit: &str) -> Result<(), AdapterError> {
        self.unit_action("StartUnit", unit).await
    }

    pub async fn stop(&self, unit: &str) -> Result<(), AdapterError> {
        self.unit_action("StopUnit", unit).await
    }

    pub async fn restart(&self, unit: &str) -> Result<(), AdapterError> {
        self.unit_action("RestartUnit", unit).await
    }

    /// Create a transient unit, as `systemd-run` does.
    pub async fn start_transient(
        &self,
        name: &str,
        properties: HashMap<String, String>,
    ) -> Result<(), AdapterError> {
        #[cfg(target_os = "linux")]
        {
            self.start_transient_linux(name, properties).await
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (name, properties);
            Err(AdapterError::Unimplemented)
        }
    }

    pub fn write_mount_unit(
        &self,
        name: &str,
        what: &str,
        where_: &str,
        options: &str,
    ) -> Result<(), AdapterError> {
        std::fs::create_dir_all(&self.unit_dir).map_err(|e| AdapterError::Failed(e.to_string()))?;
        let body = format!(
            "[Unit]\nDescription=FileDock mount {where_}\n\n[Mount]\nWhat={what}\nWhere={where_}\nType=ext4\nOptions={options}\n"
        );
        std::fs::write(self.unit_dir.join(name), body)
            .map_err(|e| AdapterError::Failed(e.to_string()))
    }

    pub fn write_dropin(&self, unit: &str, dropin: &str, body: &str) -> Result<(), AdapterError> {
        let dir = self.unit_dir.join(format!("{unit}.d"));
        std::fs::create_dir_all(&dir).map_err(|e| AdapterError::Failed(e.to_string()))?;
        std::fs::write(dir.join(dropin), body).map_err(|e| AdapterError::Failed(e.to_string()))
    }

    async fn unit_action(&self, method: &str, unit: &str) -> Result<(), AdapterError> {
        #[cfg(target_os = "linux")]
        {
            self.unit_action_linux(method, unit).await
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (method, unit);
            Err(AdapterError::Unimplemented)
        }
    }
}

#[cfg(target_os = "linux")]
impl Systemd {
    async fn unit_action_linux(&self, method: &str, unit: &str) -> Result<(), AdapterError> {
        let conn = zbus::Connection::system()
            .await
            .map_err(|e| AdapterError::Failed(e.to_string()))?;
        conn.call_method(
            Some("org.freedesktop.systemd1"),
            "/org/freedesktop/systemd1",
            Some("org.freedesktop.systemd1.Manager"),
            method,
            &(unit, "replace"),
        )
        .await
        .map_err(|e| AdapterError::Failed(e.to_string()))?;
        Ok(())
    }

    async fn start_transient_linux(
        &self,
        name: &str,
        properties: HashMap<String, String>,
    ) -> Result<(), AdapterError> {
        let conn = zbus::Connection::system()
            .await
            .map_err(|e| AdapterError::Failed(e.to_string()))?;
        if properties
            .get("ExecStart")
            .is_some_and(|v| v.trim().is_empty())
        {
            return Err(AdapterError::Failed("ExecStart must not be empty".into()));
        }
        let props: Vec<(String, zbus::zvariant::Value<'_>)> = properties
            .iter()
            .map(|(k, v)| {
                let value = if k == "ExecStart" {
                    let args: Vec<String> = v.split_whitespace().map(str::to_owned).collect();
                    zbus::zvariant::Value::from(vec![(args[0].clone(), args, false)])
                } else {
                    zbus::zvariant::Value::from(v.as_str())
                };
                (k.clone(), value)
            })
            .collect();
        conn.call_method(
            Some("org.freedesktop.systemd1"),
            "/org/freedesktop/systemd1",
            Some("org.freedesktop.systemd1.Manager"),
            "StartTransientUnit",
            &(
                name,
                "replace",
                props,
                Vec::<(String, Vec<(String, zbus::zvariant::OwnedValue)>)>::new(),
            ),
        )
        .await
        .map_err(|e| AdapterError::Failed(e.to_string()))?;
        Ok(())
    }
}
