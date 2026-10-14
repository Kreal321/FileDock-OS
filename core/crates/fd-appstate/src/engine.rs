use crate::{
    Result, atomic_write,
    copy::{
        backends::{CopyKind, CopyTarget},
        index::{Index, SnapshotEntry},
        retention,
    },
    fdconfig,
    format::{
        fdbackup,
        fdstate::{self, SnapshotPayload},
    },
    invalid,
    scheduler::Debounce,
};
use fd_secrets::recovery_key::RecoveryKeyInfo;
use fd_state::{Db, db};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::PathBuf, time::Instant};
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CopyRecord {
    pub id: String,
    pub kind: CopyKind,
    pub pool_id: String,
    pub description: String,
    pub last_snapshot_at: Option<String>,
    #[serde(default)]
    pub last_snapshot_id: Option<String>,
    pub covered_fingerprint: String,
    #[serde(default)]
    pub covered_core_seq: i64,
    #[serde(default)]
    pub covered_management_seq: u64,
    pub last_error: String,
    pub available: bool,
}
pub struct SnapshotInput {
    pub appliance_id: String,
    pub config: Value,
    pub core_seq: i64,
    pub management_seq: u64,
    pub changed_at: Option<String>,
    pub secrets: Vec<u8>,
    pub dek: [u8; 32],
    pub key: RecoveryKeyInfo,
    pub masters: BTreeMap<String, [u8; 32]>,
    pub packages: BTreeMap<String, Vec<u8>>,
    pub release_version: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Prepared {
    pub entry: SnapshotEntry,
    pub fingerprint: String,
    pub key: RecoveryKeyInfo,
    pub core_seq: i64,
    pub management_seq: u64,
}
pub struct Engine {
    pub db: Db,
    pub spool: PathBuf,
    pub seen: String,
    pub debounce: Debounce,
}
impl Engine {
    pub fn new(db: Db, spool: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&spool)?;
        Ok(Self {
            db,
            spool,
            seen: String::new(),
            debounce: Debounce::default(),
        })
    }
    pub fn configure_timing(&mut self, quiet: std::time::Duration, cap: std::time::Duration) {
        self.debounce = Debounce::new(quiet, cap);
    }
    pub fn prepared(&self) -> Result<Option<Prepared>> {
        self.db.with(|c| {
            Ok(db::get_setting(c, "state.prepared")?
                .map(serde_json::from_value)
                .transpose()?)
        })
    }
    pub fn fingerprint(input: &SnapshotInput) -> Result<String> {
        let mut hash = Sha256::new();
        let mut configuration = input.config.clone();
        for location in configuration["core"]["ownedLocations"]
            .as_array_mut()
            .into_iter()
            .flatten()
        {
            if let Some(object) = location.as_object_mut() {
                object.remove("lastWriteAt");
            }
        }
        hash.update(crate::canonical(&configuration)?);
        hash.update(&input.secrets);
        hash.update(input.key.key_id.as_bytes());
        Ok(hex::encode(hash.finalize()))
    }
    pub fn records(db: &Db) -> Result<Vec<CopyRecord>> {
        db.with(|c| {
            Ok(db::get_setting(c, "state.copies")?
                .map(serde_json::from_value)
                .transpose()?
                .unwrap_or_default())
        })
    }
    pub fn inventory(&self, targets: &[CopyTarget]) -> Result<Vec<CopyRecord>> {
        let previous = Self::records(&self.db)?;
        let records = targets
            .iter()
            .map(|t| {
                let mut r = previous
                    .iter()
                    .find(|r| r.id == t.id)
                    .cloned()
                    .unwrap_or(CopyRecord {
                        id: t.id.clone(),
                        kind: t.kind.clone(),
                        pool_id: t.pool_id.clone(),
                        description: t.description.clone(),
                        last_snapshot_at: None,
                        last_snapshot_id: None,
                        covered_fingerprint: String::new(),
                        covered_core_seq: 0,
                        covered_management_seq: 0,
                        last_error: String::new(),
                        available: t.available,
                    });
                r.available = t.available;
                r.description = t.description.clone();
                r
            })
            .collect::<Vec<_>>();
        self.save_records(&records)?;
        Ok(records)
    }
    fn save_records(&self, records: &[CopyRecord]) -> Result<()> {
        self.db
            .with(|c| db::put_setting(c, "state.copies", &json!(records)))?;
        Ok(())
    }
    pub fn prepare(&mut self, input: &SnapshotInput, fingerprint: String) -> Result<Prepared> {
        fdconfig::validate(&input.config)?;
        let master = input
            .masters
            .get(&input.key.key_id)
            .ok_or_else(|| invalid("recovery.key_unconfirmed"))?;
        let mut hashes = Vec::new();
        for item in input.config["core"]["subsystems"]
            .as_array()
            .into_iter()
            .flatten()
        {
            if let Some(hash) = item["packageSha256"].as_str() {
                if !input.packages.contains_key(hash) {
                    return Err(invalid("state.package_missing"));
                }
                hashes.push(hash.to_owned());
            }
        }
        let id = uuid::Uuid::now_v7().to_string();
        let taken = chrono::Utc::now();
        let file = format!(
            "snapshots/{}_{}.fdstate",
            taken.format("%Y%m%dT%H%M%S%3fZ"),
            id
        );
        let generations: Value = input.config["core"]["ownedLocations"]
            .as_array()
            .map(|locations| {
                locations
                    .iter()
                    .filter_map(|l| Some((l["id"].as_str()?.to_owned(), l["generation"].clone())))
                    .collect::<serde_json::Map<_, _>>()
                    .into()
            })
            .unwrap_or(json!({}));
        let manifest = json!({"schemaVersion":1,"applianceId":input.appliance_id,"applianceName":input.config["appliance"]["name"],"takenAt":taken.to_rfc3339(),"releaseVersion":input.release_version,"configSchemaVersion":"fdconfig/1","coreConfigSeq":input.core_seq,"managementConfigSeq":input.management_seq,"recordedGenerations":generations,"packages":hashes});
        let bytes = fdstate::encode(
            &SnapshotPayload {
                manifest,
                config: input.config.clone(),
                secrets: input.secrets.clone(),
                dek: input.dek,
            },
            master,
        )?;
        let entry = SnapshotEntry {
            snapshot_id: id.clone(),
            taken_at: taken.to_rfc3339(),
            file,
            size_bytes: bytes.len() as u64,
            sha256: hex::encode(Sha256::digest(&bytes)),
            key_id: input.key.key_id.clone(),
            retention_class: "Recent".into(),
            packages: hashes,
            release_version: input.release_version.clone(),
        };
        atomic_write(&self.spool.join(format!("{id}.fdstate")), &bytes)?;
        for (hash, plain) in &input.packages {
            if hex::encode(Sha256::digest(plain)) != *hash {
                return Err(invalid("state.package_hash_mismatch"));
            }
            let encrypted = fdstate::encrypt_bytes(plain, master, b"FDPKG\0\0\0")?;
            atomic_write(&self.spool.join(format!("{hash}.fdpkg")), &encrypted)?;
        }
        let prepared = Prepared {
            entry,
            fingerprint,
            key: input.key.clone(),
            core_seq: input.core_seq,
            management_seq: input.management_seq,
        };
        self.db
            .with(|c| db::put_setting(c, "state.prepared", &json!(prepared)))?;
        for entry in std::fs::read_dir(&self.spool)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            if entry.file_type()?.is_file()
                && name.ends_with(".fdstate")
                && name != format!("{id}.fdstate")
            {
                std::fs::remove_file(entry.path())?;
            }
        }
        self.debounce.clear();
        Ok(prepared)
    }
    pub async fn tick(
        &mut self,
        input: &SnapshotInput,
        targets: &[CopyTarget],
        force: bool,
    ) -> Result<bool> {
        let fingerprint = Self::fingerprint(input)?;
        let mut records = self.inventory(targets)?;
        self.db
            .with(|c| db::put_setting(c, "state.latest_fingerprint", &json!(fingerprint)))?;
        if self.seen != fingerprint {
            self.seen = fingerprint.clone();
            self.debounce.changed(Instant::now());
            let previous = self
                .db
                .with(|c| db::get_setting(c, "state.previous_fingerprint"))?;
            if previous.as_ref().and_then(Value::as_str) != Some(&fingerprint) {
                self.db.with(|c| {
                    db::put_setting(
                        c,
                        "state.latest_change_at",
                        &json!(
                            input
                                .changed_at
                                .clone()
                                .unwrap_or_else(|| chrono::Utc::now().to_rfc3339())
                        ),
                    )?;
                    db::put_setting(c, "state.previous_fingerprint", &json!(fingerprint))
                })?;
            }
        }
        let mut prepared = self.prepared()?;
        if force
            || prepared
                .as_ref()
                .is_some_and(|p| p.key.key_id != input.key.key_id)
            || (prepared
                .as_ref()
                .is_none_or(|p| p.fingerprint != fingerprint)
                && self.debounce.ready(Instant::now()))
        {
            prepared = Some(self.prepare(input, fingerprint.clone())?);
        }
        let Some(prepared) = prepared else {
            return Ok(false);
        };
        let archive = std::fs::read(
            self.spool
                .join(format!("{}.fdstate", prepared.entry.snapshot_id)),
        )?;
        let master = input
            .masters
            .get(&prepared.key.key_id)
            .ok_or_else(|| invalid("recovery.key_unavailable"))?;
        let engine = &*self;
        let prepared_ref = &prepared;
        let archive_ref = &archive;
        use futures::StreamExt;
        let mut results = targets
            .iter()
            .enumerate()
            .map(|(position, target)| {
                let skip = records[position].covered_fingerprint == prepared.fingerprint
                    && records[position].last_snapshot_id.as_deref()
                        == Some(&prepared.entry.snapshot_id)
                    && records[position].available;
                async move {
                    let result = if skip {
                        None
                    } else if !target.available {
                        Some(Err(
                            "Copy destination is unavailable; no data was written.".to_owned()
                        ))
                    } else {
                        Some(
                            match tokio::time::timeout(
                                std::time::Duration::from_secs(20),
                                engine.write_copy(target, input, prepared_ref, archive_ref, master),
                            )
                            .await
                            {
                                Ok(Ok(())) => Ok(()),
                                Ok(Err(e)) => Err(e.to_string()),
                                Err(_) => Err("Copy upload timed out; retrying.".into()),
                            },
                        )
                    };
                    (position, result)
                }
            })
            .collect::<futures::stream::FuturesUnordered<_>>();
        while let Some((position, result)) = results.next().await {
            let record = &mut records[position];
            match result {
                Some(Ok(())) => {
                    record.covered_fingerprint = prepared.fingerprint.clone();
                    record.covered_core_seq = prepared.core_seq;
                    record.covered_management_seq = prepared.management_seq;
                    record.last_snapshot_at = Some(prepared.entry.taken_at.clone());
                    record.last_snapshot_id = Some(prepared.entry.snapshot_id.clone());
                    record.last_error.clear();
                }
                Some(Err(e)) => record.last_error = e,
                None => {}
            }
            self.save_records(&records)?;
        }
        self.save_records(&records)?;
        Ok(records.iter().all(|r| {
            r.covered_fingerprint == fingerprint
                && r.last_snapshot_id.as_deref() == Some(&prepared.entry.snapshot_id)
                && r.available
        }))
    }
    async fn write_copy(
        &self,
        target: &CopyTarget,
        input: &SnapshotInput,
        prepared: &Prepared,
        archive: &[u8],
        master: &[u8; 32],
    ) -> Result<()> {
        let mut target = target.clone();
        target.backend = target.backend.scoped().await;
        let root = format!("appliance/{}/", input.appliance_id);
        let index_path = format!("{root}index.json");
        let mut index = if let Some(raw) = target.backend.read(&index_path).await? {
            let index: Index = serde_json::from_slice(&raw)?;
            if !input.masters.values().any(|key| index.verify(key).is_ok()) {
                return Err(invalid("state.copy_integrity_failed"));
            }
            index
        } else {
            Index::new(
                &input.appliance_id,
                input.config["appliance"]["name"]
                    .as_str()
                    .unwrap_or("filedock"),
            )
        };
        for hash in &prepared.entry.packages {
            let blob = std::fs::read(self.spool.join(format!("{hash}.fdpkg")))?;
            target
                .backend
                .write(&format!("{root}packages/{hash}.fdpkg"), &blob)
                .await?;
        }
        target
            .backend
            .write(
                &format!("{root}keys/{}.json", prepared.key.key_id),
                &serde_json::to_vec(&prepared.key)?,
            )
            .await?;
        target
            .backend
            .write(&format!("{root}{}", prepared.entry.file), archive)
            .await?;
        if !index
            .snapshots
            .iter()
            .any(|e| e.snapshot_id == prepared.entry.snapshot_id)
        {
            index.snapshots.push(prepared.entry.clone());
        }
        let retained = retention::retained(&index.snapshots);
        let removed = index
            .snapshots
            .iter()
            .filter(|e| !retained.contains(&e.snapshot_id))
            .cloned()
            .collect::<Vec<_>>();
        index
            .snapshots
            .retain(|e| retained.contains(&e.snapshot_id));
        index.appliance_name = input.config["appliance"]["name"]
            .as_str()
            .unwrap_or("filedock")
            .into();
        index.signing_key_id = prepared.key.key_id.clone();
        index.sign(master)?;
        target
            .backend
            .write_index(&index_path, &serde_json::to_vec(&index)?)
            .await?;
        target.backend.finish().await?;
        // Only prune after the new snapshot and replacement index are durable in this copy.
        for entry in removed {
            target
                .backend
                .delete(&format!("{root}{}", entry.file))
                .await?;
        }
        let listed = index
            .snapshots
            .iter()
            .map(|e| format!("{root}{}", e.file))
            .collect::<std::collections::BTreeSet<_>>();
        for path in target.backend.list(&format!("{root}snapshots")).await? {
            if path.ends_with(".fdstate") && !listed.contains(&path) {
                target.backend.delete(&path).await?;
            }
        }
        let packages = index
            .snapshots
            .iter()
            .flat_map(|e| e.packages.iter())
            .map(|h| format!("{root}packages/{h}.fdpkg"))
            .collect::<std::collections::BTreeSet<_>>();
        for path in target.backend.list(&format!("{root}packages")).await? {
            if path.ends_with(".fdpkg") && !packages.contains(&path) {
                target.backend.delete(&path).await?;
            }
        }
        Ok(())
    }
    pub fn export(&self, input: &SnapshotInput) -> Result<Vec<u8>> {
        let p = self
            .prepared()?
            .ok_or_else(|| invalid("state.no_snapshot"))?;
        if p.fingerprint != Self::fingerprint(input)? {
            return Err(invalid("state.snapshot_pending"));
        }
        let master = input
            .masters
            .get(&p.key.key_id)
            .ok_or_else(|| invalid("recovery.key_unavailable"))?;
        let root = format!("appliance/{}/", input.appliance_id);
        let mut index = Index::new(
            &input.appliance_id,
            input.config["appliance"]["name"]
                .as_str()
                .unwrap_or("filedock"),
        );
        index.snapshots.push(p.entry.clone());
        index.signing_key_id = p.key.key_id.clone();
        index.sign(master)?;
        let mut files = BTreeMap::from([
            (format!("{root}index.json"), serde_json::to_vec(&index)?),
            (
                format!("{root}keys/{}.json", p.key.key_id),
                serde_json::to_vec(&p.key)?,
            ),
            (
                format!("{root}{}", p.entry.file),
                std::fs::read(self.spool.join(format!("{}.fdstate", p.entry.snapshot_id)))?,
            ),
        ]);
        for hash in p.entry.packages {
            files.insert(
                format!("{root}packages/{hash}.fdpkg"),
                std::fs::read(self.spool.join(format!("{hash}.fdpkg")))?,
            );
        }
        fdbackup::encode(&files)
    }
    pub fn protection(db: &Db) -> Result<Value> {
        let records = Self::records(db)?;
        let (change, fp) = db.with(|c| {
            Ok::<_, crate::Error>((
                db::get_setting(c, "state.latest_change_at")?,
                db::get_setting(c, "state.latest_fingerprint")?,
            ))
        })?;
        let current = fp.as_ref().and_then(Value::as_str).unwrap_or("");
        let changed = change
            .as_ref()
            .and_then(Value::as_str)
            .and_then(|v| chrono::DateTime::parse_from_rfc3339(v).ok());
        let age = changed
            .map(|v| chrono::Utc::now().signed_duration_since(v).num_seconds())
            .unwrap_or(0);
        let stale = |r: &CopyRecord| {
            r.last_snapshot_at.is_none() || (r.covered_fingerprint != current && age >= 300)
        };
        let reinstall = records
            .iter()
            .any(|r| r.kind == CopyKind::LocalPool && !stale(r));
        let loss = records.iter().any(|r| {
            matches!(
                r.kind,
                CopyKind::RemoteRemovableDisk | CopyKind::RemoteObjectStorage
            ) && !stale(r)
        });
        let registered = records.iter().any(|r| {
            matches!(
                r.kind,
                CopyKind::RemoteRemovableDisk | CopyKind::RemoteObjectStorage
            )
        });
        let copies=records.iter().map(|r|json!({"id":r.id,"kind":r.kind,"poolId":r.pool_id,"description":r.description,"lastSnapshotAt":r.last_snapshot_at,"stale":stale(r),"lastError":r.last_error})).collect::<Vec<_>>();
        Ok(
            json!({"remoteDestinationRegistered":registered,"latestChangeAt":change.unwrap_or(Value::Null),"copies":copies,"reinstallCovered":reinstall,"machineLossCovered":loss,"warning":!registered||records.iter().any(stale)||records.iter().any(|r|!r.available||!r.last_error.is_empty())}),
        )
    }
}

impl Drop for SnapshotInput {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.dek.zeroize();
        for key in self.masters.values_mut() {
            key.zeroize();
        }
    }
}
