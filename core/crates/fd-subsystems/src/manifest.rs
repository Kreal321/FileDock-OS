use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

const SCHEMA: &str = include_str!(
    "../../../../specs/001-core-os-foundation/contracts/subsystem-manifest.schema.json"
);

#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Invalid(String),
    #[error("yaml: {0}")]
    Yaml(String),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadedManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub class: String,
    pub capabilities: Vec<String>,
    pub raw: Value,
}

impl LoadedManifest {
    pub fn is_connector(&self) -> bool {
        self.raw
            .get("cloud")
            .and_then(Value::as_array)
            .is_some_and(|c| !c.is_empty())
    }

    pub fn families(&self) -> Vec<Value> {
        self.raw
            .get("cloud")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    }

    pub fn declares_family(&self, family: &str, capability: &str) -> bool {
        self.families().iter().any(|entry| {
            entry.get("family").and_then(Value::as_str) == Some(family)
                && entry
                    .get("capabilities")
                    .and_then(Value::as_array)
                    .is_some_and(|caps| caps.iter().any(|c| c.as_str() == Some(capability)))
        })
    }

    pub fn backup_families(&self) -> Vec<String> {
        self.families()
            .iter()
            .filter_map(|entry| entry.get("family")?.as_str().map(str::to_owned))
            .collect()
    }

    pub fn connection_schema(&self, family: &str) -> Option<String> {
        self.raw
            .get("cloud")
            .and_then(Value::as_array)
            .and_then(|rows| {
                rows.iter().find_map(|entry| {
                    (entry.get("family")?.as_str() == Some(family))
                        .then(|| entry.get("connectionSchema")?.as_str().map(str::to_owned))
                        .flatten()
                })
            })
    }

    pub fn conditional_writes(&self, family: &str) -> bool {
        self.families().iter().any(|entry| {
            entry.get("family").and_then(Value::as_str) == Some(family)
                && entry
                    .get("conditionalWrites")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
        })
    }
}

pub fn validate_manifest(value: &Value) -> Result<(), ManifestError> {
    let schema: Value = serde_json::from_str(SCHEMA).expect("embedded schema");
    let validator =
        jsonschema::validator_for(&schema).map_err(|e| ManifestError::Invalid(e.to_string()))?;
    let errors: Vec<String> = validator
        .iter_errors(value)
        .map(|e| e.to_string())
        .collect();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(ManifestError::Invalid(errors.join("; ")))
    }
}

pub fn parse_manifest(raw: &str) -> Result<LoadedManifest, ManifestError> {
    let value: Value =
        serde_saphyr::from_str(raw).map_err(|e| ManifestError::Yaml(e.to_string()))?;
    validate_manifest(&value)?;
    Ok(LoadedManifest {
        id: required_str(&value, "id")?,
        name: required_str(&value, "name")?,
        version: required_str(&value, "version")?,
        class: required_str(&value, "class")?,
        capabilities: value
            .get("capabilities")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default(),
        raw: value,
    })
}

pub fn load_dir(dir: impl AsRef<Path>) -> Result<Vec<(PathBuf, LoadedManifest)>, ManifestError> {
    let mut out = Vec::new();
    let dir = dir.as_ref();
    if !dir.exists() {
        return Ok(out);
    }
    let mut entries: Vec<_> = fs::read_dir(dir)?.filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.path());
    for entry in entries {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("yaml") {
            continue;
        }
        out.push((path.clone(), parse_manifest(&fs::read_to_string(&path)?)?));
    }
    Ok(out)
}

fn required_str(value: &Value, key: &str) -> Result<String, ManifestError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| ManifestError::Invalid(format!("missing {key}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_s3_fixture_loads() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../tests/fixtures/manifests/valid/s3-connector.yaml");
        let loaded = parse_manifest(&fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(loaded.id, "fd-s3");
        assert!(loaded.declares_family("s3-compatible", "cloud.remote"));
        assert!(loaded.conditional_writes("s3-compatible"));
    }

    #[test]
    fn connector_without_cloud_fails() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../tests/fixtures/manifests/invalid/connector-without-cloud.yaml");
        assert!(parse_manifest(&fs::read_to_string(path).unwrap()).is_err());
    }
}
