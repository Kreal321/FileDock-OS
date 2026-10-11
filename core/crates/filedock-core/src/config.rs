use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Config {
    pub socket: PathBuf,
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
                .and_then(|s| s.parse().ok()),
        }
    }
}

fn env_path(key: &str, default: &str) -> PathBuf {
    std::env::var_os(key)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(default))
}
