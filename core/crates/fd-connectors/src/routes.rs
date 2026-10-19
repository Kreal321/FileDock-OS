//! ConnectorRoutes resolution and validation (FR-115, FR-116, research R13).

use fd_state::resources::{
    CloudConnectionSpec, ConnectionDefinition, ConnectorRoutesSpec, RouteTarget, RouteUse,
};
use fd_state::{ResourceKind, ResourceRef, ValidationIssue};
use fd_subsystems::registry::SubsystemRecord;

#[derive(Debug, thiserror::Error)]
pub enum RouteError {
    #[error("route.missing")]
    Missing,
    #[error("{0:?}")]
    Invalid(ValidationIssue),
}

pub struct RouteResolver<'a> {
    pub routes: &'a ConnectorRoutesSpec,
    pub connection: Option<&'a CloudConnectionSpec>,
}

/// Resolution: connection override, then family default, then `route.missing`.
pub fn resolve(
    routes: &ConnectorRoutesSpec,
    connection: Option<&CloudConnectionSpec>,
    family: &str,
    use_kind: RouteUse,
) -> Result<RouteTarget, RouteError> {
    if let Some(connection) = connection
        && let Some(target) = connection.route_overrides.get(&use_kind)
    {
        return Ok(target.clone());
    }
    routes
        .resolve(family, use_kind)
        .cloned()
        .ok_or(RouteError::Missing)
}

pub fn validate_target(
    family: &str,
    use_kind: RouteUse,
    target: &RouteTarget,
    connection: Option<&CloudConnectionSpec>,
    connectors: &[SubsystemRecord],
    backup_engine: Option<&SubsystemRecord>,
) -> Result<(), RouteError> {
    if !use_kind.allowed_for_family(family) {
        return Err(RouteError::Invalid(ValidationIssue::error(
            "route.use_not_allowed",
            "use",
            format!("{use_kind:?} is not allowed for {family}"),
        )));
    }
    match target {
        RouteTarget::BackupEngine => {
            if use_kind != RouteUse::BackupRepository {
                return Err(RouteError::Invalid(ValidationIssue::error(
                    "route.use_not_allowed",
                    "target",
                    "backup-engine is only allowed for BackupRepository",
                )));
            }
            let Some(engine) = backup_engine else {
                return Err(RouteError::Invalid(ValidationIssue::error(
                    "route.use_not_allowed",
                    "target",
                    "the backup engine is not registered",
                )));
            };
            if !engine.enabled {
                return Err(RouteError::Invalid(ValidationIssue::error(
                    "route.connector_disabled",
                    "target",
                    "the backup engine is disabled",
                )));
            }
            let families = engine
                .manifest
                .get("cloud")
                .and_then(serde_json::Value::as_array)
                .map(|rows| {
                    rows.iter()
                        .filter_map(|row| row.get("family")?.as_str().map(str::to_owned))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_else(|| vec!["s3-compatible".into(), "azure-blob".into()]);
            if !families.iter().any(|f| f == family) {
                return Err(RouteError::Invalid(ValidationIssue::error(
                    "route.use_not_allowed",
                    "target",
                    format!("the backup engine does not declare {family}"),
                )));
            }
        }
        RouteTarget::Connector { subsystem_id } => {
            let Some(connector) = connectors.iter().find(|c| c.id == *subsystem_id) else {
                return Err(RouteError::Invalid(
                    ValidationIssue::error(
                        "route.connector_missing",
                        "target.subsystemId",
                        "the connector is not registered",
                    )
                    .with_conflicting(ResourceRef::new(ResourceKind::Extension, subsystem_id)),
                ));
            };
            if !connector.enabled {
                return Err(RouteError::Invalid(ValidationIssue::error(
                    "route.connector_disabled",
                    "target.subsystemId",
                    "the connector is disabled",
                )));
            }
            let capability = use_kind.required_capability();
            let ok = connector
                .manifest
                .get("cloud")
                .and_then(serde_json::Value::as_array)
                .is_some_and(|rows| {
                    rows.iter().any(|row| {
                        row.get("family").and_then(serde_json::Value::as_str) == Some(family)
                            && row
                                .get("capabilities")
                                .and_then(serde_json::Value::as_array)
                                .is_some_and(|caps| {
                                    caps.iter().any(|c| c.as_str() == Some(capability))
                                })
                    })
                });
            if !ok {
                return Err(RouteError::Invalid(ValidationIssue::error(
                    "route.connector_incapable",
                    "target.subsystemId",
                    format!("{subsystem_id} does not declare {family}/{capability}"),
                )));
            }
            if let Some(ConnectionDefinition::Bound(bound)) = connection.map(|c| &c.definition)
                && bound.connector_id != *subsystem_id
            {
                return Err(RouteError::Invalid(ValidationIssue::error(
                    "route.bound_mismatch",
                    "target.subsystemId",
                    "a bound connection can only target its own connector",
                )));
            }
        }
    }
    Ok(())
}

pub fn validate_seeded_defaults(
    connectors: &[SubsystemRecord],
    backup_engine: Option<&SubsystemRecord>,
) -> Result<(), RouteError> {
    let spec = ConnectorRoutesSpec::seeded();
    for row in &spec.defaults {
        validate_target(
            &row.family,
            row.use_kind,
            &row.target,
            None,
            connectors,
            backup_engine,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fd_s3() -> SubsystemRecord {
        SubsystemRecord {
            id: "fd-s3".into(),
            class: "BuiltIn".into(),
            manifest: json!({"cloud": [{"family": "s3-compatible", "capabilities": ["cloud.remote"], "conditionalWrites": true}]}),
            version: "0.1.0".into(),
            enabled: true,
            state: "Running".into(),
        }
    }

    fn rclone() -> SubsystemRecord {
        SubsystemRecord {
            id: "rclone".into(),
            class: "BuiltIn".into(),
            manifest: json!({"cloud": [
                {"family": "s3-compatible", "capabilities": ["cloud.remote", "cloud.mount"]},
                {"family": "azure-blob", "capabilities": ["cloud.remote", "cloud.mount"]},
                {"family": "google-drive", "capabilities": ["cloud.mount"]},
                {"family": "onedrive", "capabilities": ["cloud.mount"]},
                {"family": "dropbox", "capabilities": ["cloud.mount"]}
            ]}),
            version: "1.75.2".into(),
            enabled: true,
            state: "Running".into(),
        }
    }

    fn kopia() -> SubsystemRecord {
        SubsystemRecord {
            id: "kopia".into(),
            class: "BuiltIn".into(),
            manifest: json!({"cloud": [
                {"family": "s3-compatible", "capabilities": ["cloud.remote"]},
                {"family": "azure-blob", "capabilities": ["cloud.remote"]}
            ]}),
            version: "0.23.1".into(),
            enabled: true,
            state: "Running".into(),
        }
    }

    #[test]
    fn seeded_defaults_pass() {
        validate_seeded_defaults(&[fd_s3(), rclone()], Some(&kopia())).unwrap();
    }

    #[test]
    fn missing_route() {
        let spec = ConnectorRoutesSpec {
            defaults: vec![],
            acknowledgements: vec![],
        };
        assert!(matches!(
            resolve(&spec, None, "s3-compatible", RouteUse::CloudMount),
            Err(RouteError::Missing)
        ));
    }
}
