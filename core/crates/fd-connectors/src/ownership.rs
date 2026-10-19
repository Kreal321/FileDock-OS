//! Single-writer ownership checked before writes; recovery readers never mutate.
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
#[derive(Debug, thiserror::Error)]
pub enum OwnershipError {
    #[error("{0}")]
    Refused(String),
    #[error("connector.unavailable")]
    Unavailable,
}
pub type Result<T> = std::result::Result<T, OwnershipError>;
#[derive(Clone)]
pub struct ObjectData {
    pub bytes: Vec<u8>,
    pub etag: String,
}
#[derive(Clone, Debug)]
pub enum WriteCondition {
    Overwrite,
    CreateOnly,
    IfMatch(String),
}
#[async_trait]
pub trait ObjectIo: Send + Sync {
    async fn read(&self, key: &str) -> Result<Option<ObjectData>>;
    async fn list(&self, prefix: &str) -> Result<Vec<String>>;
    async fn put(&self, key: &str, bytes: &[u8], condition: WriteCondition) -> Result<String>;
    async fn delete(&self, key: &str) -> Result<()>;
    async fn abort_incomplete(
        &self,
        _prefix: &str,
        _hours: u32,
        _root_owner_checked: bool,
    ) -> Result<u32> {
        Err(OwnershipError::Refused(
            "connector.sweep_unsupported".into(),
        ))
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Owner {
    pub schema_version: u32,
    pub appliance_id: String,
    pub appliance_name: String,
    pub generation: u64,
    pub claimed_at: String,
    pub last_write_at: String,
    pub purpose: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ClaimSafety {
    Conditional,
    Basic,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    Writer,
    RecoveryReader,
    OwnershipLost,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnedLocation {
    pub id: String,
    pub connection_id: String,
    pub bucket: String,
    pub prefix: String,
    pub purpose: String,
    pub owner_appliance_id: String,
    pub generation: u64,
    pub last_write_at: String,
    pub role: Role,
    pub served_by_connector: String,
    pub served_by_third_party: bool,
    pub claim_safety: ClaimSafety,
}
fn refused(code: &str) -> OwnershipError {
    OwnershipError::Refused(code.into())
}
fn json(bytes: &[u8]) -> Result<Owner> {
    let owner: Owner =
        serde_json::from_slice(bytes).map_err(|_| refused("location.owner_invalid"))?;
    if owner.schema_version != 1
        || owner.generation == 0
        || uuid::Uuid::parse_str(&owner.appliance_id).is_err()
    {
        return Err(refused("location.owner_invalid"));
    }
    Ok(owner)
}
pub const OWNER_KEY: &str = "_filedock/owner.json";
pub async fn claim(
    io: &dyn ObjectIo,
    id: &str,
    name: &str,
    safety: ClaimSafety,
    acknowledged_basic: bool,
) -> Result<Owner> {
    if matches!(safety, ClaimSafety::Basic) && !acknowledged_basic {
        return Err(refused("location.basic_claim_ack_required"));
    }
    if let Some(existing) = io.read(OWNER_KEY).await? {
        let owner = json(&existing.bytes)?;
        if owner.appliance_id != id {
            return Err(refused("location.already_owned"));
        }
        if owner.purpose != "ApplianceState" {
            return Err(refused("location.purpose_mismatch"));
        }
        return Ok(owner);
    }
    if !io.list("").await?.is_empty() {
        return Err(refused("location.not_empty"));
    }
    let now = chrono::Utc::now().to_rfc3339();
    let owner = Owner {
        schema_version: 1,
        appliance_id: id.into(),
        appliance_name: name.into(),
        generation: 1,
        claimed_at: now.clone(),
        last_write_at: now,
        purpose: "ApplianceState".into(),
    };
    let bytes = serde_json::to_vec(&owner).map_err(|_| refused("location.owner_invalid"))?;
    io.put(
        OWNER_KEY,
        &bytes,
        if matches!(safety, ClaimSafety::Conditional) {
            WriteCondition::CreateOnly
        } else {
            WriteCondition::Overwrite
        },
    )
    .await?;
    let actual = io
        .read(OWNER_KEY)
        .await?
        .ok_or_else(|| refused("location.claim_unverified"))?;
    if actual.bytes != bytes {
        return Err(refused("location.claim_unverified"));
    }
    Ok(owner)
}
pub async fn takeover(
    io: &dyn ObjectIo,
    id: &str,
    name: &str,
    generation: u64,
    safety: &ClaimSafety,
    acknowledged_recent: bool,
    automatic: bool,
) -> Result<Owner> {
    let current = io
        .read(OWNER_KEY)
        .await?
        .ok_or_else(|| refused("location.owner_missing"))?;
    let old = json(&current.bytes)?;
    if old.generation != generation {
        return Err(refused("location.generation_mismatch"));
    }
    let last = chrono::DateTime::parse_from_rfc3339(&old.last_write_at)
        .map_err(|_| refused("location.owner_invalid"))?;
    let recent = chrono::Utc::now().signed_duration_since(last).num_seconds() <= 900;
    if recent && (automatic || !acknowledged_recent) {
        return Err(refused("location.recent_writer_ack_required"));
    }
    let now = chrono::Utc::now().to_rfc3339();
    let owner = Owner {
        schema_version: 1,
        appliance_id: id.into(),
        appliance_name: name.into(),
        generation: old
            .generation
            .checked_add(1)
            .ok_or_else(|| refused("location.generation_invalid"))?,
        claimed_at: now.clone(),
        last_write_at: now,
        purpose: old.purpose,
    };
    let bytes = serde_json::to_vec(&owner).map_err(|_| refused("location.owner_invalid"))?;
    io.put(
        OWNER_KEY,
        &bytes,
        if matches!(safety, ClaimSafety::Conditional) {
            WriteCondition::IfMatch(current.etag)
        } else {
            WriteCondition::Overwrite
        },
    )
    .await?;
    if io.read(OWNER_KEY).await?.is_none_or(|v| v.bytes != bytes) {
        return Err(refused("location.claim_unverified"));
    }
    Ok(owner)
}
pub struct WriterSession {
    pub io: Arc<dyn ObjectIo>,
    pub location: OwnedLocation,
}
impl WriterSession {
    pub async fn check(&self) -> Result<ObjectData> {
        if self.location.role != Role::Writer {
            return Err(refused("location.recovery_reader"));
        }
        let current = self
            .io
            .read(OWNER_KEY)
            .await?
            .ok_or_else(|| refused("location.ownership_lost"))?;
        let owner = json(&current.bytes)?;
        if owner.appliance_id != self.location.owner_appliance_id
            || owner.generation != self.location.generation
            || owner.purpose != self.location.purpose
        {
            return Err(refused("location.ownership_lost"));
        }
        Ok(current)
    }
    pub async fn put(&self, key: &str, bytes: &[u8]) -> Result<()> {
        self.check().await?;
        self.io.put(key, bytes, WriteCondition::Overwrite).await?;
        Ok(())
    }
    pub async fn delete(&self, key: &str) -> Result<()> {
        self.check().await?;
        self.io.delete(key).await
    }
    pub async fn finish(&self) -> Result<()> {
        let current = self.check().await?;
        let mut owner = json(&current.bytes)?;
        owner.last_write_at = chrono::Utc::now().to_rfc3339();
        let bytes = serde_json::to_vec(&owner).map_err(|_| refused("location.owner_invalid"))?;
        self.io
            .put(
                OWNER_KEY,
                &bytes,
                if matches!(self.location.claim_safety, ClaimSafety::Conditional) {
                    WriteCondition::IfMatch(current.etag)
                } else {
                    WriteCondition::Overwrite
                },
            )
            .await?;
        if self
            .io
            .read(OWNER_KEY)
            .await?
            .is_none_or(|actual| actual.bytes != bytes)
        {
            return Err(refused("location.claim_unverified"));
        }
        Ok(())
    }
}
