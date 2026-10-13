use crate::adapter::{AdapterError, Description, Health, SubsystemAdapter};
use serde_json::Value;

#[derive(Default)]
pub struct KopiaAdapter;

#[async_trait::async_trait]
impl SubsystemAdapter for KopiaAdapter {
    fn id(&self) -> &str {
        "kopia"
    }
    async fn describe(&self) -> Result<Description, AdapterError> {
        super::describe("kopia", "Kopia", "0.23.1", &["backup.snapshot"]).await
    }
    async fn state(&self) -> Result<Value, AdapterError> {
        super::empty_state().await
    }
    async fn configure(&self, _: Value) -> Result<(), AdapterError> {
        Ok(())
    }
    async fn start(&self) -> Result<(), AdapterError> {
        Ok(())
    }
    async fn stop(&self) -> Result<(), AdapterError> {
        Ok(())
    }
    async fn health(&self) -> Result<Health, AdapterError> {
        // This is an on-demand tool check, not a data-plane process. Cancellation kills it.
        let healthy = tokio::process::Command::new("kopia")
            .arg("--version")
            .kill_on_drop(true)
            .output()
            .await
            .is_ok_and(|output| output.status.success());
        Ok(Health {
            healthy,
            detail: if healthy {
                "kopia is ready for on-demand operations"
            } else {
                "kopia could not execute --version"
            }
            .into(),
        })
    }
}
