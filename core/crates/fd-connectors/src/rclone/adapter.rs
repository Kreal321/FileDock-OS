//! CloudRemote over rclone's remote-control API. `conditionalWrites: false`.

use std::path::PathBuf;

use reqwest::Client;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::rclone::rcd::SOCKET;

pub struct RcloneAdapter {
    pub socket: PathBuf,
    http: Client,
}

impl RcloneAdapter {
    pub fn new() -> Self {
        Self::with_socket(PathBuf::from(SOCKET))
    }

    pub fn with_socket(socket: PathBuf) -> Self {
        Self {
            socket: socket.clone(),
            http: Client::builder()
                .unix_socket(socket)
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("rclone client"),
        }
    }

    pub async fn health(&self) -> Result<(), String> {
        self.call("core/stats", json!({})).await.map(|_| ())
    }

    pub fn conditional_writes(&self) -> bool {
        false
    }

    pub(crate) async fn call(&self, method: &str, body: Value) -> Result<Value, String> {
        let response = self
            .http
            .post(format!("http://localhost/{method}"))
            .json(&body)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let status = response.status();
        let value: Value = response.json().await.map_err(|e| e.to_string())?;
        if !status.is_success() {
            return Err(format!(
                "rclone {status}: {}",
                value
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("request failed")
            ));
        }
        Ok(value)
    }

    pub async fn create_azure_recovery_remote(
        &self,
        name: &str,
        parameters: Value,
    ) -> Result<(), String> {
        self.call("config/create",json!({"name":name,"type":"azureblob","parameters":parameters,"opt":{"nonInteractive":true,"obscure":true,"noOutput":true}})).await?;
        Ok(())
    }
    pub async fn delete_recovery_remote(&self, name: &str) -> Result<(), String> {
        self.call("config/delete", json!({"name":name})).await?;
        Ok(())
    }
    pub async fn list(&self, remote: &str, prefix: &str) -> Result<Vec<String>, String> {
        let body = self
            .call(
                "operations/list",
                json!({"fs": filesystem(remote), "remote": prefix, "opt":{"recurse":true,"filesOnly":true}}),
            )
            .await?;
        Ok(body
            .get("list")
            .and_then(Value::as_array)
            .map(|rows| {
                rows.iter()
                    .filter_map(|row| row.get("Path")?.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default())
    }

    pub async fn stat(&self, remote: &str, key: &str) -> Result<Value, String> {
        self.call(
            "operations/stat",
            json!({"fs": filesystem(remote), "remote": key}),
        )
        .await
    }

    pub async fn copyfile(&self, src: &str, dst_fs: &str, dst: &str) -> Result<(), String> {
        self.call(
            "operations/copyfile",
            json!({"srcFs": "/", "srcRemote": src.trim_start_matches('/'), "dstFs": filesystem(dst_fs), "dstRemote": dst}),
        )
        .await?;
        Ok(())
    }

    pub async fn deletefile(&self, remote: &str, key: &str) -> Result<(), String> {
        self.call(
            "operations/deletefile",
            json!({"fs": filesystem(remote), "remote": key}),
        )
        .await?;
        Ok(())
    }

    pub async fn read(&self, remote: &str, key: &str) -> Result<Vec<u8>, String> {
        let directory = tempfile::tempdir_in(self.socket.parent().ok_or("invalid socket path")?)
            .map_err(|e| e.to_string())?;
        self.call("operations/copyfile", json!({"srcFs": filesystem(remote), "srcRemote": key, "dstFs": directory.path().to_string_lossy(), "dstRemote": "object"})).await?;
        std::fs::read(directory.path().join("object")).map_err(|e| e.to_string())
    }

    pub async fn write_verified(
        &self,
        remote: &str,
        key: &str,
        bytes: &[u8],
    ) -> Result<(), String> {
        let directory = tempfile::tempdir_in(self.socket.parent().ok_or("invalid socket path")?)
            .map_err(|e| e.to_string())?;
        let path = directory.path().join("object");
        std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
        self.copyfile(path.to_str().ok_or("invalid temporary path")?, remote, key)
            .await?;
        let actual = self.read(remote, key).await?;
        Self::verify(bytes, &actual)
    }

    /// Read-back verification for every successful write (no provider checksums).
    pub fn verify(expected: &[u8], actual: &[u8]) -> Result<(), String> {
        if Sha256::digest(expected) == Sha256::digest(actual) && expected == actual {
            Ok(())
        } else {
            Err("read-back mismatch".into())
        }
    }
}

impl Default for RcloneAdapter {
    fn default() -> Self {
        Self::new()
    }
}

fn filesystem(remote: &str) -> String {
    if remote.contains(':') {
        remote.into()
    } else {
        format!("{remote}:")
    }
}
