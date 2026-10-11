use serde::{Deserialize, Serialize};

use super::is_uuid;
use crate::ValidationIssue;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum StateDestinationTarget {
    #[serde(rename_all = "camelCase")]
    ObjectStorage {
        connection_id: String,
        bucket: String,
        prefix: String,
    },
    #[serde(rename_all = "camelCase")]
    RemovableDisk { path: String },
}

/// The single remote appliance-state destination (FR-037). Local copies need no configuration.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplianceStateDestinationSpec {
    pub target: StateDestinationTarget,
}

impl ApplianceStateDestinationSpec {
    pub(crate) fn validate(&self, issues: &mut Vec<ValidationIssue>) {
        match &self.target {
            StateDestinationTarget::ObjectStorage {
                connection_id,
                bucket,
                prefix,
            } => {
                if !is_uuid(connection_id) {
                    issues.push(ValidationIssue::error(
                        "destination.connection_invalid",
                        "target.objectStorage.connectionId",
                        "must be a connection id",
                    ));
                }
                if bucket.is_empty() {
                    issues.push(ValidationIssue::error(
                        "destination.bucket_required",
                        "target.objectStorage.bucket",
                        "a bucket is required",
                    ));
                }
                if prefix.starts_with('/') || prefix.split('/').any(|s| s == "..") {
                    issues.push(ValidationIssue::error(
                        "destination.prefix_invalid",
                        "target.objectStorage.prefix",
                        "the prefix is relative to the bucket",
                    ));
                }
            }
            StateDestinationTarget::RemovableDisk { path } => {
                if !path.starts_with('/') || path.split('/').any(|s| s == "..") {
                    issues.push(ValidationIssue::error(
                        "destination.path_invalid",
                        "target.removableDisk.path",
                        "must be an absolute path",
                    ));
                }
            }
        }
    }
}
