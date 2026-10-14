use crate::{Result, invalid};
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotEntry {
    pub snapshot_id: String,
    pub taken_at: String,
    pub file: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub key_id: String,
    pub retention_class: String,
    pub packages: Vec<String>,
    pub release_version: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Index {
    pub schema_version: u32,
    pub appliance_id: String,
    pub appliance_name: String,
    pub signing_key_id: String,
    pub snapshots: Vec<SnapshotEntry>,
    pub mac: String,
}
impl Index {
    pub fn new(id: &str, name: &str) -> Self {
        Self {
            schema_version: 1,
            appliance_id: id.into(),
            appliance_name: name.into(),
            signing_key_id: String::new(),
            snapshots: vec![],
            mac: String::new(),
        }
    }
    fn bytes(&self) -> Result<Vec<u8>> {
        crate::canonical(
            &serde_json::json!({"applianceId":self.appliance_id,"applianceName":self.appliance_name,"signingKeyId":self.signing_key_id,"snapshots":self.snapshots}),
        )
    }
    fn hmac(&self, key: &[u8; 32]) -> Result<Hmac<Sha256>> {
        let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(key)
            .map_err(|_| invalid("state.mac_invalid"))?;
        mac.update(b"FileDock-index-v1\0");
        mac.update(&self.bytes()?);
        Ok(mac)
    }
    pub fn sign(&mut self, key: &[u8; 32]) -> Result<()> {
        self.mac = hex::encode(self.hmac(key)?.finalize().into_bytes());
        Ok(())
    }
    pub fn verify(&self, key: &[u8; 32]) -> Result<()> {
        if self.schema_version != 1
            || uuid::Uuid::parse_str(&self.appliance_id).is_err()
            || self.snapshots.len() > 256
            || self.signing_key_id.len() != 16
            || !self.signing_key_id.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(invalid("state.index_invalid"));
        }
        for s in &self.snapshots {
            crate::safe_relative(&s.file)?;
            if !s.file.starts_with("snapshots/")
                || !s.file.ends_with(".fdstate")
                || s.sha256.len() != 64
                || s.key_id.len() != 16
                || !s.key_id.bytes().all(|b| b.is_ascii_hexdigit())
                || !s.sha256.bytes().all(|b| b.is_ascii_hexdigit())
                || uuid::Uuid::parse_str(&s.snapshot_id).is_err()
                || chrono::DateTime::parse_from_rfc3339(&s.taken_at).is_err()
                || s.size_bytes > 128 * 1024 * 1024
                || s.packages
                    .iter()
                    .any(|p| p.len() != 64 || !p.bytes().all(|b| b.is_ascii_hexdigit()))
            {
                return Err(invalid("state.index_invalid"));
            }
        }
        self.hmac(key)?
            .verify_slice(&hex::decode(&self.mac).map_err(|_| invalid("state.mac_invalid"))?)
            .map_err(|_| invalid("recovery.key_invalid"))
    }
}
