//! Drops values of known secret fields (FR-029, SC-007).

use serde_json::{Map, Value};

const SECRET_KEYS: &[&str] = &[
    "password",
    "passwordhash",
    "password_hash",
    "secret",
    "secretkey",
    "secret_key",
    "secretkeyref",
    "accesskey",
    "access_key",
    "token",
    "apikey",
    "api_key",
    "authorization",
    "recoverykey",
    "recovery_key",
    "dek",
    "wrappeddek",
    "ciphertext",
    "nonce",
    "credential",
    "credentials",
    "privatekey",
    "private_key",
];

fn is_secret_key(key: &str) -> bool {
    let compact: String = key
        .chars()
        .filter(|c| *c != '-' && *c != '_')
        .flat_map(char::to_lowercase)
        .collect();
    SECRET_KEYS
        .iter()
        .any(|known| compact.contains(&known.chars().filter(|c| *c != '_').collect::<String>()))
        || compact.contains("secret")
        || compact.contains("password")
        || compact.contains("token")
        || compact.ends_with("key") && !compact.ends_with("keyid") && compact != "key"
}

/// Replaces secret-looking fields with a mask. Structure is kept so logs stay useful.
pub fn redact_value(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut out = Map::new();
            for (key, child) in map {
                out.insert(
                    key.clone(),
                    if is_secret_key(&key) {
                        Value::String("••••".into())
                    } else {
                        redact_value(child)
                    },
                );
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.into_iter().map(redact_value).collect()),
        other => other,
    }
}

pub fn redact_text(text: &str) -> String {
    match serde_json::from_str::<Value>(text) {
        Ok(value) => redact_value(value).to_string(),
        Err(_) => SECRET_KEYS.iter().fold(text.to_owned(), |acc, key| {
            acc.replace(&format!("{key}="), &format!("{key}=••••"))
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn masks_known_fields_and_keeps_others() {
        let redacted = redact_value(json!({
            "username": "alice",
            "password": "hunter2",
            "nested": {"secretKey": "abc", "region": "us"}
        }));
        assert_eq!(redacted["username"], "alice");
        assert_eq!(redacted["password"], "••••");
        assert_eq!(redacted["nested"]["secretKey"], "••••");
        assert_eq!(redacted["nested"]["region"], "us");
    }
}
