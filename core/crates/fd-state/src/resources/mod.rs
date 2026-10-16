//! Typed desired-state specs (data-model.md). Specs are stored as camelCase JSON, and unknown
//! fields are rejected so a typo can't silently drop configuration.
//!
//! Each spec checks its own fields in `validate`. Rules that need other resources (overlaps,
//! references, routes) are registered as validators with the [`crate::ValidatorRegistry`].

mod appliance_state_destination;
mod connection;
mod connector_routes;
mod container_app;
mod export;
mod extension;
mod folder;
mod pool;
mod settings;
mod share_user;

pub use appliance_state_destination::{ApplianceStateDestinationSpec, StateDestinationTarget};
pub use connection::{
    AddressingStyle, BoundDefinition, CloudConnectionSpec, ConnectionDefinition, S3Definition,
    ServerSideEncryption, is_object_storage_family, is_valid_family,
};
pub use connector_routes::{
    Acknowledgement, AcknowledgementKind, ConnectorRoutesSpec, RouteDefault, RouteTarget, RouteUse,
};
pub use container_app::ContainerAppSpec;
pub use export::{ExportProtocol, ExportRule, ExportSpec};
pub use extension::{ExtensionSignature, ExtensionSpec};
pub use folder::{
    Access, DataImportance, ManagedFolderSpec, ReferencePath, StorageMode, paths_overlap,
};
pub use pool::{DiskIdentity, PoolKind, PoolSpec, SystemLimits};
pub use settings::{NotificationChannel, SettingsSpec};
pub use share_user::ShareUserSpec;

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::{ResourceKind, ValidationIssue};

/// A parsed spec of any kind.
#[derive(Clone, Debug, PartialEq)]
pub enum ResourceSpec {
    Pool(PoolSpec),
    ManagedFolder(ManagedFolderSpec),
    Export(ExportSpec),
    ShareUser(ShareUserSpec),
    CloudConnection(CloudConnectionSpec),
    ContainerApp(ContainerAppSpec),
    Extension(ExtensionSpec),
    ApplianceStateDestination(ApplianceStateDestinationSpec),
    Settings(SettingsSpec),
    ConnectorRoutes(ConnectorRoutesSpec),
}

impl ResourceSpec {
    /// Parses a spec. A shape error becomes one `spec.invalid` issue.
    pub fn parse(kind: ResourceKind, spec: &Value) -> Result<Self, ValidationIssue> {
        fn typed<T: DeserializeOwned>(spec: &Value) -> Result<T, ValidationIssue> {
            T::deserialize(spec)
                .map_err(|e| ValidationIssue::error("spec.invalid", "spec", e.to_string()))
        }
        Ok(match kind {
            ResourceKind::Pool => Self::Pool(typed(spec)?),
            ResourceKind::ManagedFolder => Self::ManagedFolder(typed(spec)?),
            ResourceKind::Export => Self::Export(typed(spec)?),
            ResourceKind::ShareUser => Self::ShareUser(typed(spec)?),
            ResourceKind::CloudConnection => Self::CloudConnection(typed(spec)?),
            ResourceKind::ContainerApp => Self::ContainerApp(typed(spec)?),
            ResourceKind::Extension => Self::Extension(typed(spec)?),
            ResourceKind::ApplianceStateDestination => {
                Self::ApplianceStateDestination(typed(spec)?)
            }
            ResourceKind::Settings => Self::Settings(typed(spec)?),
            ResourceKind::ConnectorRoutes => Self::ConnectorRoutes(typed(spec)?),
        })
    }

    pub fn kind(&self) -> ResourceKind {
        match self {
            Self::Pool(_) => ResourceKind::Pool,
            Self::ManagedFolder(_) => ResourceKind::ManagedFolder,
            Self::Export(_) => ResourceKind::Export,
            Self::ShareUser(_) => ResourceKind::ShareUser,
            Self::CloudConnection(_) => ResourceKind::CloudConnection,
            Self::ContainerApp(_) => ResourceKind::ContainerApp,
            Self::Extension(_) => ResourceKind::Extension,
            Self::ApplianceStateDestination(_) => ResourceKind::ApplianceStateDestination,
            Self::Settings(_) => ResourceKind::Settings,
            Self::ConnectorRoutes(_) => ResourceKind::ConnectorRoutes,
        }
    }

    /// Field-level checks that need no other resource.
    pub fn validate(&self) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();
        match self {
            Self::Pool(s) => s.validate(&mut issues),
            Self::ManagedFolder(s) => s.validate(&mut issues),
            Self::Export(s) => s.validate(&mut issues),
            Self::ShareUser(s) => s.validate(&mut issues),
            Self::CloudConnection(s) => s.validate(&mut issues),
            Self::ContainerApp(s) => s.validate(&mut issues),
            Self::Extension(s) => s.validate(&mut issues),
            Self::ApplianceStateDestination(s) => s.validate(&mut issues),
            Self::Settings(s) => s.validate(&mut issues),
            Self::ConnectorRoutes(s) => s.validate(&mut issues),
        }
        issues
            .into_iter()
            .map(|issue| issue.under("spec"))
            .collect()
    }

    /// The canonical JSON form that is stored.
    pub fn to_value(&self) -> Value {
        fn json<T: Serialize>(spec: &T) -> Value {
            serde_json::to_value(spec).expect("specs always serialize")
        }
        match self {
            Self::Pool(s) => json(s),
            Self::ManagedFolder(s) => json(s),
            Self::Export(s) => json(s),
            Self::ShareUser(s) => json(s),
            Self::CloudConnection(s) => json(s),
            Self::ContainerApp(s) => json(s),
            Self::Extension(s) => json(s),
            Self::ApplianceStateDestination(s) => json(s),
            Self::Settings(s) => json(s),
            Self::ConnectorRoutes(s) => json(s),
        }
    }
}

/// Secret references such as `connection/<id>/secret-key`.
pub fn is_valid_secret_ref(reference: &str) -> bool {
    let bytes = reference.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 255
        && bytes[0].is_ascii_alphanumeric()
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._/-".contains(b))
        && !reference.contains("..")
        && !reference.ends_with('/')
}

pub(crate) fn check_secret_ref(field: &str, reference: &str, issues: &mut Vec<ValidationIssue>) {
    if !is_valid_secret_ref(reference) {
        issues.push(ValidationIssue::error(
            "secret.ref_invalid",
            field,
            format!("'{reference}' is not a valid secret reference"),
        ));
    }
}

pub(crate) fn check_name(field: &str, name: &str, max: usize, issues: &mut Vec<ValidationIssue>) {
    let length = name.chars().count();
    if length == 0 || length > max || name.trim() != name {
        issues.push(ValidationIssue::error(
            "name.invalid",
            field,
            format!("must be 1 to {max} characters, without leading or trailing spaces"),
        ));
    }
}

pub(crate) fn is_uuid(text: &str) -> bool {
    uuid::Uuid::parse_str(text).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn secret_refs() {
        assert!(is_valid_secret_ref("connection/0192/secret-key"));
        assert!(is_valid_secret_ref("share-user/alice/password"));
        assert!(!is_valid_secret_ref(""));
        assert!(!is_valid_secret_ref("/abs"));
        assert!(!is_valid_secret_ref("a/../b"));
        assert!(!is_valid_secret_ref("Upper"));
        assert!(!is_valid_secret_ref("trailing/"));
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let issue = ResourceSpec::parse(
            ResourceKind::ShareUser,
            &json!({"username": "alice", "passwordSecret": "share-user/alice", "enabled": true, "x": 1}),
        )
        .unwrap_err();
        assert_eq!(issue.code, "spec.invalid");
    }

    #[test]
    fn parse_round_trips_every_kind() {
        let samples = [
            (
                ResourceKind::Pool,
                json!({"name": "media", "kind": "Lvm", "layers": ["lvm"], "memberDisks": [{"serial": "S1"}]}),
            ),
            (
                ResourceKind::ManagedFolder,
                json!({"name": "Movies", "filedockPath": "/storage/movies", "storageMode": "LocalOnly",
                       "reference": {"poolId": "0192d3c4-0000-7000-8000-000000000001", "relativePath": "/movies"},
                       "access": ["Smb"]}),
            ),
            (
                ResourceKind::Export,
                json!({"folderId": "0192d3c4-0000-7000-8000-000000000002", "protocol": "Nfs",
                       "rules": [{"client": "10.0.0.0/24", "access": "rw"}]}),
            ),
            (
                ResourceKind::ShareUser,
                json!({"username": "alice", "passwordSecret": "share-user/alice/password"}),
            ),
            (
                ResourceKind::CloudConnection,
                json!({"name": "wasabi", "providerFamily": "s3-compatible",
                       "definition": {"type": "s3", "endpoint": "https://s3.wasabisys.com", "region": "us-east-1",
                                      "addressingStyle": "Virtual", "bucket": "b",
                                      "accessKeyRef": "connection/1/access-key", "secretKeyRef": "connection/1/secret-key"}}),
            ),
            (
                ResourceKind::ContainerApp,
                json!({"name": "jellyfin", "compose": "services: {jellyfin: {image: jellyfin/jellyfin:1}}"}),
            ),
            (
                ResourceKind::Extension,
                json!({"subsystemId": "sample", "version": "1.0.0", "packageSha256": "ab".repeat(32),
                       "signature": {"unsigned": {"acknowledged": true}}}),
            ),
            (
                ResourceKind::ApplianceStateDestination,
                json!({"target": {"removableDisk": {"path": "/media/usb"}}}),
            ),
            (
                ResourceKind::Settings,
                json!({"applianceName": "filedock-home"}),
            ),
            (ResourceKind::ConnectorRoutes, json!({"defaults": []})),
        ];
        for (kind, value) in samples {
            let spec =
                ResourceSpec::parse(kind, &value).unwrap_or_else(|e| panic!("{kind}: {e:?}"));
            assert_eq!(spec.kind(), kind);
            let issues = spec.validate();
            assert!(issues.is_empty(), "{kind}: {issues:?}");
            assert_eq!(ResourceSpec::parse(kind, &spec.to_value()).unwrap(), spec);
        }
    }
}
