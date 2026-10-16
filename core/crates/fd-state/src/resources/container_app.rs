use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::check_secret_ref;
use crate::ValidationIssue;

/// A Compose application. The Compose project is `fd-<name>` (FR-085).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContainerAppSpec {
    pub name: String,
    pub compose: String,
    #[serde(default)]
    pub environment: BTreeMap<String, String>,
    #[serde(default)]
    pub secrets: Vec<String>,
}

impl ContainerAppSpec {
    pub(crate) fn validate(&self, issues: &mut Vec<ValidationIssue>) {
        // Compose project names: lowercase letters, digits, '-', '_', starting with a letter or digit.
        let name = self.name.as_bytes();
        let valid = !name.is_empty()
            && name.len() <= 60
            && name[0].is_ascii_alphanumeric()
            && name
                .iter()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"-_".contains(b));
        if !valid {
            issues.push(ValidationIssue::error(
                "container.name_invalid",
                "name",
                "must be lowercase letters, digits, '-', or '_' (at most 60)",
            ));
        }
        if self.compose.trim().is_empty() {
            issues.push(ValidationIssue::error(
                "container.compose_empty",
                "compose",
                "a Compose definition is required",
            ));
        }
        match serde_saphyr::from_str::<serde_json::Value>(&self.compose) {
            Ok(document) if document["services"].as_object().is_some_and(|s|!s.is_empty()) && document.get("include").is_none() && document["services"].as_object().is_none_or(|s|s.values().all(|s|s.get("extends").is_none())) && document.get("secrets").is_none_or(|s|s.is_object()) => {},
            _=>issues.push(ValidationIssue::error("app.compose_invalid","compose","Use a self-contained Compose services object without include/extends; secrets must be an object")),
        }
        for key in self.environment.keys() {
            let valid = key
                .bytes()
                .next()
                .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
                && key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_');
            if !valid {
                issues.push(ValidationIssue::error(
                    "container.env_name_invalid",
                    &format!("environment.{key}"),
                    "environment names are letters, digits, and '_'",
                ));
            }
        }
        for (name, value) in &self.environment {
            if value.contains(['\n', '\r', '\0']) {
                issues.push(ValidationIssue::error(
                    "container.env_value_invalid",
                    &format!("environment.{name}"),
                    "Environment values must be single-line",
                ));
            }
        }
        for (index, reference) in self.secrets.iter().enumerate() {
            check_secret_ref(&format!("secrets[{index}]"), reference, issues);
        }
    }
}
