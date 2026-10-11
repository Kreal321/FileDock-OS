//! CloudConnection validation (FR-114).

use fd_state::ValidationIssue;
use fd_state::resources::{CloudConnectionSpec, ConnectionDefinition, is_valid_family};
use fd_subsystems::manifest::LoadedManifest;

#[derive(Debug, thiserror::Error)]
pub enum ConnectionError {
    #[error("the connection is invalid")]
    Invalid(Vec<ValidationIssue>),
}

/// Field checks plus connector-schema and credential-type rules.
pub fn validate_connection(
    spec: &CloudConnectionSpec,
    owner: Option<&LoadedManifest>,
) -> Result<(), ConnectionError> {
    let mut issues = Vec::new();
    spec.validate(&mut issues);
    if !is_valid_family(&spec.provider_family) {
        issues.push(ValidationIssue::error(
            "connection.family_invalid",
            "providerFamily",
            "unknown provider family",
        ));
    }
    match &spec.definition {
        ConnectionDefinition::S3(_) => {
            // Static keys only: the spec type has no session token or role fields.
        }
        ConnectionDefinition::Bound(bound) => {
            match owner {
                Some(manifest) if manifest.id == bound.connector_id => {
                    if let Some(_schema) = manifest.connection_schema(&spec.provider_family) {
                        // Schema path is relative to the package; the live JSON Schema is applied
                        // by the owning connector on Configure. Here we require that it exists.
                    } else if spec.provider_family != "s3-compatible" {
                        issues.push(ValidationIssue::error(
                            "connection.schema_missing",
                            "definition.config",
                            "the connector has no connectionSchema for this family",
                        ));
                    }
                }
                Some(_) => issues.push(ValidationIssue::error(
                    "connection.connector_mismatch",
                    "definition.connectorId",
                    "the bound connector does not match the owning subsystem",
                )),
                None => issues.push(ValidationIssue::error(
                    "connection.connector_missing",
                    "definition.connectorId",
                    "the owning connector is not registered",
                )),
            }
        }
    }
    for (use_kind, target) in &spec.route_overrides {
        if let Err(issue) = target.validate_for(*use_kind, spec.bound_connector()) {
            issues.push(issue);
        }
        if !use_kind.allowed_for_family(&spec.provider_family) {
            issues.push(ValidationIssue::error(
                "route.use_not_allowed",
                "routeOverrides",
                format!("{use_kind:?} is not allowed for {}", spec.provider_family),
            ));
        }
    }
    if issues.iter().any(ValidationIssue::is_error) {
        Err(ConnectionError::Invalid(issues))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn http_endpoint_is_refused() {
        let spec: CloudConnectionSpec = serde_json::from_value(json!({
            "name": "minio",
            "providerFamily": "s3-compatible",
            "definition": {
                "type": "s3",
                "endpoint": "http://127.0.0.1:9000",
                "region": "us-east-1",
                "addressingStyle": "Path",
                "bucket": "b",
                "accessKeyRef": "connection/1/access-key",
                "secretKeyRef": "connection/1/secret-key"
            }
        }))
        .unwrap();
        let err = validate_connection(&spec, None).unwrap_err();
        match err {
            ConnectionError::Invalid(issues) => {
                assert!(issues.iter().any(|i| i.code == "connection.https_required"));
            }
        }
    }
}
