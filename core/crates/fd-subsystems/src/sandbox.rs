//! Closed permission vocabulary translated into systemd and Docker controls.
use crate::{
    LoadedManifest,
    package::{Result, invalid},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Binding {
    pub folder_id: String,
    pub source: String,
    pub protected: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Mount {
    pub source: String,
    pub target: String,
    pub folder_id: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Sandbox {
    pub bindings: Vec<Mount>,
    pub outbound: bool,
    pub listen: Vec<u16>,
    pub devices: Vec<String>,
    pub mount: bool,
}
impl Sandbox {
    pub fn derive(manifest: &LoadedManifest, bindings: &BTreeMap<String, Binding>) -> Result<Self> {
        let mut policy = Self {
            bindings: vec![],
            outbound: false,
            listen: vec![],
            devices: vec![],
            mount: false,
        };
        let mut targets = BTreeSet::new();
        let mut names = BTreeSet::new();
        for data in manifest.raw["data"].as_array().into_iter().flatten() {
            let name = data["name"]
                .as_str()
                .ok_or_else(|| invalid("extension.data_invalid"))?;
            let target = data["mountPath"]
                .as_str()
                .ok_or_else(|| invalid("extension.data_invalid"))?;
            if !names.insert(name.to_owned())
                || !targets.insert(target.to_owned())
                || !target.starts_with('/')
                || target == "/"
                || target.contains(['\n', '\r', '\0', ':', ' '])
                || target.split('/').any(|c| c == ".." || c == ".")
                || [
                    "/etc",
                    "/usr",
                    "/bin",
                    "/sbin",
                    "/lib",
                    "/lib64",
                    "/proc",
                    "/sys",
                    "/dev",
                    "/run",
                    "/tmp",
                    "/storage",
                    "/mnt",
                    "/home",
                    "/root",
                    "/var/lib/filedock",
                    "/var/lib/filedock-core",
                    "/var/lib/filedock-management",
                    "/var/backups/filedock",
                ]
                .iter()
                .any(|p| {
                    target == *p
                        || target.starts_with(&format!("{p}/"))
                        || p.starts_with(&format!("{target}/"))
                })
            {
                return Err(invalid("extension.data_path_invalid"));
            }
            let binding = bindings
                .get(name)
                .ok_or_else(|| invalid("extension.binding_required"))?;
            if !binding.source.starts_with("/storage/")
                || binding.source.contains(['\n', '\r', '\0', ':', ' '])
                || binding.source.split('/').any(|c| c == ".." || c == ".")
            {
                return Err(invalid("extension.binding_invalid"));
            }
            policy.bindings.push(Mount {
                source: binding.source.clone(),
                target: target.into(),
                folder_id: binding.folder_id.clone(),
            });
        }
        if names.len() != bindings.len() {
            return Err(invalid("extension.binding_unknown"));
        }
        for permission in manifest.raw["permissions"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            match permission {
                "network.outbound" => policy.outbound = true,
                "privileged.mount" => policy.mount = true,
                "core.events.read" | "core.alerts.write" | "cloud.credentials" => {}
                p if p.starts_with("network.listen:") => {
                    let port = p[15..]
                        .parse::<u16>()
                        .map_err(|_| invalid("extension.permission_invalid"))?;
                    if port == 0 {
                        return Err(invalid("extension.permission_invalid"));
                    }
                    policy.listen.push(port);
                }
                p if p.starts_with("device:") => {
                    policy.devices.push(format!("/dev/disk/by-id/{}", &p[7..]))
                }
                p if p.starts_with("secrets:") => {}
                p if p.starts_with("storage.read:") || p.starts_with("storage.write:") => {
                    let target = p.split_once(':').unwrap().1;
                    if target != "*"
                        && !names.contains(target)
                        && !bindings.values().any(|b| b.folder_id == target)
                    {
                        return Err(invalid("extension.permission_unbound_storage"));
                    }
                }
                _ => return Err(invalid("extension.permission_invalid")),
            }
        }
        Ok(policy)
    }
    pub fn systemd(&self, user: &str, runtime: &str) -> BTreeMap<String, String> {
        let mut props = BTreeMap::from([
            ("User".into(), user.into()),
            ("NoNewPrivileges".into(), "yes".into()),
            ("ProtectSystem".into(), "strict".into()),
            ("ProtectHome".into(), "yes".into()),
            ("PrivateTmp".into(), "yes".into()),
            (
                "PrivateDevices".into(),
                if self.devices.is_empty() { "yes" } else { "no" }.into(),
            ),
            ("ProtectKernelTunables".into(), "yes".into()),
            ("ProtectKernelModules".into(), "yes".into()),
            ("ProtectControlGroups".into(), "yes".into()),
            ("RestrictSUIDSGID".into(), "yes".into()),
            ("LockPersonality".into(), "yes".into()),
            ("ProtectProc".into(), "invisible".into()),
            (
                "RestrictAddressFamilies".into(),
                if self.outbound {
                    "AF_UNIX AF_INET AF_INET6"
                } else {
                    "AF_UNIX"
                }
                .into(),
            ),
            (
                "CapabilityBoundingSet".into(),
                if self.mount { "CAP_SYS_ADMIN" } else { "" }.into(),
            ),
            (
                "AmbientCapabilities".into(),
                if self.mount { "CAP_SYS_ADMIN" } else { "" }.into(),
            ),
            ("DevicePolicy".into(), "closed".into()),
            ("SocketBindDeny".into(), "any".into()),
            ("Restart".into(), "no".into()),
            (
                "RuntimeDirectory".into(),
                runtime.trim_start_matches("/run/").into(),
            ),
            ("RuntimeDirectoryMode".into(), "0700".into()),
            (
                "ReadWritePaths".into(),
                self.bindings
                    .iter()
                    .map(|b| b.target.as_str())
                    .chain(std::iter::once(runtime))
                    .collect::<Vec<_>>()
                    .join(" "),
            ),
        ]);
        props.insert("InaccessiblePaths".into(),"-/var/lib/filedock/secrets.enc -/var/lib/filedock/keys -/var/lib/filedock/recovery-keys.enc -/var/lib/filedock/extensions/packages -/var/lib/filedock-core -/var/lib/filedock-management -/var/backups/filedock -/storage -/mnt/filedock/pools -/run/filedock/core.sock -/run/docker.sock -/etc/filedock".into());
        props.insert(
            "BindPaths".into(),
            self.bindings
                .iter()
                .map(|b| format!("{}:{}", b.source, b.target))
                .collect::<Vec<_>>()
                .join(" "),
        );
        if !self.devices.is_empty() {
            props.insert(
                "DeviceAllow".into(),
                self.devices
                    .iter()
                    .map(|d| format!("{d} rw"))
                    .collect::<Vec<_>>()
                    .join("|"),
            );
        }
        if !self.listen.is_empty() {
            props.insert(
                "SocketBindAllow".into(),
                self.listen
                    .iter()
                    .flat_map(|p| {
                        [
                            format!("ipv4:tcp:{p}"),
                            format!("ipv6:tcp:{p}"),
                            format!("ipv4:udp:{p}"),
                            format!("ipv6:udp:{p}"),
                        ]
                    })
                    .collect::<Vec<_>>()
                    .join("|"),
            );
        }
        props
    }
    pub fn docker(&self, user: &str, runtime: &str, bridge: &str, _token: &str) -> Result<Value> {
        let mut mounts = self
            .bindings
            .iter()
            .map(|b| json!({"Type":"bind","Source":b.source,"Target":b.target,"ReadOnly":false}))
            .collect::<Vec<_>>();
        mounts.push(json!({"Type":"bind","Source":runtime,"Target":runtime,"ReadOnly":false}));
        mounts.push(json!({"Type":"bind","Source":bridge,"Target":"/run/filedock/extensions.sock","ReadOnly":true}));
        let bindings = self
            .listen
            .iter()
            .map(|p| {
                (
                    format!("{p}/tcp"),
                    json!([{"HostIp":"0.0.0.0","HostPort":p.to_string()}]),
                )
            })
            .collect::<serde_json::Map<_, _>>();
        Ok(
            json!({"User":user,"Env":[format!("FD_EXTENSION_TOKEN_FILE={runtime}/token"),"FD_CORE_SOCKET=/run/filedock/extensions.sock",format!("FD_EXTENSION_SOCKET={runtime}/subsystem.sock")],"HostConfig":{"ReadonlyRootfs":true,"CapDrop":["ALL"],"CapAdd":if self.mount{vec!["SYS_ADMIN"]}else{vec![]},"SecurityOpt":["no-new-privileges:true"],"NetworkMode":if self.outbound{"bridge"}else{"none"},"Mounts":mounts,"Tmpfs":{"/tmp":"rw,nosuid,nodev,size=64m"},"PortBindings":bindings,"Devices":self.devices.iter().map(|d|json!({"PathOnHost":d,"PathInContainer":d,"CgroupPermissions":"rw"})).collect::<Vec<_>>(),"RestartPolicy":{"Name":"no"}}}),
        )
    }
}
