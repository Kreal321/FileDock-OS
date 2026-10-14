//! Generic capability selection. Cloud selection is deliberately left to connector routes.
use crate::{
    SubsystemRegistry,
    package::{Result, invalid},
};
use fd_state::{Db, db};
use serde_json::json;
use std::collections::BTreeMap;
pub fn defaults(db: &Db) -> Result<BTreeMap<String, String>> {
    db.with(|c| {
        let value = db::get_setting(c, "capability_defaults")?.unwrap_or(json!({}));
        Ok::<_, fd_state::DbError>(serde_json::from_value(value)?)
    })
    .map_err(|_| invalid("capability.defaults_invalid"))
}
pub fn resolve(registry: &SubsystemRegistry, capability: &str) -> Result<String> {
    if matches!(capability, "cloud.remote" | "cloud.mount") {
        return Err(invalid("capability.requires_connector_route"));
    }
    let providers = registry
        .list()
        .map_err(|_| invalid("capability.registry_unavailable"))?
        .into_iter()
        .filter(|s| {
            s.enabled
                && s.state == "Running"
                && registry
                    .effective_capabilities(s)
                    .is_ok_and(|caps| caps.iter().any(|c| c == capability))
        })
        .map(|s| s.id)
        .collect::<Vec<_>>();
    let selected = defaults(registry.db())?.get(capability).cloned();
    if let Some(selected) = selected {
        if providers.contains(&selected) {
            return Ok(selected);
        }
        return Err(invalid("capability.default_unavailable"));
    }
    match providers.as_slice() {
        [one] => Ok(one.clone()),
        [] => Err(invalid("capability.missing")),
        _ => Err(invalid("capability.default_required")),
    }
}
pub fn set_default(registry: &SubsystemRegistry, capability: &str, id: &str) -> Result<()> {
    if matches!(capability, "cloud.remote" | "cloud.mount") {
        return Err(invalid("capability.requires_connector_route"));
    }
    let row = registry
        .get(id)
        .map_err(|_| invalid("capability.registry_unavailable"))?
        .ok_or_else(|| invalid("capability.provider_missing"))?;
    if !row.enabled
        || !row.manifest["capabilities"]
            .as_array()
            .is_some_and(|caps| caps.iter().any(|c| c == capability))
    {
        return Err(invalid("capability.provider_incapable"));
    }
    let mut map = defaults(registry.db())?;
    map.insert(capability.into(), id.into());
    registry
        .db()
        .with(|c| {
            let tx = c.transaction()?;
            db::put_setting(&tx, "capability_defaults", &json!(map))?;
            db::put_setting(&tx, "config.changed_at", &json!(db::now_text()))?;
            tx.commit()?;
            Ok::<_, fd_state::DbError>(())
        })
        .map_err(|_| invalid("capability.defaults_invalid"))
}
