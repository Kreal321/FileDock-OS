use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::is_uuid;
use crate::ValidationIssue;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum ExtensionSignature {
    #[serde(rename_all = "camelCase")]
    Signed {
        key_id: String,
    },
    Unsigned {
        acknowledged: bool,
    },
}

/// An installed extension: what restore needs to reinstall it from its stored package (FR-109,
/// FR-110).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExtensionSpec {
    pub subsystem_id: String,
    pub version: String,
    pub package_sha256: String,
    #[serde(default = "empty_object")]
    pub config: Value,
    #[serde(default)]
    pub approved_permissions: Vec<String>,
    pub signature: ExtensionSignature,
    /// Declared data path name to managed folder id.
    #[serde(default)]
    pub data_bindings: BTreeMap<String, String>,
}

fn empty_object() -> Value {
    Value::Object(Default::default())
}

impl ExtensionSpec {
    pub(crate) fn validate(&self, issues: &mut Vec<ValidationIssue>) {
        let id = self.subsystem_id.as_bytes();
        let id_ok = (2..=64).contains(&id.len())
            && id[0].is_ascii_lowercase()
            && id
                .iter()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-');
        if !id_ok {
            issues.push(ValidationIssue::error(
                "extension.id_invalid",
                "subsystemId",
                "not a valid subsystem id",
            ));
        }
        let sha_ok = self.package_sha256.len() == 64
            && self
                .package_sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        if !sha_ok {
            issues.push(ValidationIssue::error(
                "extension.package_hash_invalid",
                "packageSha256",
                "must be a lowercase hex SHA-256",
            ));
        }
        if self.version.is_empty() {
            issues.push(ValidationIssue::error(
                "extension.version_invalid",
                "version",
                "a version is required",
            ));
        }
        if let ExtensionSignature::Unsigned {
            acknowledged: false,
        } = self.signature
        {
            issues.push(ValidationIssue::error(
                "extension.unsigned_unacknowledged",
                "signature",
                "an unsigned extension must be acknowledged",
            ));
        }
        for (name, folder) in &self.data_bindings {
            if !is_uuid(folder) {
                issues.push(ValidationIssue::error(
                    "extension.binding_invalid",
                    &format!("dataBindings.{name}"),
                    "must be a managed folder id",
                ));
            }
        }
    }
}
