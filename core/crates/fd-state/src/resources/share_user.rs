use serde::{Deserialize, Serialize};

use super::check_secret_ref;
use crate::ValidationIssue;

/// A share user becomes a Linux system user with no shell. It can't sign in to the management UI.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ShareUserSpec {
    pub username: String,
    pub password_secret: String,
    #[serde(default = "enabled_by_default")]
    pub enabled: bool,
}

fn enabled_by_default() -> bool {
    true
}

impl ShareUserSpec {
    pub(crate) fn validate(&self, issues: &mut Vec<ValidationIssue>) {
        // Linux user names: start with a letter, then letters, digits, '-', '_'.
        let name = self.username.as_bytes();
        let valid = !name.is_empty()
            && name.len() <= 32
            && name[0].is_ascii_lowercase()
            && name
                .iter()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"-_".contains(b));
        if !valid {
            issues.push(ValidationIssue::error(
                "share_user.name_invalid",
                "username",
                "must start with a-z and use only a-z, 0-9, '-', and '_' (at most 32)",
            ));
        }
        check_secret_ref("passwordSecret", &self.password_secret, issues);
    }
}
