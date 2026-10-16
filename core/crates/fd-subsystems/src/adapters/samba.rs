use super::{HealthOnly, systemd_health};
use crate::adapter::Health;

#[derive(Default)]
pub struct SambaAdapter;

impl HealthOnly for SambaAdapter {
    const ID: &'static str = "samba";
    const NAME: &'static str = "Samba";
    const VERSION: &'static str = "4.22.11";
    const CAPABILITIES: &'static [&'static str] = &["protocol.smb"];

    fn probe(&self) -> Health {
        systemd_health("smbd.service")
    }
}

/// Render only validated names and absolute folder paths. Empty ACL means no access.
pub fn render(
    exports: &[(String, String, Vec<fd_state::resources::ExportRule>)],
) -> Result<String, String> {
    let mut out = String::from("# FileDock managed shares\n");
    for (id, path, rules) in exports {
        if !safe(id) || !path.starts_with("/storage/") || path.contains(['\n', '\r', '"', '%']) {
            return Err("export.path_invalid".into());
        }
        let mut users = Vec::new();
        let mut write = Vec::new();
        for rule in rules {
            let p = rule
                .principal
                .as_deref()
                .ok_or("export.principal_missing")?;
            let (prefix, name) = p.split_once(':').ok_or("export.principal_invalid")?;
            if !safe(name) || !matches!(prefix, "user" | "group") {
                return Err("export.principal_invalid".into());
            }
            let name = format!("{}{}", if prefix == "group" { "@" } else { "" }, name);
            users.push(name.clone());
            if rule.access == "read-write" {
                write.push(name)
            }
        }
        if users.is_empty() {
            continue;
        }
        out.push_str(&format!("\n[{id}]\npath = {path}\nvalid users = {}\nwrite list = {}\nread only = yes\nguest ok = no\ninherit acls = yes\n",users.join(" "),write.join(" ")));
    }
    Ok(out)
}
fn safe(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
}

#[cfg(test)]
mod rendering_tests {
    use super::*;
    use fd_state::resources::ExportRule;
    #[test]
    fn rules_keep_read_and_write_lists_distinct_and_deny_empty_exports() {
        let rules = vec![
            ExportRule {
                principal: Some("user:alice".into()),
                client: None,
                access: "read".into(),
            },
            ExportRule {
                principal: Some("user:bob".into()),
                client: None,
                access: "read-write".into(),
            },
        ];
        let text = render(&[("share-id".into(), "/storage/data".into(), rules)]).unwrap();
        assert!(text.contains("valid users = alice bob"));
        assert!(text.contains("write list = bob\n"));
        assert!(text.contains("guest ok = no"));
        assert!(
            !render(&[("empty".into(), "/storage/empty".into(), vec![])])
                .unwrap()
                .contains("[empty]")
        );
    }
    #[test]
    fn configuration_injection_is_refused() {
        assert!(render(&[("bad\n[global]".into(), "/storage/data".into(), vec![])]).is_err());
    }
}
