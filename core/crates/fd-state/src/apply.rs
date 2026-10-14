//! `ApplyChange` and `ValidateChange` (FR-013, FR-014).
//!
//! One SQLite transaction per batch. Validators see the proposed state. The commit channel is
//! notified only after the transaction commits.

use std::collections::{BTreeMap, HashMap};

use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde_json::Value;
use tokio::sync::broadcast;

use crate::db::{self, Db, DbError};
use crate::resources::{ConnectorRoutesSpec, ExportProtocol, ResourceSpec, paths_overlap};
use crate::{ResourceKind, ResourceRef, SINGLETON_ID, Severity, ValidationIssue, new_id};

/// Who requested the change. Reconcilers use this to avoid echoing their own writes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChangeOrigin {
    Management { actor: String, request_id: String },
    System { reason: String },
    Restore,
    Console { actor: String },
}

#[derive(Clone, Debug)]
pub enum Change {
    Upsert {
        resource: ResourceRef,
        expected_generation: u64,
        spec: Value,
    },
    Delete {
        resource: ResourceRef,
        expected_generation: u64,
    },
}

impl Change {
    pub fn resource(&self) -> &ResourceRef {
        match self {
            Self::Upsert { resource, .. } | Self::Delete { resource, .. } => resource,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct StoredResource {
    pub resource: ResourceRef,
    pub generation: u64,
    pub spec: Value,
    pub status: Option<Value>,
    pub deleted: bool,
    pub change_seq: i64,
    pub created_at: String,
    pub updated_at: String,
}

impl StoredResource {
    pub fn parsed_spec(&self) -> Result<ResourceSpec, ValidationIssue> {
        ResourceSpec::parse(self.resource.kind, &self.spec)
    }
}

#[derive(Clone, Debug)]
pub struct ApplyOutcome {
    pub committed: Vec<StoredResource>,
    pub warnings: Vec<ValidationIssue>,
    pub change_seq: i64,
}

#[derive(Clone, Debug)]
pub struct CommitNotice {
    pub change_seq: i64,
    pub resources: Vec<ResourceRef>,
    pub origin: ChangeOrigin,
}

#[derive(Debug, thiserror::Error)]
pub enum ApplyError {
    #[error("the batch is invalid")]
    Invalid(Vec<ValidationIssue>),
    #[error("generation mismatch for {0}")]
    Conflict(ResourceRef),
    #[error(transparent)]
    Db(#[from] DbError),
}

impl From<rusqlite::Error> for ApplyError {
    fn from(error: rusqlite::Error) -> Self {
        ApplyError::Db(error.into())
    }
}

/// Cross-resource check. Field-level checks live on the spec types.
pub trait Validator: Send + Sync {
    fn validate(&self, view: &StateView, changes: &[Change]) -> Vec<ValidationIssue>;
}

#[derive(Default)]
pub struct ValidatorRegistry {
    validators: Vec<Box<dyn Validator>>,
}

impl ValidatorRegistry {
    pub fn new() -> Self {
        let mut registry = Self::default();
        registry.register(BuiltinValidators);
        registry
    }

    pub fn register(&mut self, validator: impl Validator + 'static) {
        self.validators.push(Box::new(validator));
    }

    pub fn run(&self, view: &StateView, changes: &[Change]) -> Vec<ValidationIssue> {
        self.validators
            .iter()
            .flat_map(|v| v.validate(view, changes))
            .collect()
    }
}

/// Read view of desired state as it will look after the batch is applied.
pub struct StateView<'a> {
    rows: BTreeMap<(ResourceKind, String), StoredResource>,
    pub enforce_capabilities: bool,
    pub capabilities: std::collections::BTreeSet<String>,
    pub original_folders: BTreeMap<String, StoredResource>,
    _db: std::marker::PhantomData<&'a ()>,
}

impl StateView<'_> {
    fn from_connection(conn: &Connection) -> Result<Self, DbError> {
        let mut stmt = conn.prepare(
            "SELECT kind, id, generation, spec, status, deleted, change_seq, created_at, updated_at
             FROM resource WHERE deleted = 0",
        )?;
        let rows = stmt
            .query_map([], row_from_sql)?
            .collect::<Result<Vec<_>, _>>()?;
        let original_folders = rows
            .iter()
            .filter(|r| r.resource.kind == ResourceKind::ManagedFolder)
            .map(|r| (r.resource.id.clone(), r.clone()))
            .collect();
        let mut capabilities = std::collections::BTreeSet::from(["storage.local".to_owned()]);
        let exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='subsystem')",
            [],
            |r| r.get(0),
        )?;
        if exists {
            let mut q = conn.prepare(
                "SELECT id,class,manifest FROM subsystem WHERE enabled=1 AND state='Running'",
            )?;
            for entry in q.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })? {
                let (id, class, manifest) = entry?;
                let manifest: Value = serde_json::from_str(&manifest)?;
                let caps = if class == "Extension" {
                    db::get_setting(conn, &format!("subsystem.registered_capabilities.{id}"))?
                        .unwrap_or(serde_json::json!([]))
                } else {
                    manifest["capabilities"].clone()
                };
                capabilities.extend(
                    caps.as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|v| v.as_str().map(str::to_owned)),
                );
            }
        }
        Ok(Self {
            enforce_capabilities: true,
            capabilities,
            original_folders,
            rows: rows
                .into_iter()
                .map(|row| ((row.resource.kind, row.resource.id.clone()), row))
                .collect(),
            _db: std::marker::PhantomData,
        })
    }

    pub fn get(&self, resource: &ResourceRef) -> Option<&StoredResource> {
        self.rows.get(&(resource.kind, resource.id.clone()))
    }

    pub fn list(&self, kind: ResourceKind) -> impl Iterator<Item = &StoredResource> {
        self.rows
            .values()
            .filter(move |row| row.resource.kind == kind)
    }

    fn apply_projection(&mut self, change: &Change, now: &str, seq: i64) -> Result<(), ApplyError> {
        match change {
            Change::Upsert {
                resource,
                spec,
                expected_generation,
                ..
            } => {
                let generation =
                    if let Some(existing) = self.rows.get(&(resource.kind, resource.id.clone())) {
                        if existing.generation != *expected_generation {
                            return Err(ApplyError::Conflict(resource.clone()));
                        }
                        existing.generation + 1
                    } else if *expected_generation == 0 {
                        1
                    } else {
                        return Err(ApplyError::Conflict(resource.clone()));
                    };
                let created_at = self
                    .rows
                    .get(&(resource.kind, resource.id.clone()))
                    .map(|r| r.created_at.clone())
                    .unwrap_or_else(|| now.to_owned());
                self.rows.insert(
                    (resource.kind, resource.id.clone()),
                    StoredResource {
                        resource: resource.clone(),
                        generation,
                        spec: spec.clone(),
                        status: None,
                        deleted: false,
                        change_seq: seq,
                        created_at,
                        updated_at: now.to_owned(),
                    },
                );
            }
            Change::Delete {
                resource,
                expected_generation,
            } => match self.rows.remove(&(resource.kind, resource.id.clone())) {
                Some(existing) if existing.generation == *expected_generation => {}
                _ => return Err(ApplyError::Conflict(resource.clone())),
            },
        }
        Ok(())
    }
}

/// Desired-state store. Shared by the Core daemon and tests.
pub struct StateStore {
    db: Db,
    validators: ValidatorRegistry,
    commits: broadcast::Sender<CommitNotice>,
}

impl StateStore {
    pub fn new(db: Db) -> Self {
        let (commits, _) = broadcast::channel(256);
        Self {
            db,
            validators: ValidatorRegistry::new(),
            commits,
        }
    }

    pub fn db(&self) -> &Db {
        &self.db
    }

    pub fn subscribe(&self) -> broadcast::Receiver<CommitNotice> {
        self.commits.subscribe()
    }

    pub fn validators_mut(&mut self) -> &mut ValidatorRegistry {
        &mut self.validators
    }

    pub fn validate(&self, changes: &[Change]) -> Result<Vec<ValidationIssue>, ApplyError> {
        self.db.with(|conn| {
            let prepared = prepare_changes(changes)?;
            let mut view = StateView::from_connection(conn)?;
            let now = db::now_text();
            for change in &prepared {
                view.apply_projection(change, &now, 0)?;
            }
            Ok(collect_issues(&self.validators, &view, &prepared))
        })
    }

    pub fn apply(
        &self,
        origin: ChangeOrigin,
        changes: &[Change],
    ) -> Result<ApplyOutcome, ApplyError> {
        let validators = &self.validators;
        let (outcome, resources) = self.db.with(|conn| {
            let tx = conn.transaction()?;
            let prepared = prepare_changes(changes)?;
            let mut view = StateView::from_connection(&tx)?;
            view.enforce_capabilities = !matches!(origin, ChangeOrigin::Restore);
            let now = db::now_text();
            let seq: i64 =
                tx.query_row("SELECT change_seq FROM state_meta WHERE id = 1", [], |r| {
                    r.get(0)
                })?;
            let next_seq = seq + 1;
            for change in &prepared {
                view.apply_projection(change, &now, next_seq)?;
            }
            let issues = collect_issues(validators, &view, &prepared);
            if issues.iter().any(ValidationIssue::is_error) {
                return Err(ApplyError::Invalid(issues));
            }
            persist(&tx, &prepared, &now, next_seq)?;
            db::put_setting(&tx, "config.changed_at", &serde_json::json!(now))?;
            tx.execute(
                "UPDATE state_meta SET change_seq = ?1 WHERE id = 1",
                [next_seq],
            )?;
            let mut committed = Vec::new();
            for change in &prepared {
                if let Change::Upsert { resource, .. } = change {
                    committed.extend(load(&tx, resource)?);
                }
            }
            tx.commit()?;
            Ok((
                ApplyOutcome {
                    committed,
                    warnings: issues
                        .into_iter()
                        .filter(|i| i.severity == Severity::Warning)
                        .collect(),
                    change_seq: next_seq,
                },
                prepared
                    .iter()
                    .map(|change| change.resource().clone())
                    .collect::<Vec<_>>(),
            ))
        })?;
        let _ = self.commits.send(CommitNotice {
            change_seq: outcome.change_seq,
            resources,
            origin,
        });
        Ok(outcome)
    }

    pub fn get(&self, resource: &ResourceRef) -> Result<Option<StoredResource>, DbError> {
        self.db.with(|conn| load(conn, resource))
    }

    pub fn list(
        &self,
        kind: ResourceKind,
        after_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<StoredResource>, DbError> {
        self.db.with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT kind, id, generation, spec, status, deleted, change_seq, created_at, updated_at
                 FROM resource
                 WHERE kind = ?1 AND deleted = 0 AND (?2 IS NULL OR id > ?2)
                 ORDER BY id
                 LIMIT ?3",
            )?;
            let rows = stmt
                .query_map(params![kind.as_str(), after_id, limit as i64], row_from_sql)?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
    }

    pub fn get_including_deleted(
        &self,
        resource: &ResourceRef,
    ) -> Result<Option<StoredResource>, DbError> {
        self.db.with(|c|Ok(c.query_row("SELECT kind,id,generation,spec,status,deleted,change_seq,created_at,updated_at FROM resource WHERE kind=?1 AND id=?2",params![resource.kind.as_str(),resource.id],row_from_sql).optional()?))
    }
    pub fn deleted(&self, kind: ResourceKind) -> Result<Vec<StoredResource>, DbError> {
        self.db.with(|c|{let mut q=c.prepare("SELECT kind,id,generation,spec,status,deleted,change_seq,created_at,updated_at FROM resource WHERE kind=?1 AND deleted=1")?;Ok(q.query_map([kind.as_str()],row_from_sql)?.collect::<Result<Vec<_>,_>>()?)})
    }
    /// Observed status is written only by reconcilers, never by ApplyChange.
    pub fn write_status(&self, resource: &ResourceRef, status: &Value) -> Result<(), DbError> {
        self.db.with(|conn| {
            let n = conn.execute(
                "UPDATE resource SET status = ?1, updated_at = ?2 WHERE kind = ?3 AND id = ?4 AND deleted = 0",
                params![status.to_string(), db::now_text(), resource.kind.as_str(), resource.id],
            )?;
            if n == 0 {
                Err(DbError::Sqlite(rusqlite::Error::QueryReturnedNoRows))
            } else {
                Ok(())
            }
        })
    }

    pub fn change_seq(&self) -> Result<i64, DbError> {
        self.db.with(|conn| {
            Ok(
                conn.query_row("SELECT change_seq FROM state_meta WHERE id = 1", [], |r| {
                    r.get(0)
                })?,
            )
        })
    }

    /// Seeds singleton resources that must exist on first boot.
    pub fn seed_defaults(&self) -> Result<(), ApplyError> {
        if self
            .get(&ResourceRef::singleton(ResourceKind::ConnectorRoutes))?
            .is_some()
        {
            return Ok(());
        }
        let spec = ResourceSpec::ConnectorRoutes(ConnectorRoutesSpec::seeded());
        self.apply(
            ChangeOrigin::System {
                reason: "seed connector routes".into(),
            },
            &[Change::Upsert {
                resource: ResourceRef::singleton(ResourceKind::ConnectorRoutes),
                expected_generation: 0,
                spec: spec.to_value(),
            }],
        )?;
        Ok(())
    }
}

fn prepare_changes(changes: &[Change]) -> Result<Vec<Change>, ApplyError> {
    let mut prepared = Vec::with_capacity(changes.len());
    let mut seen = HashMap::new();
    for change in changes {
        let mut change = change.clone();
        if let Change::Upsert {
            resource,
            expected_generation,
            spec,
        } = &mut change
        {
            if resource.kind == ResourceKind::ManagedFolder {
                *spec = crate::validate::folder::normalize(spec.clone());
            }
            if resource.kind.is_singleton() {
                if resource.id.is_empty() {
                    resource.id = SINGLETON_ID.to_owned();
                } else if resource.id != SINGLETON_ID {
                    return Err(ApplyError::Invalid(vec![ValidationIssue::error(
                        "resource.singleton_id",
                        "id",
                        format!(
                            "{} is a singleton stored as '{SINGLETON_ID}'",
                            resource.kind
                        ),
                    )]));
                }
            } else if resource.id.is_empty() {
                if *expected_generation != 0 {
                    return Err(ApplyError::Conflict(resource.clone()));
                }
                resource.id = new_id();
            }
        }
        let key = (change.resource().kind, change.resource().id.clone());
        if seen.insert(key, ()).is_some() {
            return Err(ApplyError::Invalid(vec![ValidationIssue::error(
                "change.duplicate",
                "changes",
                format!("{} appears twice in the batch", change.resource()),
            )]));
        }
        prepared.push(change);
    }
    Ok(prepared)
}

fn collect_issues(
    validators: &ValidatorRegistry,
    view: &StateView<'_>,
    changes: &[Change],
) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();
    for change in changes {
        if let Change::Upsert { resource, spec, .. } = change {
            match ResourceSpec::parse(resource.kind, spec) {
                Ok(parsed) => issues.extend(parsed.validate()),
                Err(issue) => issues.push(issue),
            }
        }
    }
    issues.extend(validators.run(view, changes));
    issues
}

fn persist(tx: &Transaction<'_>, changes: &[Change], now: &str, seq: i64) -> Result<(), DbError> {
    for change in changes {
        match change {
            Change::Upsert { resource, spec, .. } => {
                tx.execute(
                    "INSERT INTO resource (kind, id, generation, spec, status, deleted, change_seq, created_at, updated_at)
                     VALUES (?1, ?2, 1, ?3, NULL, 0, ?4, ?5, ?5)
                     ON CONFLICT (kind, id) DO UPDATE SET
                        generation = resource.generation + 1,
                        spec = excluded.spec,
                        deleted = 0,
                        change_seq = excluded.change_seq,
                        updated_at = excluded.updated_at",
                    params![resource.kind.as_str(), resource.id, spec.to_string(), seq, now],
                )?;
            }
            Change::Delete { resource, .. } => {
                tx.execute(
                    "UPDATE resource SET deleted = 1, generation = generation + 1, change_seq = ?1, updated_at = ?2
                     WHERE kind = ?3 AND id = ?4",
                    params![seq, now, resource.kind.as_str(), resource.id],
                )?;
            }
        }
    }
    Ok(())
}

fn load(conn: &Connection, resource: &ResourceRef) -> Result<Option<StoredResource>, DbError> {
    Ok(conn
        .query_row(
            "SELECT kind, id, generation, spec, status, deleted, change_seq, created_at, updated_at
             FROM resource WHERE kind = ?1 AND id = ?2 AND deleted = 0",
            params![resource.kind.as_str(), resource.id],
            row_from_sql,
        )
        .optional()?)
}

fn row_from_sql(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredResource> {
    let kind = ResourceKind::parse(&row.get::<_, String>(0)?).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "unknown kind",
            )),
        )
    })?;
    let spec: String = row.get(3)?;
    let status: Option<String> = row.get(4)?;
    Ok(StoredResource {
        resource: ResourceRef {
            kind,
            id: row.get(1)?,
        },
        generation: row.get::<_, i64>(2)? as u64,
        spec: serde_json::from_str(&spec).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e))
        })?,
        status: status
            .map(|text| serde_json::from_str(&text))
            .transpose()
            .map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    4,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })?,
        deleted: row.get::<_, i64>(5)? != 0,
        change_seq: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

struct BuiltinValidators;

impl Validator for BuiltinValidators {
    fn validate(&self, view: &StateView, _changes: &[Change]) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();
        unique_name(
            view,
            ResourceKind::Pool,
            "name",
            "name",
            "pool.name_conflict",
            &mut issues,
        );
        unique_name(
            view,
            ResourceKind::ManagedFolder,
            "name",
            "name",
            "folder.name_conflict",
            &mut issues,
        );
        unique_name(
            view,
            ResourceKind::ShareUser,
            "username",
            "username",
            "share_user.name_conflict",
            &mut issues,
        );
        unique_name(
            view,
            ResourceKind::CloudConnection,
            "name",
            "name",
            "connection.name_conflict",
            &mut issues,
        );
        unique_name(
            view,
            ResourceKind::ContainerApp,
            "name",
            "name",
            "container.name_conflict",
            &mut issues,
        );
        folder_rules(view, &mut issues);
        issues.extend(crate::validate::folder::validate(view, _changes));
        for extension in view.list(ResourceKind::Extension) {
            if let Ok(ResourceSpec::Extension(spec)) = extension.parsed_spec() {
                for folder in spec.data_bindings.values() {
                    if view
                        .get(&ResourceRef::new(ResourceKind::ManagedFolder, folder))
                        .is_none()
                    {
                        issues.push(ValidationIssue::error("folder.bound_to_extension", "dataBindings", "A bound folder must exist and cannot be deleted while its extension binding exists").with_conflicting(extension.resource.clone()));
                    }
                }
            }
        }
        export_rules(view, &mut issues);
        disk_membership(view, &mut issues);
        system_pool_count(view, &mut issues);
        issues
    }
}

fn unique_name(
    view: &StateView<'_>,
    kind: ResourceKind,
    json_field: &str,
    issue_field: &str,
    code: &str,
    issues: &mut Vec<ValidationIssue>,
) {
    let mut seen: HashMap<String, ResourceRef> = HashMap::new();
    for row in view.list(kind) {
        let Some(name) = row.spec.get(json_field).and_then(Value::as_str) else {
            continue;
        };
        let key = name.to_ascii_lowercase();
        if let Some(other) = seen.insert(key, row.resource.clone()) {
            issues.push(
                ValidationIssue::error(code, issue_field, format!("'{name}' is already used"))
                    .with_conflicting(other),
            );
        }
    }
}

fn folder_rules(view: &StateView<'_>, issues: &mut Vec<ValidationIssue>) {
    let folders: Vec<_> = view.list(ResourceKind::ManagedFolder).collect();
    for row in &folders {
        let Ok(ResourceSpec::ManagedFolder(spec)) = row.parsed_spec() else {
            continue;
        };
        if let Some((pool_id, rel)) = spec.local_reference() {
            let pool = ResourceRef::new(ResourceKind::Pool, pool_id);
            if view.get(&pool).is_none() {
                issues.push(
                    ValidationIssue::error(
                        "reference.pool_missing",
                        "spec.reference.poolId",
                        "the pool does not exist",
                    )
                    .with_conflicting(pool),
                );
            }
            for other in &folders {
                if other.resource == row.resource {
                    continue;
                }
                let Ok(ResourceSpec::ManagedFolder(other_spec)) = other.parsed_spec() else {
                    continue;
                };
                if let Some((other_pool, other_rel)) = other_spec.local_reference()
                    && other_pool == pool_id
                    && paths_overlap(rel, other_rel)
                {
                    issues.push(
                        ValidationIssue::error(
                            "reference.overlap",
                            "spec.reference.relativePath",
                            format!(
                                "Local reference overlaps folder {} ({})",
                                other_spec.name, other_rel
                            ),
                        )
                        .with_conflicting(other.resource.clone()),
                    );
                }
            }
        }
        for other in &folders {
            if other.resource == row.resource {
                continue;
            }
            let Ok(ResourceSpec::ManagedFolder(other_spec)) = other.parsed_spec() else {
                continue;
            };
            if paths_overlap(&spec.filedock_path, &other_spec.filedock_path) {
                issues.push(
                    ValidationIssue::error(
                        "folder.path_overlap",
                        "spec.filedockPath",
                        format!("FileDock path overlaps folder {} ({}). Subfolders use their parent’s storage mode; Hybrid Backup permits importance overrides.",other_spec.name,other_spec.filedock_path),
                    )
                    .with_conflicting(other.resource.clone()),
                );
            }
        }
    }
}

fn export_rules(view: &StateView<'_>, issues: &mut Vec<ValidationIssue>) {
    let mut owners = HashMap::new();
    for row in view.list(ResourceKind::Export) {
        let Ok(ResourceSpec::Export(spec)) = row.parsed_spec() else {
            continue;
        };
        if let Some(other) = owners.insert(
            format!("{}:{:?}", spec.folder_id, spec.protocol),
            row.resource.clone(),
        ) {
            issues.push(
                ValidationIssue::error(
                    "export.duplicate",
                    "folderId",
                    "Only one export of each protocol is allowed per folder",
                )
                .with_conflicting(other),
            );
        }
        let folder_ref = ResourceRef::new(ResourceKind::ManagedFolder, spec.folder_id.clone());
        let Some(folder) = view.get(&folder_ref) else {
            issues.push(
                ValidationIssue::error(
                    "export.folder_missing",
                    "spec.folderId",
                    view.original_folders.get(&spec.folder_id).map(|f|format!("Folder {} is still used by export {}. Remove the export before deleting its folder.",f.spec["name"],row.resource.id)).unwrap_or_else(||"the folder does not exist".into()),
                )
                .with_conflicting(folder_ref),
            );
            continue;
        };
        let Ok(ResourceSpec::ManagedFolder(folder_spec)) = folder.parsed_spec() else {
            continue;
        };
        let needed = match spec.protocol {
            ExportProtocol::Smb => crate::resources::Access::Smb,
            ExportProtocol::Nfs => crate::resources::Access::Nfs,
            ExportProtocol::WebDav => crate::resources::Access::WebDav,
        };
        if !folder_spec.access.contains(&needed) {
            issues.push(ValidationIssue::error(
                "export.access_missing",
                "spec.protocol",
                "the folder does not enable this protocol",
            ));
        }
    }
}

fn disk_membership(view: &StateView<'_>, issues: &mut Vec<ValidationIssue>) {
    let mut seen: HashMap<String, ResourceRef> = HashMap::new();
    for row in view.list(ResourceKind::Pool) {
        let Ok(ResourceSpec::Pool(spec)) = row.parsed_spec() else {
            continue;
        };
        for disk in &spec.member_disks {
            if let Some(other) = seen.insert(disk.serial.clone(), row.resource.clone()) {
                issues.push(
                    ValidationIssue::error(
                        "pool.disk_in_use",
                        "spec.memberDisks",
                        format!("disk {} is already in another pool", disk.serial),
                    )
                    .with_conflicting(other),
                );
            }
        }
    }
}

fn system_pool_count(view: &StateView<'_>, issues: &mut Vec<ValidationIssue>) {
    let systems: Vec<_> = view
        .list(ResourceKind::Pool)
        .filter(|row| {
            row.parsed_spec().ok().and_then(|spec| match spec {
                ResourceSpec::Pool(p) => Some(p.kind),
                _ => None,
            }) == Some(crate::resources::PoolKind::System)
        })
        .collect();
    if systems.len() > 1 {
        issues.push(ValidationIssue::error(
            "system_pool.duplicate",
            "spec.kind",
            "exactly one System Pool is allowed",
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{Db, MIGRATION_0001_INIT};
    use serde_json::json;

    fn store() -> StateStore {
        StateStore::new(Db::open_in_memory(&[MIGRATION_0001_INIT]).unwrap())
    }

    fn pool_spec(name: &str) -> Value {
        json!({"name": name, "kind": "Lvm", "layers": ["lvm"], "memberDisks": [{"serial": name}]})
    }

    #[test]
    fn apply_is_one_transaction_and_notifies() {
        let store = store();
        let mut rx = store.subscribe();
        let outcome = store
            .apply(
                ChangeOrigin::System {
                    reason: "test".into(),
                },
                &[
                    Change::Upsert {
                        resource: ResourceRef::new(ResourceKind::Pool, ""),
                        expected_generation: 0,
                        spec: pool_spec("media"),
                    },
                    Change::Upsert {
                        resource: ResourceRef::new(ResourceKind::Pool, ""),
                        expected_generation: 0,
                        spec: pool_spec("backup"),
                    },
                ],
            )
            .unwrap();
        assert_eq!(outcome.committed.len(), 2);
        assert_eq!(outcome.change_seq, 1);
        let notice = rx.try_recv().unwrap();
        assert_eq!(notice.change_seq, 1);
        assert_eq!(store.list(ResourceKind::Pool, None, 10).unwrap().len(), 2);
    }

    #[test]
    fn generation_mismatch_is_conflict() {
        let store = store();
        let created = store
            .apply(
                ChangeOrigin::System { reason: "t".into() },
                &[Change::Upsert {
                    resource: ResourceRef::new(ResourceKind::ShareUser, "u1"),
                    expected_generation: 0,
                    spec: json!({"username": "alice", "passwordSecret": "share-user/alice/password"}),
                }],
            )
            .unwrap();
        let err = store
            .apply(
                ChangeOrigin::System { reason: "t".into() },
                &[Change::Upsert {
                    resource: created.committed[0].resource.clone(),
                    expected_generation: 0,
                    spec: json!({"username": "alice", "passwordSecret": "share-user/alice/password"}),
                }],
            )
            .unwrap_err();
        assert!(matches!(err, ApplyError::Conflict(_)));
    }

    #[test]
    fn validation_errors_roll_back() {
        let store = store();
        let err = store
            .apply(
                ChangeOrigin::System { reason: "t".into() },
                &[
                    Change::Upsert {
                        resource: ResourceRef::new(ResourceKind::ShareUser, "a"),
                        expected_generation: 0,
                        spec: json!({"username": "alice", "passwordSecret": "share-user/alice/password"}),
                    },
                    Change::Upsert {
                        resource: ResourceRef::new(ResourceKind::ShareUser, "b"),
                        expected_generation: 0,
                        spec: json!({"username": "alice", "passwordSecret": "share-user/bob/password"}),
                    },
                ],
            )
            .unwrap_err();
        match err {
            ApplyError::Invalid(issues) => {
                assert!(issues.iter().any(|i| i.code == "share_user.name_conflict"));
            }
            other => panic!("{other:?}"),
        }
        assert!(
            store
                .list(ResourceKind::ShareUser, None, 10)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn seed_routes_is_idempotent() {
        let store = store();
        store.seed_defaults().unwrap();
        store.seed_defaults().unwrap();
        let row = store
            .get(&ResourceRef::singleton(ResourceKind::ConnectorRoutes))
            .unwrap()
            .unwrap();
        assert_eq!(row.generation, 1);
    }
    #[test]
    fn commit_notifies_the_resolved_resource_identity() {
        let store = store();
        let mut commits = store.subscribe();
        let result = store
            .apply(
                ChangeOrigin::System {
                    reason: "test".into(),
                },
                &[Change::Upsert {
                    resource: ResourceRef::new(ResourceKind::Settings, ""),
                    expected_generation: 0,
                    spec: json!({"applianceName":"resolved"}),
                }],
            )
            .unwrap();
        assert_eq!(
            commits.try_recv().unwrap().resources,
            vec![result.committed[0].resource.clone()]
        );
        assert_eq!(result.committed[0].resource.id, "default");
    }
}
