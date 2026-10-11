use serde::{Deserialize, Serialize};

use crate::ValidationIssue;

/// What a routed connector is asked to do (research R13).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RouteUse {
    OwnedObjectStorage,
    BackupRepository,
    CloudMount,
}

impl RouteUse {
    pub const ALL: [RouteUse; 3] = [
        RouteUse::OwnedObjectStorage,
        RouteUse::BackupRepository,
        RouteUse::CloudMount,
    ];

    pub fn required_capability(self) -> &'static str {
        match self {
            RouteUse::CloudMount => "cloud.mount",
            RouteUse::OwnedObjectStorage | RouteUse::BackupRepository => "cloud.remote",
        }
    }

    /// `OwnedObjectStorage` and `BackupRepository` are object-storage families only (FR-088).
    pub fn allowed_for_family(self, family: &str) -> bool {
        match self {
            RouteUse::CloudMount => true,
            RouteUse::OwnedObjectStorage | RouteUse::BackupRepository => {
                super::connection::is_object_storage_family(family)
            }
        }
    }
}

/// Where I/O for a (family, use) is sent.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum RouteTarget {
    Connector { subsystem_id: String },
    BackupEngine,
}

impl RouteTarget {
    pub fn fd_s3() -> Self {
        Self::Connector {
            subsystem_id: "fd-s3".into(),
        }
    }

    pub fn rclone() -> Self {
        Self::Connector {
            subsystem_id: "rclone".into(),
        }
    }

    pub fn subsystem_id(&self) -> Option<&str> {
        match self {
            Self::Connector { subsystem_id } => Some(subsystem_id),
            Self::BackupEngine => None,
        }
    }

    pub fn validate_for(
        &self,
        use_kind: RouteUse,
        bound_connector: Option<&str>,
    ) -> Result<(), ValidationIssue> {
        match self {
            Self::BackupEngine if use_kind != RouteUse::BackupRepository => {
                Err(ValidationIssue::error(
                    "route.use_not_allowed",
                    "target",
                    "backup-engine is only allowed for BackupRepository (FR-116)",
                ))
            }
            Self::Connector { subsystem_id } if subsystem_id.is_empty() => {
                Err(ValidationIssue::error(
                    "route.connector_required",
                    "target.subsystemId",
                    "a connector target needs a subsystem id",
                ))
            }
            Self::Connector { subsystem_id } => {
                if let Some(bound) = bound_connector
                    && bound != subsystem_id
                {
                    return Err(ValidationIssue::error(
                        "route.bound_mismatch",
                        "target.subsystemId",
                        "a bound connection can only target its own connector",
                    ));
                }
                Ok(())
            }
            Self::BackupEngine => Ok(()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RouteDefault {
    pub family: String,
    #[serde(rename = "use")]
    pub use_kind: RouteUse,
    pub target: RouteTarget,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AcknowledgementKind {
    BasicClaimSafety,
    ThirdPartyOwnedStorage,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Acknowledgement {
    pub kind: AcknowledgementKind,
    pub scope: String,
    pub audit_entry_id: String,
}

/// Singleton route table. `generation` lives on the resource row and advances on every change.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConnectorRoutesSpec {
    #[serde(default)]
    pub defaults: Vec<RouteDefault>,
    #[serde(default)]
    pub acknowledgements: Vec<Acknowledgement>,
}

impl ConnectorRoutesSpec {
    /// Family defaults from research R13, seeded on first boot.
    pub fn seeded() -> Self {
        use RouteTarget::{BackupEngine, Connector};
        use RouteUse::{BackupRepository, CloudMount, OwnedObjectStorage};
        let rclone = || Connector {
            subsystem_id: "rclone".into(),
        };
        let fd_s3 = || Connector {
            subsystem_id: "fd-s3".into(),
        };
        Self {
            defaults: vec![
                RouteDefault {
                    family: "s3-compatible".into(),
                    use_kind: OwnedObjectStorage,
                    target: fd_s3(),
                },
                RouteDefault {
                    family: "s3-compatible".into(),
                    use_kind: BackupRepository,
                    target: BackupEngine,
                },
                RouteDefault {
                    family: "s3-compatible".into(),
                    use_kind: CloudMount,
                    target: rclone(),
                },
                RouteDefault {
                    family: "azure-blob".into(),
                    use_kind: OwnedObjectStorage,
                    target: rclone(),
                },
                RouteDefault {
                    family: "azure-blob".into(),
                    use_kind: BackupRepository,
                    target: BackupEngine,
                },
                RouteDefault {
                    family: "azure-blob".into(),
                    use_kind: CloudMount,
                    target: rclone(),
                },
                RouteDefault {
                    family: "google-drive".into(),
                    use_kind: CloudMount,
                    target: rclone(),
                },
                RouteDefault {
                    family: "onedrive".into(),
                    use_kind: CloudMount,
                    target: rclone(),
                },
                RouteDefault {
                    family: "dropbox".into(),
                    use_kind: CloudMount,
                    target: rclone(),
                },
            ],
            acknowledgements: Vec::new(),
        }
    }

    pub fn resolve(&self, family: &str, use_kind: RouteUse) -> Option<&RouteTarget> {
        self.defaults
            .iter()
            .find(|row| row.family == family && row.use_kind == use_kind)
            .map(|row| &row.target)
    }

    pub(crate) fn validate(&self, issues: &mut Vec<ValidationIssue>) {
        let mut seen = std::collections::BTreeSet::new();
        for (index, row) in self.defaults.iter().enumerate() {
            let field = format!("defaults[{index}]");
            if !super::connection::is_valid_family(&row.family) {
                issues.push(ValidationIssue::error(
                    "route.family_invalid",
                    &format!("{field}.family"),
                    "unknown provider family",
                ));
            }
            if !row.use_kind.allowed_for_family(&row.family) {
                issues.push(ValidationIssue::error(
                    "route.use_not_allowed",
                    &field,
                    format!("{:?} is not allowed for {}", row.use_kind, row.family),
                ));
            }
            if let Err(issue) = row.target.validate_for(row.use_kind, None) {
                issues.push(issue.under(&field));
            }
            if !seen.insert((row.family.clone(), row.use_kind)) {
                issues.push(ValidationIssue::error(
                    "route.default_duplicate",
                    &field,
                    "a family has two defaults for the same use",
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeded_defaults_are_valid() {
        let spec = ConnectorRoutesSpec::seeded();
        let mut issues = Vec::new();
        spec.validate(&mut issues);
        assert!(issues.is_empty(), "{issues:?}");
        assert_eq!(
            spec.resolve("s3-compatible", RouteUse::BackupRepository),
            Some(&RouteTarget::BackupEngine)
        );
        assert_eq!(
            spec.resolve("s3-compatible", RouteUse::OwnedObjectStorage)
                .and_then(RouteTarget::subsystem_id),
            Some("fd-s3")
        );
    }
}
