use serde::{Deserialize, Serialize};

use super::check_name;
use crate::ValidationIssue;

/// Appliance-wide desired settings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SettingsSpec {
    pub appliance_name: String,
    #[serde(default)]
    pub notification_channels: Vec<NotificationChannel>,
}

impl SettingsSpec {
    pub(crate) fn validate(&self, issues: &mut Vec<ValidationIssue>) {
        check_name("applianceName", &self.appliance_name, 64, issues);
        let mut ids = std::collections::BTreeSet::new();
        for channel in &self.notification_channels {
            if !super::is_uuid(&channel.id)
                || !ids.insert(&channel.id)
                || !matches!(
                    channel.min_severity.as_str(),
                    "Info" | "Warning" | "Critical"
                )
                || channel.name.trim().is_empty()
                || channel.name.len() > 128
                || channel.subsystem_id.is_empty()
                || !channel.config.is_object()
            {
                issues.push(ValidationIssue::error("notification.channel_invalid","notificationChannels","Each channel requires a unique UUID, name, provider, severity and object configuration"));
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NotificationChannel {
    pub id: String,
    pub subsystem_id: String,
    pub name: String,
    pub min_severity: String,
    #[serde(default)]
    pub config: serde_json::Value,
    #[serde(default)]
    pub secret_refs: Vec<String>,
}
