//! `core.db` and the desired-state store: typed resources, `ApplyChange`, and `ValidateChange`.

pub mod apply;
pub mod db;
pub mod resources;

use std::fmt;

use serde::{Deserialize, Serialize};

pub use apply::{
    ApplyError, ApplyOutcome, Change, ChangeOrigin, CommitNotice, StateStore, StateView,
    StoredResource, Validator, ValidatorRegistry,
};
pub use db::{Db, DbError};

/// Desired-state resource kinds. The names match `filedock.core.v1.ResourceKind`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ResourceKind {
    Pool,
    ManagedFolder,
    Export,
    ShareUser,
    CloudConnection,
    ContainerApp,
    Extension,
    ApplianceStateDestination,
    Settings,
    ConnectorRoutes,
}

/// The id every singleton resource is stored under.
pub const SINGLETON_ID: &str = "default";

impl ResourceKind {
    pub const ALL: [ResourceKind; 10] = [
        ResourceKind::Pool,
        ResourceKind::ManagedFolder,
        ResourceKind::Export,
        ResourceKind::ShareUser,
        ResourceKind::CloudConnection,
        ResourceKind::ContainerApp,
        ResourceKind::Extension,
        ResourceKind::ApplianceStateDestination,
        ResourceKind::Settings,
        ResourceKind::ConnectorRoutes,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ResourceKind::Pool => "Pool",
            ResourceKind::ManagedFolder => "ManagedFolder",
            ResourceKind::Export => "Export",
            ResourceKind::ShareUser => "ShareUser",
            ResourceKind::CloudConnection => "CloudConnection",
            ResourceKind::ContainerApp => "ContainerApp",
            ResourceKind::Extension => "Extension",
            ResourceKind::ApplianceStateDestination => "ApplianceStateDestination",
            ResourceKind::Settings => "Settings",
            ResourceKind::ConnectorRoutes => "ConnectorRoutes",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == text)
    }

    /// Singletons always live under [`SINGLETON_ID`]: one remote destination (FR-037), one settings
    /// document, and one route table.
    pub fn is_singleton(self) -> bool {
        matches!(
            self,
            ResourceKind::ApplianceStateDestination
                | ResourceKind::Settings
                | ResourceKind::ConnectorRoutes
        )
    }
}

impl fmt::Display for ResourceKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ResourceRef {
    pub kind: ResourceKind,
    pub id: String,
}

impl ResourceRef {
    pub fn new(kind: ResourceKind, id: impl Into<String>) -> Self {
        Self {
            kind,
            id: id.into(),
        }
    }

    pub fn singleton(kind: ResourceKind) -> Self {
        Self::new(kind, SINGLETON_ID)
    }
}

impl fmt::Display for ResourceRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.kind, self.id)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    Error,
    Warning,
}

/// Mirrors `filedock.core.v1.ValidationIssue`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationIssue {
    pub severity: Severity,
    pub code: String,
    pub field: String,
    pub message: String,
    pub conflicting: Option<ResourceRef>,
}

impl ValidationIssue {
    pub fn error(code: &str, field: &str, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Error,
            code: code.to_owned(),
            field: field.to_owned(),
            message: message.into(),
            conflicting: None,
        }
    }

    pub fn warning(code: &str, field: &str, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Warning,
            ..Self::error(code, field, message)
        }
    }

    pub fn with_conflicting(mut self, conflicting: ResourceRef) -> Self {
        self.conflicting = Some(conflicting);
        self
    }

    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }

    /// Prefixes the field path, so a nested validator's issues point into the enclosing spec.
    pub fn under(mut self, prefix: &str) -> Self {
        self.field = if self.field.is_empty() {
            prefix.to_owned()
        } else {
            format!("{prefix}.{}", self.field)
        };
        self
    }
}

/// A new resource identity: UUIDv7, time-ordered and stable across restore.
pub fn new_id() -> String {
    uuid::Uuid::now_v7().to_string()
}
