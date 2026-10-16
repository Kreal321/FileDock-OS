use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Config {
    pub socket: PathBuf,
    pub apps_root: PathBuf,
    pub state_root: PathBuf,
    pub extension_root: PathBuf,
    pub extension_inbox: PathBuf,
    pub extension_keys: PathBuf,
    pub state_spool: PathBuf,
    pub pools_root: PathBuf,
    pub core_db: PathBuf,
    pub secrets: PathBuf,
    pub recovery_keys: PathBuf,
    pub dek: PathBuf,
    pub manifests: PathBuf,
    pub secrets_run: PathBuf,
    pub interface_version: &'static str,
    pub release_version: &'static str,
    pub mgmt_uid: Option<u32>,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            apps_root: env_path("FD_APPS_ROOT", "/var/lib/filedock/apps"),
            extension_root: env_path("FD_EXTENSION_ROOT", "/var/lib/filedock/extensions"),
            extension_inbox: env_path("FD_EXTENSION_INBOX", "/var/lib/filedock/extensions/inbox"),
            extension_keys: env_path("FD_EXTENSION_KEYS", "/etc/filedock/extensions/trusted-keys"),
            state_root: env_path("FD_STATE_ROOT", "/var/backups/filedock/appliance-state"),
            state_spool: env_path("FD_STATE_SPOOL", "/var/lib/filedock-core/appstate"),
            pools_root: env_path("FD_POOLS_ROOT", "/mnt/filedock/pools"),
            socket: env_path("FD_CORE_SOCKET", "/run/filedock/core.sock"),
            core_db: env_path("FD_CORE_DB", "/var/lib/filedock-core/core.db"),
            secrets: env_path("FD_SECRETS", "/var/lib/filedock/secrets.enc"),
            recovery_keys: env_path("FD_RECOVERY_KEYS", "/var/lib/filedock/recovery-keys.enc"),
            dek: env_path("FD_DEK", "/var/lib/filedock/keys/local.key"),
            manifests: env_path("FD_MANIFESTS", "/usr/share/filedock/subsystems"),
            secrets_run: env_path("FD_SECRETS_RUN", "/run/filedock/secrets"),
            interface_version: "1.0",
            release_version: env!("CARGO_PKG_VERSION"),
            mgmt_uid: std::env::var("FD_MGMT_UID")
                .ok()
                .and_then(|s| s.parse().ok())
                .or_else(|| {
                    let name = std::ffi::CString::new("filedock-mgmt").ok()?;
                    let entry = unsafe { libc::getpwnam(name.as_ptr()) };
                    if entry.is_null() {
                        None
                    } else {
                        Some(unsafe { (*entry).pw_uid })
                    }
                }),
        }
    }
}

fn env_path(key: &str, default: &str) -> PathBuf {
    std::env::var_os(key)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(default))
}
