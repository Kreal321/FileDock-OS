use super::{HealthOnly, systemd_health};
use crate::adapter::Health;

#[derive(Default)]
pub struct NfsAdapter;

impl HealthOnly for NfsAdapter {
    const ID: &'static str = "nfs";
    const NAME: &'static str = "NFS";
    const VERSION: &'static str = "2.8.3";
    const CAPABILITIES: &'static [&'static str] = &["protocol.nfs"];

    fn probe(&self) -> Health {
        systemd_health("nfs-server.service")
    }
}

pub fn render(
    exports: &[(String, Vec<fd_state::resources::ExportRule>)],
) -> Result<String, String> {
    let mut out = String::from("# FileDock NFSv4 sec=sys; root squash enforced\n");
    for (path, rules) in exports {
        if !path.starts_with("/storage/")
            || path
                .chars()
                .any(|c| c.is_control() || matches!(c, '"' | '\\'))
        {
            return Err("export.path_invalid".into());
        }
        for rule in rules {
            let client = rule.client.as_deref().ok_or("export.client_required")?;
            if client
                .bytes()
                .any(|b| !b.is_ascii_alphanumeric() && !b".-_:/*".contains(&b))
            {
                return Err("export.client_invalid".into());
            }
            if !matches!(rule.access.as_str(), "ro" | "rw") {
                return Err("export.access_invalid".into());
            }
            out.push_str(&format!(
                "\"{path}\" {client}({},sync,root_squash,no_subtree_check,sec=sys)\n",
                rule.access
            ));
        }
    }
    Ok(out)
}
