use serde::{Deserialize, Serialize};

use super::is_uuid;
use crate::ValidationIssue;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExportProtocol {
    Smb,
    Nfs,
    WebDav,
}

/// One access rule. SMB and WebDAV rules name a share user or group (`user:<name>` or
/// `group:<name>`) with `read` or `read-write`. NFS rules name a client host or CIDR with `ro` or
/// `rw`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExportRule {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub principal: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client: Option<String>,
    pub access: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExportSpec {
    pub folder_id: String,
    pub protocol: ExportProtocol,
    #[serde(default)]
    pub rules: Vec<ExportRule>,
}

fn is_valid_nfs_client(client: &str) -> bool {
    let (host, prefix) = match client.split_once('/') {
        Some((host, prefix)) => (host, Some(prefix)),
        None => (client, None),
    };
    let address = host.parse::<std::net::IpAddr>();
    match (address, prefix) {
        (Ok(ip), Some(bits)) => bits
            .parse::<u8>()
            .is_ok_and(|bits| bits <= if ip.is_ipv4() { 32 } else { 128 }),
        (Ok(_), None) => true,
        (Err(_), None) => {
            !host.is_empty()
                && host.len() <= 253
                && host.split('.').all(|label| {
                    !label.is_empty()
                        && label.len() <= 63
                        && label
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'*')
                })
        }
        (Err(_), Some(_)) => false,
    }
}

impl ExportSpec {
    pub(crate) fn validate(&self, issues: &mut Vec<ValidationIssue>) {
        if !is_uuid(&self.folder_id) {
            issues.push(ValidationIssue::error(
                "export.folder_invalid",
                "folderId",
                "must be a folder id",
            ));
        }
        for (index, rule) in self.rules.iter().enumerate() {
            let field = format!("rules[{index}]");
            match self.protocol {
                ExportProtocol::Nfs => {
                    let client_ok = rule.client.as_deref().is_some_and(is_valid_nfs_client);
                    if !client_ok || rule.principal.is_some() {
                        issues.push(ValidationIssue::error(
                            "export.rule_invalid",
                            &field,
                            "NFS rules name a client host or CIDR",
                        ));
                    }
                    if !matches!(rule.access.as_str(), "ro" | "rw") {
                        issues.push(ValidationIssue::error(
                            "export.rule_invalid",
                            &format!("{field}.access"),
                            "NFS access is 'ro' or 'rw'",
                        ));
                    }
                }
                ExportProtocol::Smb | ExportProtocol::WebDav => {
                    let principal_ok = rule.principal.as_deref().is_some_and(|p| {
                        p.strip_prefix("user:")
                            .or_else(|| p.strip_prefix("group:"))
                            .is_some_and(|name| {
                                !name.is_empty()
                                    && name.len() <= 32
                                    && name
                                        .bytes()
                                        .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
                            })
                    });
                    if !principal_ok || rule.client.is_some() {
                        issues.push(ValidationIssue::error(
                            "export.rule_invalid",
                            &field,
                            "rules name 'user:<name>' or 'group:<name>'",
                        ));
                    }
                    if !matches!(rule.access.as_str(), "read" | "read-write") {
                        issues.push(ValidationIssue::error(
                            "export.rule_invalid",
                            &format!("{field}.access"),
                            "access is 'read' or 'read-write'",
                        ));
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nfs_clients() {
        assert!(is_valid_nfs_client("10.0.0.0/24"));
        assert!(is_valid_nfs_client("fd00::/64"));
        assert!(is_valid_nfs_client("192.168.1.5"));
        assert!(is_valid_nfs_client("*.lan"));
        assert!(!is_valid_nfs_client("10.0.0.0/33"));
        assert!(!is_valid_nfs_client("host name"));
        assert!(!is_valid_nfs_client("host/24"));
    }
}
