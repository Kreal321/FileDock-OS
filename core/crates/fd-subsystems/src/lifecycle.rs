//! Validation before journaled lifecycle side effects.
use crate::{
    package::{Package, Result, invalid},
    sandbox::{Binding, Sandbox},
};
use std::collections::{BTreeMap, BTreeSet};
pub fn approve(
    package: &Package,
    permissions: &[String],
    unsigned: bool,
    data: &BTreeMap<String, String>,
    folders: &BTreeMap<String, Binding>,
    unprotected: bool,
) -> Result<Sandbox> {
    let requested = package.permissions().into_iter().collect::<BTreeSet<_>>();
    let approved = permissions.iter().cloned().collect::<BTreeSet<_>>();
    if requested != approved || permissions.len() != approved.len() {
        return Err(invalid("extension.permission_approval_required"));
    }
    if !package.signed && !unsigned {
        return Err(invalid("extension.unsigned_ack_required"));
    }
    let bindings = data
        .iter()
        .map(|(name, id)| {
            folders
                .get(id)
                .cloned()
                .map(|b| (name.clone(), b))
                .ok_or_else(|| invalid("extension.folder_missing"))
        })
        .collect::<Result<BTreeMap<_, _>>>()?;
    if bindings.values().any(|b| !b.protected) && !unprotected {
        return Err(invalid("extension.data_unprotected"));
    }
    Sandbox::derive(&package.manifest, &bindings)
}
