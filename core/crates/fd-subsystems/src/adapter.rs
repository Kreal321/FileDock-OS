//! Core-side adapter for the generic subsystem contract (FR-049).

use serde_json::Value;

#[derive(Debug, thiserror::Error)]
pub enum AdapterError {
    #[error("{0}")]
    Failed(String),
    #[error("not implemented")]
    Unimplemented,
}

#[derive(Clone, Debug)]
pub struct Description {
    pub id: String,
    pub name: String,
    pub version: String,
    pub capabilities: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Health {
    pub healthy: bool,
    pub detail: String,
}

/// Implemented by Samba, NFS, Docker, Kopia, rclone, and extension clients.
#[async_trait::async_trait]
pub trait SubsystemAdapter: Send + Sync {
    fn id(&self) -> &str;
    fn startup_grace(&self) -> std::time::Duration {
        std::time::Duration::from_secs(5)
    }
    async fn describe(&self) -> Result<Description, AdapterError>;
    async fn state(&self) -> Result<Value, AdapterError>;
    async fn configure(&self, desired: Value) -> Result<(), AdapterError>;
    async fn start(&self) -> Result<(), AdapterError>;
    async fn stop(&self) -> Result<(), AdapterError>;
    async fn restart(&self) -> Result<(), AdapterError> {
        self.stop().await?;
        self.start().await
    }
    async fn reload(&self) -> Result<(), AdapterError> {
        Ok(())
    }
    async fn health(&self) -> Result<Health, AdapterError>;
    async fn diagnostics(&self) -> Result<Value, AdapterError> {
        Ok(Value::Object(Default::default()))
    }
}
