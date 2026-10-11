//! Health-only built-in adapters (US1). Full configure/start land with later stories.

pub mod docker;
pub mod kopia;
pub mod nfs;
pub mod samba;

pub use docker::DockerAdapter;
pub use kopia::KopiaAdapter;
pub use nfs::NfsAdapter;
pub use samba::SambaAdapter;

use crate::adapter::{AdapterError, Description, Health, SubsystemAdapter};
use serde_json::Value;

pub(crate) fn unit_active(unit: &str) -> bool {
    std::process::Command::new("systemctl")
        .args(["is-active", "--quiet", unit])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

pub(crate) fn binary_present(name: &str) -> bool {
    std::process::Command::new("sh")
        .args(["-c", &format!("command -v {name} >/dev/null")])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

pub(crate) async fn describe(
    id: &str,
    name: &str,
    version: &str,
    capabilities: &[&str],
) -> Result<Description, AdapterError> {
    Ok(Description {
        id: id.into(),
        name: name.into(),
        version: version.into(),
        capabilities: capabilities.iter().map(|c| (*c).to_string()).collect(),
    })
}

pub(crate) async fn noop_ok() -> Result<(), AdapterError> {
    Ok(())
}

pub(crate) async fn empty_state() -> Result<Value, AdapterError> {
    Ok(Value::Object(Default::default()))
}

pub(crate) fn systemd_health(unit: &str) -> Health {
    if !binary_present("systemctl") {
        return Health {
            healthy: true,
            detail: "systemd not available; health assumed for this host".into(),
        };
    }
    if unit_active(unit) {
        Health {
            healthy: true,
            detail: format!("{unit} is active"),
        }
    } else {
        Health {
            healthy: false,
            detail: format!("{unit} is not active"),
        }
    }
}

pub(crate) fn process_health(name: &str) -> Health {
    if binary_present(name) {
        Health {
            healthy: true,
            detail: format!("{name} is installed"),
        }
    } else {
        Health {
            healthy: false,
            detail: format!("{name} is not installed"),
        }
    }
}

/// Shared no-op lifecycle for health-only adapters.
pub trait HealthOnly: Send + Sync {
    const ID: &'static str;
    const NAME: &'static str;
    const VERSION: &'static str;
    const CAPABILITIES: &'static [&'static str];
    fn probe(&self) -> Health;
}

#[async_trait::async_trait]
impl<T: HealthOnly> SubsystemAdapter for T {
    fn id(&self) -> &str {
        T::ID
    }

    async fn describe(&self) -> Result<Description, AdapterError> {
        describe(T::ID, T::NAME, T::VERSION, T::CAPABILITIES).await
    }

    async fn state(&self) -> Result<Value, AdapterError> {
        empty_state().await
    }

    async fn configure(&self, _desired: Value) -> Result<(), AdapterError> {
        noop_ok().await
    }

    async fn start(&self) -> Result<(), AdapterError> {
        noop_ok().await
    }

    async fn stop(&self) -> Result<(), AdapterError> {
        noop_ok().await
    }

    async fn health(&self) -> Result<Health, AdapterError> {
        Ok(self.probe())
    }
}
