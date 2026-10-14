use async_trait::async_trait;
use fd_connectors::ownership::*;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
#[derive(Default)]
struct Memory {
    data: Mutex<BTreeMap<String, ObjectData>>,
    writes: Mutex<usize>,
}
#[async_trait]
impl ObjectIo for Memory {
    async fn read(&self, key: &str) -> Result<Option<ObjectData>> {
        Ok(self.data.lock().unwrap().get(key).cloned())
    }
    async fn list(&self, prefix: &str) -> Result<Vec<String>> {
        Ok(self
            .data
            .lock()
            .unwrap()
            .keys()
            .filter(|k| k.starts_with(prefix))
            .cloned()
            .collect())
    }
    async fn put(&self, key: &str, bytes: &[u8], condition: WriteCondition) -> Result<String> {
        let mut data = self.data.lock().unwrap();
        if match condition {
            WriteCondition::CreateOnly => data.contains_key(key),
            WriteCondition::IfMatch(etag) => data.get(key).is_none_or(|d| d.etag != etag),
            WriteCondition::Overwrite => false,
        } {
            return Err(OwnershipError::Refused("condition".into()));
        }
        let mut writes = self.writes.lock().unwrap();
        *writes += 1;
        let etag = writes.to_string();
        data.insert(
            key.into(),
            ObjectData {
                bytes: bytes.into(),
                etag: etag.clone(),
            },
        );
        Ok(etag)
    }
    async fn delete(&self, key: &str) -> Result<()> {
        *self.writes.lock().unwrap() += 1;
        self.data.lock().unwrap().remove(key);
        Ok(())
    }
}
fn location(owner: &Owner, role: Role) -> OwnedLocation {
    OwnedLocation {
        id: "1".into(),
        connection_id: "c".into(),
        bucket: "b".into(),
        prefix: "p".into(),
        purpose: "ApplianceState".into(),
        owner_appliance_id: owner.appliance_id.clone(),
        generation: owner.generation,
        last_write_at: owner.last_write_at.clone(),
        role,
        served_by_connector: "fd-s3".into(),
        served_by_third_party: false,
        claim_safety: ClaimSafety::Conditional,
    }
}
#[tokio::test]
async fn readers_never_write_and_stale_writer_stops() {
    let io = Arc::new(Memory::default());
    let id = uuid::Uuid::new_v4().to_string();
    let owner = claim(io.as_ref(), &id, "a", ClaimSafety::Conditional, false)
        .await
        .unwrap();
    let reader = WriterSession {
        io: io.clone(),
        location: location(&owner, Role::RecoveryReader),
    };
    let before = *io.writes.lock().unwrap();
    assert!(reader.put("snapshot", b"bad").await.is_err());
    assert!(reader.delete("snapshot").await.is_err());
    assert!(reader.finish().await.is_err());
    assert_eq!(before, *io.writes.lock().unwrap());
    let writer = WriterSession {
        io: io.clone(),
        location: location(&owner, Role::Writer),
    };
    writer.put("snapshot", b"good").await.unwrap();
    takeover(
        io.as_ref(),
        &id,
        "b",
        owner.generation,
        &ClaimSafety::Conditional,
        true,
        false,
    )
    .await
    .unwrap();
    let before = *io.writes.lock().unwrap();
    assert!(writer.put("snapshot", b"bad").await.is_err());
    assert_eq!(before, *io.writes.lock().unwrap());
}
#[tokio::test]
async fn nonempty_foreign_basic_and_recent_claims_are_refused() {
    let io = Memory::default();
    let id = uuid::Uuid::new_v4().to_string();
    assert!(
        claim(&io, &id, "a", ClaimSafety::Basic, false)
            .await
            .is_err()
    );
    io.put("foreign", b"content", WriteCondition::Overwrite)
        .await
        .unwrap();
    assert!(
        claim(&io, &id, "a", ClaimSafety::Conditional, false)
            .await
            .is_err()
    );
    io.delete("foreign").await.unwrap();
    let owner = claim(&io, &id, "a", ClaimSafety::Conditional, false)
        .await
        .unwrap();
    assert!(
        claim(
            &io,
            &uuid::Uuid::new_v4().to_string(),
            "b",
            ClaimSafety::Conditional,
            false
        )
        .await
        .is_err()
    );
    assert!(
        takeover(
            &io,
            &id,
            "b",
            owner.generation + 1,
            &ClaimSafety::Conditional,
            true,
            false
        )
        .await
        .is_err()
    );
    assert!(
        takeover(
            &io,
            &id,
            "b",
            owner.generation,
            &ClaimSafety::Conditional,
            false,
            false
        )
        .await
        .is_err()
    );
    assert!(
        takeover(
            &io,
            &id,
            "b",
            owner.generation,
            &ClaimSafety::Conditional,
            true,
            true
        )
        .await
        .is_err()
    );
}
