use serde::{Deserialize, Serialize};

use crate::ValidationIssue;

/// The reserved name of the System Pool (FR-098).
pub const SYSTEM_POOL_NAME: &str = "system";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PoolKind {
    Lvm,
    Adopted,
    System,
}

/// A disk is matched by serial, with the WWN as a second identity when the disk reports one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiskIdentity {
    pub serial: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wwn: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SystemLimits {
    pub max_usage_percent: u32,
    pub reserved_free_bytes: u64,
}

impl SystemLimits {
    pub const MIN_RESERVE_BYTES: u64 = 20 * 1_000_000_000;

    pub const DEFAULT: SystemLimits = SystemLimits {
        max_usage_percent: 50,
        reserved_free_bytes: 100 * 1_000_000_000,
    };
}

/// Pool desired state. Capacity, health, and writability are observed status.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PoolSpec {
    pub name: String,
    pub kind: PoolKind,
    #[serde(default)]
    pub layers: Vec<String>,
    #[serde(default)]
    pub member_disks: Vec<DiskIdentity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_limits: Option<SystemLimits>,
}

impl PoolSpec {
    pub(crate) fn validate(&self, issues: &mut Vec<ValidationIssue>) {
        let valid_name = !self.name.is_empty()
            && self.name.len() <= 32
            && self
                .name
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
        if !valid_name {
            issues.push(ValidationIssue::error(
                "pool.name_invalid",
                "name",
                "must be 1 to 32 characters of a-z, 0-9, and '-'",
            ));
        }
        let is_system = self.kind == PoolKind::System;
        if (self.name == SYSTEM_POOL_NAME) != is_system {
            issues.push(ValidationIssue::error(
                "pool.name_reserved",
                "name",
                "the name 'system' is reserved for the System Pool",
            ));
        }

        match self.kind {
            PoolKind::Lvm if self.member_disks.is_empty() => issues.push(ValidationIssue::error(
                "pool.disks_required",
                "memberDisks",
                "an LVM pool needs at least one disk",
            )),
            PoolKind::Adopted if self.member_disks.len() != 1 => {
                issues.push(ValidationIssue::error(
                    "pool.adopted_single_disk",
                    "memberDisks",
                    "an adopted pool is exactly one disk",
                ))
            }
            PoolKind::System if !self.member_disks.is_empty() => {
                issues.push(ValidationIssue::error(
                    "system_pool.immutable",
                    "memberDisks",
                    "the System Pool always uses the system disk only",
                ))
            }
            _ => {}
        }
        if self.member_disks.iter().any(|d| d.serial.trim().is_empty()) {
            issues.push(ValidationIssue::error(
                "pool.disk_serial_empty",
                "memberDisks",
                "disk serial is empty",
            ));
        }

        match (self.kind, self.system_limits) {
            (PoolKind::System, None) => issues.push(ValidationIssue::error(
                "system_pool.limits_required",
                "systemLimits",
                "the System Pool needs limits",
            )),
            (PoolKind::System, Some(limits)) => {
                if !(10..=90).contains(&limits.max_usage_percent) {
                    issues.push(ValidationIssue::error(
                        "system_pool.max_usage_out_of_range",
                        "systemLimits.maxUsagePercent",
                        "must be between 10 and 90",
                    ));
                }
                if limits.reserved_free_bytes < SystemLimits::MIN_RESERVE_BYTES {
                    issues.push(ValidationIssue::error(
                        "system_pool.reserve_too_small",
                        "systemLimits.reservedFreeBytes",
                        "must be at least 20 GB",
                    ));
                }
            }
            (_, Some(_)) => issues.push(ValidationIssue::error(
                "pool.limits_system_only",
                "systemLimits",
                "only the System Pool has limits",
            )),
            (_, None) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn issues(spec: &PoolSpec) -> Vec<String> {
        let mut out = Vec::new();
        spec.validate(&mut out);
        out.into_iter().map(|i| i.code).collect()
    }

    #[test]
    fn system_pool_rules() {
        let mut spec = PoolSpec {
            name: "system".into(),
            kind: PoolKind::System,
            layers: vec!["image".into()],
            member_disks: vec![],
            system_limits: Some(SystemLimits::DEFAULT),
        };
        assert!(issues(&spec).is_empty());
        spec.system_limits = Some(SystemLimits {
            max_usage_percent: 95,
            reserved_free_bytes: 1,
        });
        assert_eq!(
            issues(&spec),
            [
                "system_pool.max_usage_out_of_range",
                "system_pool.reserve_too_small"
            ]
        );
    }

    #[test]
    fn data_pool_rules() {
        let spec = PoolSpec {
            name: "system".into(),
            kind: PoolKind::Lvm,
            layers: vec!["lvm".into()],
            member_disks: vec![],
            system_limits: None,
        };
        assert_eq!(issues(&spec), ["pool.name_reserved", "pool.disks_required"]);
    }
}
