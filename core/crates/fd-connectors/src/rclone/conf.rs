//! tmpfs rclone.conf listing only connections routed to rclone (FR-119).

use fd_state::resources::{
    AddressingStyle, CloudConnectionSpec, ConnectionDefinition, S3Definition,
};

#[derive(Clone, Debug)]
pub struct RcloneRemote {
    pub name: String,
    pub lines: Vec<(String, String)>,
}

/// Neutral S3 → rclone `s3` remote (`provider=Other`, `env_auth=false`).
pub fn s3_remote(name: &str, s3: &S3Definition) -> RcloneRemote {
    let mut lines = vec![
        ("type".into(), "s3".into()),
        ("provider".into(), "Other".into()),
        ("env_auth".into(), "false".into()),
        ("endpoint".into(), s3.endpoint.clone()),
        ("region".into(), s3.region.clone()),
        (
            "force_path_style".into(),
            (s3.addressing_style == AddressingStyle::Path).to_string(),
        ),
    ];
    if let Some(class) = &s3.storage_class {
        lines.push(("storage_class".into(), class.clone()));
    }
    RcloneRemote {
        name: name.into(),
        lines,
    }
}

pub fn render_conf(remotes: &[RcloneRemote]) -> String {
    let mut out = String::new();
    for remote in remotes {
        out.push_str(&format!("[{}]\n", remote.name));
        for (k, v) in &remote.lines {
            out.push_str(&format!("{k} = {v}\n"));
        }
        out.push('\n');
    }
    out
}

pub fn remotes_for(connections: &[(String, CloudConnectionSpec)]) -> Vec<RcloneRemote> {
    connections
        .iter()
        .map(|(id, spec)| match &spec.definition {
            ConnectionDefinition::S3(s3) => s3_remote(id, s3),
            ConnectionDefinition::Bound(bound) => {
                let backend = match spec.provider_family.as_str() {
                    "google-drive" => "drive",
                    "azure-blob" => "azureblob",
                    other => other,
                };
                let mut lines = vec![("type".into(), backend.to_string())];
                if let Some(config) = bound.config.as_object() {
                    for (key, value) in config {
                        if let Some(value) = value.as_str() {
                            lines.push((key.clone(), value.to_string()));
                        }
                    }
                }
                RcloneRemote {
                    name: id.clone(),
                    lines,
                }
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn s3_translation() {
        let spec: CloudConnectionSpec = serde_json::from_value(json!({
            "name": "wasabi",
            "providerFamily": "s3-compatible",
            "definition": {
                "type": "s3",
                "endpoint": "https://s3.wasabisys.com",
                "region": "us-east-1",
                "addressingStyle": "Path",
                "bucket": "b",
                "accessKeyRef": "connection/1/access-key",
                "secretKeyRef": "connection/1/secret-key"
            }
        }))
        .unwrap();
        let ConnectionDefinition::S3(s3) = &spec.definition else {
            panic!()
        };
        let conf = render_conf(&[s3_remote("c1", s3)]);
        assert!(conf.contains("provider = Other"));
        assert!(conf.contains("env_auth = false"));
        assert!(conf.contains("force_path_style = true"));
        assert!(!conf.contains("crypt"));
        assert!(!conf.contains("chunker"));
    }
}
