//! Pure route preview: no provider I/O or credentials are read here.
use crate::{
    ownership::{ClaimSafety, OwnedLocation},
    routes,
};
use fd_state::resources::{
    CloudConnectionSpec, ConnectorRoutesSpec, ResourceSpec, RouteDefault, RouteTarget, RouteUse,
};
use fd_state::{StoredResource, ValidationIssue};
use fd_subsystems::SubsystemRecord;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Change {
    pub family: String,
    pub use_kind: RouteUse,
    #[serde(default)]
    pub connection_id: String,
    pub target: Option<RouteTarget>,
}
#[derive(Default)]
pub struct Impact {
    pub connections: BTreeSet<String>,
    pub folders: BTreeSet<String>,
    pub locations: Vec<OwnedLocation>,
    pub lost_features: BTreeSet<String>,
    pub recipients: BTreeMap<String, BTreeSet<String>>,
    pub mounts: BTreeSet<String>,
    pub basic_ack: bool,
    pub third_party_ack: bool,
    pub issues: Vec<ValidationIssue>,
}
pub struct Preview {
    pub routes: ConnectorRoutesSpec,
    pub connections: BTreeMap<String, CloudConnectionSpec>,
    pub impact: Impact,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn connector(id: &str, enabled: bool, conditional: bool, class: &str) -> SubsystemRecord {
        SubsystemRecord {
            id: id.into(),
            class: class.into(),
            enabled,
            state: "Running".into(),
            version: "1".into(),
            manifest: json!({"cloud":[{"family":"s3-compatible","capabilities":["cloud.remote"],"conditionalWrites":conditional,"providerChecksums":conditional}]}),
        }
    }
    fn connection(id: &str, overrides: serde_json::Value) -> StoredResource {
        StoredResource {
            resource: fd_state::ResourceRef::new(fd_state::ResourceKind::CloudConnection, id),
            generation: 1,
            spec: json!({"name":id,"providerFamily":"s3-compatible","definition":{"type":"s3","endpoint":"https://example.invalid","region":"us-east-1","bucket":"test","addressingStyle":"Path","accessKeyRef":"access","secretKeyRef":"secret"},"routeOverrides":overrides}),
            status: None,
            deleted: false,
            change_seq: 1,
            created_at: "now".into(),
            updated_at: "now".into(),
        }
    }
    #[test]
    fn family_change_preserves_overrides_and_only_new_recipients_are_reported() {
        let first = connection("first", json!({}));
        let second = connection(
            "second",
            json!({"ownedObjectStorage":{"type":"connector","subsystem_id":"fd-s3"}}),
        );
        let records = [
            connector("fd-s3", true, true, "BuiltIn"),
            connector("other", true, false, "Extension"),
        ];
        let p = preview(
            &ConnectorRoutesSpec::seeded(),
            &[first, second],
            &[],
            &[],
            &records,
            &[Change {
                family: "s3-compatible".into(),
                use_kind: RouteUse::OwnedObjectStorage,
                connection_id: String::new(),
                target: Some(RouteTarget::Connector {
                    subsystem_id: "other".into(),
                }),
            }],
        );
        assert!(p.impact.issues.is_empty());
        assert_eq!(p.impact.connections, BTreeSet::from(["first".into()]));
        assert_eq!(
            p.impact.recipients["other"],
            BTreeSet::from(["first".into()])
        );
        assert_eq!(
            routes::resolve(
                &p.routes,
                Some(&p.connections["second"]),
                "s3-compatible",
                RouteUse::OwnedObjectStorage
            )
            .unwrap(),
            RouteTarget::Connector {
                subsystem_id: "fd-s3".into()
            }
        );
    }
    #[test]
    fn removing_override_previews_inherited_target_without_changing_other_connections() {
        let first = connection(
            "first",
            json!({"ownedObjectStorage":{"type":"connector","subsystem_id":"other"}}),
        );
        let second = connection("second", json!({}));
        let p = preview(
            &ConnectorRoutesSpec::seeded(),
            &[first, second],
            &[],
            &[],
            &[
                connector("fd-s3", true, true, "BuiltIn"),
                connector("other", true, false, "Extension"),
            ],
            &[Change {
                family: "s3-compatible".into(),
                use_kind: RouteUse::OwnedObjectStorage,
                connection_id: "first".into(),
                target: None,
            }],
        );
        assert!(p.impact.issues.is_empty());
        assert_eq!(p.impact.connections, BTreeSet::from(["first".into()]));
        assert!(p.connections["first"].route_overrides.is_empty());
    }
    #[test]
    fn disabled_and_mount_incapable_targets_are_refused() {
        for (enabled, usage, code) in [
            (
                false,
                RouteUse::OwnedObjectStorage,
                "route.connector_disabled",
            ),
            (true, RouteUse::CloudMount, "route.connector_incapable"),
        ] {
            let p = preview(
                &ConnectorRoutesSpec::seeded(),
                &[],
                &[],
                &[],
                &[connector("other", enabled, false, "Extension")],
                &[Change {
                    family: "s3-compatible".into(),
                    use_kind: usage,
                    connection_id: String::new(),
                    target: Some(RouteTarget::Connector {
                        subsystem_id: "other".into(),
                    }),
                }],
            );
            assert!(p.impact.issues.iter().any(|i| i.code == code));
        }
    }
    #[test]
    fn third_party_owned_routes_require_separate_acknowledgements() {
        let original = ConnectorRoutesSpec::seeded();
        let p = preview(
            &original,
            &[],
            &[],
            &[],
            &[connector("other", true, false, "Extension")],
            &[Change {
                family: "s3-compatible".into(),
                use_kind: RouteUse::OwnedObjectStorage,
                connection_id: String::new(),
                target: Some(RouteTarget::Connector {
                    subsystem_id: "other".into(),
                }),
            }],
        );
        assert!(p.impact.basic_ack && p.impact.third_party_ack);
        assert_eq!(original, ConnectorRoutesSpec::seeded());
    }
    #[test]
    fn null_default_and_duplicate_changes_are_refused() {
        let c = Change {
            family: "s3-compatible".into(),
            use_kind: RouteUse::OwnedObjectStorage,
            connection_id: String::new(),
            target: None,
        };
        let p = preview(
            &ConnectorRoutesSpec::seeded(),
            &[],
            &[],
            &[],
            &[],
            &[c.clone(), c],
        );
        assert!(
            p.impact
                .issues
                .iter()
                .any(|i| i.code == "route.default_required")
        );
        assert!(
            p.impact
                .issues
                .iter()
                .any(|i| i.code == "route.change_duplicate")
        );
    }
}

pub fn preview(
    original: &ConnectorRoutesSpec,
    rows: &[StoredResource],
    folders: &[StoredResource],
    locations: &[OwnedLocation],
    connectors: &[SubsystemRecord],
    changes: &[Change],
) -> Preview {
    let mut p = Preview {
        routes: original.clone(),
        connections: rows
            .iter()
            .filter_map(|r| match r.parsed_spec().ok()? {
                ResourceSpec::CloudConnection(c) => Some((r.resource.id.clone(), c)),
                _ => None,
            })
            .collect(),
        impact: Impact::default(),
    };
    let mut seen = BTreeSet::new();
    for c in changes {
        if !seen.insert((c.family.clone(), c.use_kind, c.connection_id.clone())) {
            p.impact.issues.push(ValidationIssue::error(
                "route.change_duplicate",
                "changes",
                "A route may be changed only once in a batch",
            ));
            continue;
        }
        if !c.use_kind.allowed_for_family(&c.family) {
            p.impact.issues.push(ValidationIssue::error(
                "route.use_not_allowed",
                "use",
                "This provider does not support this use",
            ));
            continue;
        }
        let connection = if c.connection_id.is_empty() {
            None
        } else {
            match p.connections.get(&c.connection_id) {
                Some(v) if v.provider_family == c.family => Some(v),
                _ => {
                    p.impact.issues.push(ValidationIssue::error(
                        "route.connection_mismatch",
                        "connectionId",
                        "The connection must exist and belong to this family",
                    ));
                    continue;
                }
            }
        };
        if let Some(t) = &c.target {
            if let Err(routes::RouteError::Invalid(issue)) = routes::validate_target(
                &c.family,
                c.use_kind,
                t,
                connection,
                connectors,
                connectors.iter().find(|x| x.id == "kopia"),
            ) {
                p.impact.issues.push(issue);
                continue;
            }
        } else if c.connection_id.is_empty() {
            p.impact.issues.push(ValidationIssue::error(
                "route.default_required",
                "target",
                "Family defaults cannot be removed",
            ));
            continue;
        }
        if c.connection_id.is_empty() {
            let t = c.target.clone().unwrap();
            if let Some(d) = p
                .routes
                .defaults
                .iter_mut()
                .find(|d| d.family == c.family && d.use_kind == c.use_kind)
            {
                d.target = t.clone()
            } else {
                p.routes.defaults.push(RouteDefault {
                    family: c.family.clone(),
                    use_kind: c.use_kind,
                    target: t.clone(),
                });
            }
            if c.use_kind == RouteUse::OwnedObjectStorage {
                if !feature(&t, &c.family, connectors, "conditionalWrites") {
                    p.impact.basic_ack = true;
                }
                if third_party(&t, connectors) {
                    p.impact.third_party_ack = true;
                }
            }
        } else {
            let v = p.connections.get_mut(&c.connection_id).unwrap();
            if let Some(t) = &c.target {
                v.route_overrides.insert(c.use_kind, t.clone());
            } else {
                v.route_overrides.remove(&c.use_kind);
            }
        }
    }
    let mut owned_changed = BTreeSet::new();
    for row in rows {
        let Ok(ResourceSpec::CloudConnection(old)) = row.parsed_spec() else {
            continue;
        };
        let Some(new) = p.connections.get(&row.resource.id) else {
            continue;
        };
        for usage in RouteUse::ALL {
            if !usage.allowed_for_family(&old.provider_family) {
                continue;
            }
            let before = routes::resolve(original, Some(&old), &old.provider_family, usage).ok();
            let after = routes::resolve(&p.routes, Some(new), &new.provider_family, usage).ok();
            if before == after {
                continue;
            }
            p.impact.connections.insert(row.resource.id.clone());
            let Some(target) = after else {
                p.impact.issues.push(ValidationIssue::error(
                    "route.missing",
                    "target",
                    "Removing the override requires a valid family default",
                ));
                continue;
            };
            if let Err(routes::RouteError::Invalid(issue)) = routes::validate_target(
                &new.provider_family,
                usage,
                &target,
                Some(new),
                connectors,
                connectors.iter().find(|x| x.id == "kopia"),
            ) {
                p.impact.issues.push(issue);
            }
            for (flag, label) in [
                ("conditionalWrites", "conditional_writes"),
                ("providerChecksums", "provider_checksums"),
            ] {
                if before
                    .as_ref()
                    .is_some_and(|t| feature(t, &old.provider_family, connectors, flag))
                    && !feature(&target, &old.provider_family, connectors, flag)
                {
                    p.impact.lost_features.insert(label.into());
                }
            }
            if usage == RouteUse::OwnedObjectStorage {
                owned_changed.insert(row.resource.id.clone());
                if matches!(claim_safety(row, &target, connectors), ClaimSafety::Basic) {
                    p.impact.basic_ack = true;
                }
                if third_party(&target, connectors) {
                    p.impact.third_party_ack = true;
                }
            }
            for f in folders {
                if references(&f.spec, &row.resource.id) {
                    p.impact.folders.insert(f.resource.id.clone());
                    if usage == RouteUse::CloudMount && f.spec["storageMode"] == "CloudMount" {
                        p.impact.mounts.insert(f.resource.id.clone());
                    }
                }
            }
        }
        let used = |routes: &ConnectorRoutesSpec, c: &CloudConnectionSpec| -> BTreeSet<String> {
            RouteUse::ALL
                .into_iter()
                .filter(|u| u.allowed_for_family(&c.provider_family))
                .filter_map(|u| routes::resolve(routes, Some(c), &c.provider_family, u).ok())
                .map(|t| t.subsystem_id().unwrap_or("backup-engine").to_owned())
                .collect()
        };
        let previous = used(original, &old);
        for id in used(&p.routes, new).difference(&previous) {
            p.impact
                .recipients
                .entry(id.clone())
                .or_default()
                .insert(row.resource.id.clone());
        }
    }
    for loc in locations
        .iter()
        .filter(|l| owned_changed.contains(&l.connection_id))
    {
        let mut next = loc.clone();
        if let Some(c) = p.connections.get(&loc.connection_id)
            && let Ok(t) = routes::resolve(
                &p.routes,
                Some(c),
                &c.provider_family,
                RouteUse::OwnedObjectStorage,
            )
        {
            next.served_by_connector = t.subsystem_id().unwrap_or("backup-engine").into();
            next.served_by_third_party = third_party(&t, connectors);
            if let Some(row) = rows.iter().find(|r| r.resource.id == loc.connection_id) {
                next.claim_safety = claim_safety(row, &t, connectors);
            }
        }
        p.impact.locations.push(next);
    }
    p
}
fn references(value: &serde_json::Value, id: &str) -> bool {
    match value {
        serde_json::Value::Object(o) => {
            o.get("connectionId").is_some_and(|v| v == id) || o.values().any(|v| references(v, id))
        }
        serde_json::Value::Array(a) => a.iter().any(|v| references(v, id)),
        _ => false,
    }
}
pub fn feature(
    target: &RouteTarget,
    family: &str,
    connectors: &[SubsystemRecord],
    flag: &str,
) -> bool {
    target
        .subsystem_id()
        .and_then(|id| connectors.iter().find(|c| c.id == id))
        .is_some_and(|c| {
            c.manifest["cloud"]
                .as_array()
                .is_some_and(|a| a.iter().any(|f| f["family"] == family && f[flag] == true))
        })
}
pub fn third_party(target: &RouteTarget, connectors: &[SubsystemRecord]) -> bool {
    target
        .subsystem_id()
        .and_then(|id| connectors.iter().find(|c| c.id == id))
        .is_some_and(|c| c.class == "Extension")
}
pub fn claim_safety(
    row: &StoredResource,
    target: &RouteTarget,
    connectors: &[SubsystemRecord],
) -> ClaimSafety {
    let family = row.spec["providerFamily"].as_str().unwrap_or("");
    let id = target.subsystem_id().unwrap_or("");
    let status = row.status.as_ref();
    let test = status
        .and_then(|s| s.get("testsByConnector"))
        .and_then(|s| s.get(id))
        .or_else(|| status.and_then(|s| s.get("lastTest")))
        .and_then(|v| serde_json::from_value::<crate::test::StoredTestResult>(v.clone()).ok())
        .filter(|p| p.connector_id == id);
    if feature(target, family, connectors, "conditionalWrites")
        && test.is_some_and(|p| p.proves_conditional())
    {
        ClaimSafety::Conditional
    } else {
        ClaimSafety::Basic
    }
}
