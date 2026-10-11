use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RoutedConnection {
    pub id: String,
    pub endpoint: String,
    pub region: String,
    pub bucket: String,
    pub virtual_hosted: bool,
    pub access_key_file: PathBuf,
    pub secret_key_file: PathBuf,
    pub checksums: bool,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub socket: PathBuf,
    pub core_socket: PathBuf,
    pub secrets_dir: PathBuf,
    pub connections: BTreeMap<String, RoutedConnection>,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            socket: PathBuf::from(
                std::env::var_os("FD_S3_SOCKET")
                    .unwrap_or_else(|| "/run/filedock/fd-s3.sock".into()),
            ),
            core_socket: PathBuf::from(
                std::env::var_os("FD_CORE_SOCKET")
                    .unwrap_or_else(|| "/run/filedock/core.sock".into()),
            ),
            secrets_dir: PathBuf::from(
                std::env::var_os("FD_SECRETS_RUN")
                    .unwrap_or_else(|| "/run/filedock/secrets/fd-s3".into()),
            ),
            connections: BTreeMap::new(),
        }
    }
}

pub fn read_secret(path: &std::path::Path) -> std::io::Result<String> {
    Ok(std::fs::read_to_string(path)?.trim().to_owned())
}
