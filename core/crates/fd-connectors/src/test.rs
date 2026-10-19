//! TestConnection and the daily probe scheduler (FR-083, FR-117, Principle V).

use serde::{Deserialize, Serialize};

use fd_api::extension::v1 as ext;
use fd_state::resources::RouteUse;

use crate::routes::{RouteError, resolve};
use fd_state::resources::{CloudConnectionSpec, ConnectorRoutesSpec};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StoredTestResult {
    pub tested_at: String,
    pub connector_id: String,
    pub checks: Vec<(String, bool, String)>,
    pub conditional_create: Option<bool>,
    pub conditional_replace: Option<bool>,
    pub provider_checksums: bool,
    pub clock_skew_ms: i64,
    pub write_steps_run: bool,
}

impl StoredTestResult {
    pub fn from_probe(connector_id: &str, tested_at: String, probe: &ext::ProbeResult) -> Self {
        Self {
            tested_at,
            connector_id: connector_id.into(),
            checks: probe
                .checks
                .iter()
                .map(|c| (c.name.clone(), c.ok, c.detail.clone()))
                .collect(),
            conditional_create: probe.write_steps_run.then_some(probe.conditional_create),
            conditional_replace: probe.write_steps_run.then_some(probe.conditional_replace),
            provider_checksums: probe.provider_checksums,
            clock_skew_ms: probe.clock_skew_ms,
            write_steps_run: probe.write_steps_run,
        }
    }

    /// Claim safety treats a not-tested result as not proven.
    pub fn proves_conditional(&self) -> bool {
        self.write_steps_run
            && self.conditional_create == Some(true)
            && self.conditional_replace == Some(true)
    }
}

pub struct ConnectionTester;

impl ConnectionTester {
    /// `read_only` when owner.json says this appliance isn't the Writer (including restore re-probe).
    pub fn probe_prefix(appliance_id: &str, random: &str) -> String {
        format!("_filedock/probe/{appliance_id}/{random}")
    }

    pub fn routed_connector(
        routes: &ConnectorRoutesSpec,
        connection: &CloudConnectionSpec,
        use_kind: RouteUse,
    ) -> Result<String, RouteError> {
        match resolve(
            routes,
            Some(connection),
            &connection.provider_family,
            use_kind,
        )? {
            fd_state::resources::RouteTarget::Connector { subsystem_id } => Ok(subsystem_id),
            fd_state::resources::RouteTarget::BackupEngine => Ok("backup-engine".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_only_probe_is_not_conditional() {
        let result = StoredTestResult::from_probe(
            "fd-s3",
            "now".into(),
            &ext::ProbeResult {
                checks: vec![],
                conditional_create: false,
                conditional_replace: false,
                provider_checksums: false,
                clock_skew_ms: 0,
                write_steps_run: false,
            },
        );
        assert!(result.conditional_create.is_none());
        assert!(!result.proves_conditional());
    }
}
