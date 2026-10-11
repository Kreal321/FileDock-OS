use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::check_secret_ref;
use super::connector_routes::{RouteTarget, RouteUse};
use crate::ValidationIssue;

/// Families that can host a single-writer location (FR-088).
pub const OBJECT_STORAGE_FAMILIES: &[&str] = &["s3-compatible", "azure-blob"];

const KNOWN_FAMILIES: &[&str] = &[
    "s3-compatible",
    "azure-blob",
    "google-drive",
    "onedrive",
    "dropbox",
];

pub fn is_object_storage_family(family: &str) -> bool {
    OBJECT_STORAGE_FAMILIES.contains(&family)
}

pub fn is_valid_family(family: &str) -> bool {
    KNOWN_FAMILIES.contains(&family)
        || {
            let rest = family.strip_prefix("x-").unwrap_or("");
            let mut parts = rest.splitn(2, '-');
            matches!(
                (parts.next(), parts.next()),
                (Some(connector), Some(name))
                    if !connector.is_empty()
                        && !name.is_empty()
                        && family.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            )
        }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AddressingStyle {
    Path,
    Virtual,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum ServerSideEncryption {
    ProviderDefault,
    #[serde(rename_all = "camelCase")]
    Kms {
        key_id: String,
    },
}

/// Connector-neutral S3 definition (FR-114). Static keys only; HTTPS only.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct S3Definition {
    pub endpoint: String,
    pub region: String,
    pub addressing_style: AddressingStyle,
    pub bucket: String,
    pub access_key_ref: String,
    pub secret_key_ref: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage_class: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_side_encryption: Option<ServerSideEncryption>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bandwidth_limit: Option<u64>,
}

/// Connector-bound definition: validated later against that connector's `connectionSchema`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BoundDefinition {
    pub connector_id: String,
    #[serde(default)]
    pub config: serde_json::Value,
    #[serde(default)]
    pub credentials: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ConnectionDefinition {
    S3(S3Definition),
    Bound(BoundDefinition),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CloudConnectionSpec {
    pub name: String,
    pub provider_family: String,
    pub definition: ConnectionDefinition,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub route_overrides: BTreeMap<RouteUse, RouteTarget>,
}

impl CloudConnectionSpec {
    pub fn is_object_storage(&self) -> bool {
        is_object_storage_family(&self.provider_family)
    }

    pub fn bound_connector(&self) -> Option<&str> {
        match &self.definition {
            ConnectionDefinition::Bound(bound) => Some(bound.connector_id.as_str()),
            ConnectionDefinition::S3(_) => None,
        }
    }

    pub fn validate(&self, issues: &mut Vec<ValidationIssue>) {
        super::check_name("name", &self.name, 64, issues);
        if !is_valid_family(&self.provider_family) {
            issues.push(ValidationIssue::error(
                "connection.family_invalid",
                "providerFamily",
                "unknown provider family",
            ));
        }
        match &self.definition {
            ConnectionDefinition::S3(s3) => {
                if self.provider_family != "s3-compatible" {
                    issues.push(ValidationIssue::error(
                        "connection.definition_mismatch",
                        "definition",
                        "an S3 definition is only valid for s3-compatible",
                    ));
                }
                validate_s3(s3, issues);
            }
            ConnectionDefinition::Bound(bound) => {
                if self.provider_family == "s3-compatible" {
                    issues.push(ValidationIssue::error(
                        "connection.definition_mismatch",
                        "definition",
                        "s3-compatible connections use the connector-neutral S3 definition",
                    ));
                }
                if bound.connector_id.is_empty() {
                    issues.push(ValidationIssue::error(
                        "connection.connector_required",
                        "definition.connectorId",
                        "a bound definition names its connector",
                    ));
                }
                for (index, reference) in bound.credentials.iter().enumerate() {
                    check_secret_ref(
                        &format!("definition.credentials[{index}]"),
                        reference,
                        issues,
                    );
                }
            }
        }
        for (use_kind, target) in &self.route_overrides {
            if let Err(issue) = target.validate_for(*use_kind, self.bound_connector()) {
                issues.push(issue.under("routeOverrides"));
            }
        }
    }
}

fn validate_s3(s3: &S3Definition, issues: &mut Vec<ValidationIssue>) {
    if let Some(rest) = s3.endpoint.strip_prefix("https://") {
        let host = rest.split(['/', '?', '#']).next().unwrap_or("");
        if host.is_empty() || host.contains(char::is_whitespace) {
            issues.push(ValidationIssue::error(
                "connection.endpoint_invalid",
                "definition.endpoint",
                "the endpoint is not a URL",
            ));
        }
    } else if s3.endpoint.starts_with("http://") {
        issues.push(ValidationIssue::error(
            "connection.https_required",
            "definition.endpoint",
            "the endpoint must be https",
        ));
    } else {
        issues.push(ValidationIssue::error(
            "connection.endpoint_invalid",
            "definition.endpoint",
            "the endpoint is not a URL",
        ));
    }
    if s3.region.is_empty() {
        issues.push(ValidationIssue::error(
            "connection.region_required",
            "definition.region",
            "a region is required",
        ));
    }
    if s3.bucket.is_empty() {
        issues.push(ValidationIssue::error(
            "connection.bucket_required",
            "definition.bucket",
            "a bucket is required",
        ));
    }
    check_secret_ref("definition.accessKeyRef", &s3.access_key_ref, issues);
    check_secret_ref("definition.secretKeyRef", &s3.secret_key_ref, issues);
    if let ServerSideEncryption::Kms { key_id } = s3
        .server_side_encryption
        .as_ref()
        .unwrap_or(&ServerSideEncryption::ProviderDefault)
        && key_id.is_empty()
    {
        issues.push(ValidationIssue::error(
            "connection.kms_key_required",
            "definition.serverSideEncryption.keyId",
            "a KMS key id is required",
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn families() {
        assert!(is_valid_family("s3-compatible"));
        assert!(is_valid_family("x-sample-drive-archive"));
        assert!(!is_valid_family("ftp"));
        assert!(!is_valid_family("x-"));
        assert!(is_object_storage_family("azure-blob"));
        assert!(!is_object_storage_family("google-drive"));
    }

    #[test]
    fn https_and_static_keys() {
        let spec: CloudConnectionSpec = serde_json::from_value(json!({
            "name": "wasabi",
            "providerFamily": "s3-compatible",
            "definition": {
                "type": "s3",
                "endpoint": "http://s3.example",
                "region": "us-east-1",
                "addressingStyle": "Virtual",
                "bucket": "b",
                "accessKeyRef": "connection/1/access-key",
                "secretKeyRef": "connection/1/secret-key"
            }
        }))
        .unwrap();
        let mut issues = Vec::new();
        spec.validate(&mut issues);
        assert!(issues.iter().any(|i| i.code == "connection.https_required"));
    }
}
