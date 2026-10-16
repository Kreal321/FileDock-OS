use crate::api::App;
use serde_json::Value;
use std::path::{Component, Path};
use tonic::Status;
pub fn schema(
    app: &App,
    subsystem: &fd_subsystems::SubsystemRecord,
    family: &str,
) -> Option<Value> {
    let entry = subsystem.manifest["cloud"]
        .as_array()?
        .iter()
        .find(|f| f["family"] == family)?;
    let relative = Path::new(entry["connectionSchema"].as_str()?);
    if relative
        .components()
        .any(|c| !matches!(c, Component::Normal(_)))
    {
        return None;
    }
    let root = if subsystem.class == "BuiltIn" {
        app.config.manifests.clone()
    } else {
        let row = crate::storage_runtime::rows(app, fd_state::ResourceKind::Extension)
            .ok()?
            .into_iter()
            .find(|r| r.spec["subsystemId"] == subsystem.id)?;
        app.config
            .extension_root
            .join("runtimes")
            .join(row.spec["packageSha256"].as_str()?)
    };
    std::fs::read(root.join(relative))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
}
pub fn validate(app: &App, spec: &Value) -> Result<(), Status> {
    let family = spec["providerFamily"].as_str().unwrap_or("");
    let connectors = app
        .subsystems
        .connectors()
        .map_err(|_| Status::internal("connection.registry_failed"))?;
    let served = connectors
        .iter()
        .filter(|s| {
            s.enabled
                && s.manifest["cloud"]
                    .as_array()
                    .is_some_and(|a| a.iter().any(|f| f["family"] == family))
        })
        .collect::<Vec<_>>();
    if served.is_empty() {
        return Err(Status::failed_precondition("connection.family_unserved"));
    }
    let def = &spec["definition"];
    if def.get("sessionToken").is_some() || def.get("roleArn").is_some() {
        return Err(Status::failed_precondition(
            "connection.credential_type_unsupported",
        ));
    }
    if def["type"] == "bound" {
        let owner = served
            .into_iter()
            .find(|s| def["connectorId"] == s.id)
            .ok_or_else(|| Status::failed_precondition("connection.connector_mismatch"))?;
        let config = &def["config"];
        if contains_secret(config) {
            return Err(Status::failed_precondition(
                "connection.credentials_require_vault",
            ));
        }
        let schema = schema(app, owner, family)
            .ok_or_else(|| Status::failed_precondition("connection.schema_missing"))?;
        let validator = jsonschema::validator_for(&schema)
            .map_err(|_| Status::internal("connection.schema_invalid"))?;
        if !validator.is_valid(config) {
            return Err(Status::failed_precondition(
                "connection.schema_validation_failed",
            ));
        }
    }
    Ok(())
}
fn contains_secret(value: &Value) -> bool {
    match value {
        Value::Object(fields) => fields.iter().any(|(k, v)| {
            [
                "token",
                "client_secret",
                "password",
                "secret",
                "account_key",
                "sas_url",
                "type",
            ]
            .contains(&k.as_str())
                || k.contains(['\n', '\r', '='])
                || contains_secret(v)
        }),
        Value::Array(values) => values.iter().any(contains_secret),
        Value::String(s) => s.contains(['\n', '\r']),
        _ => false,
    }
}
