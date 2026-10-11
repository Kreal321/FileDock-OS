use serde::{Deserialize, Serialize};

use super::check_name;
use crate::ValidationIssue;

/// Appliance-wide desired settings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SettingsSpec {
    pub appliance_name: String,
}

impl SettingsSpec {
    pub(crate) fn validate(&self, issues: &mut Vec<ValidationIssue>) {
        check_name("applianceName", &self.appliance_name, 64, issues);
    }
}
