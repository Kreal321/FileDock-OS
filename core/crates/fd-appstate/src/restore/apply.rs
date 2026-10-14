//! Import only fully verified configuration into a fresh Core. Disk provisioning is delegated.
use crate::{Result, fdconfig::section_for, invalid};
use fd_state::{Db, ResourceKind, db, resources::ResourceSpec};
use rusqlite::params;
use serde_json::{Value, json};
pub fn import_config(
    db: &Db,
    document: &Value,
    operation_id: &str,
    present: &[String],
) -> Result<()> {
    crate::fdconfig::validate(document)?;
    let identity = document["appliance"]["id"]
        .as_str()
        .ok_or_else(|| invalid("restore.identity_invalid"))?;
    if uuid::Uuid::parse_str(identity).is_err() {
        return Err(invalid("restore.identity_invalid"));
    }
    let mut resources = Vec::new();
    for kind in ResourceKind::ALL {
        let field = section_for(kind);
        let values = match kind {
            ResourceKind::Settings => vec![document["appliance"]["settings"].clone()],
            ResourceKind::ConnectorRoutes => vec![document["core"][field].clone()],
            ResourceKind::ApplianceStateDestination => {
                if document["core"][field].is_null() {
                    vec![]
                } else {
                    vec![document["core"][field].clone()]
                }
            }
            _ => document["core"][field]
                .as_array()
                .cloned()
                .unwrap_or_default(),
        };
        for mut spec in values {
            let object = spec
                .as_object_mut()
                .ok_or_else(|| invalid("fdconfig.spec_invalid"))?;
            let id = if kind.is_singleton() {
                "default".to_owned()
            } else {
                object
                    .remove("id")
                    .and_then(|v| v.as_str().map(str::to_owned))
                    .ok_or_else(|| invalid("fdconfig.identity_invalid"))?
            };
            if !kind.is_singleton() && uuid::Uuid::parse_str(&id).is_err() {
                return Err(invalid("fdconfig.identity_invalid"));
            }
            if kind == ResourceKind::Pool {
                object.insert("memberDisks".into(), json!([]));
            }
            let parsed =
                ResourceSpec::parse(kind, &spec).map_err(|_| invalid("fdconfig.spec_invalid"))?;
            let issues = parsed.validate();
            if kind != ResourceKind::Pool && issues.iter().any(|i| i.is_error()) {
                return Err(invalid("fdconfig.validation_failed"));
            }
            resources.push((kind, id, spec));
        }
    }
    db.with(|c|{let tx=c.transaction().map_err(fd_state::DbError::from)?;
 let done=db::get_setting(&tx,"restore.imported_operation")?.as_ref().and_then(Value::as_str).is_some_and(|id|id==operation_id);if done{return Ok(());}
 let has_user:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM resource WHERE deleted=0 AND kind NOT IN ('Settings','ConnectorRoutes'))",[],|r|r.get(0)).map_err(fd_state::DbError::from)?;
 if has_user{return Err(invalid("restore.target_not_fresh"));}
 tx.execute("DELETE FROM resource",[]).map_err(fd_state::DbError::from)?;let now=db::now_text();let mut seq=0;
 for(kind,id,spec)in &resources{seq+=1;let status=if *kind==ResourceKind::Pool&&!present.contains(id){json!({"state":"Unavailable","detail":"Awaiting disk assignment and storage capability provisioning"})}else{json!({"state":"Pending","detail":"Restored; awaiting reconciliation"})};tx.execute("INSERT INTO resource(kind,id,generation,spec,status,deleted,change_seq,created_at,updated_at) VALUES(?1,?2,1,?3,?4,0,?5,?6,?6)",params![kind.as_str(),id,spec.to_string(),status.to_string(),seq,now]).map_err(fd_state::DbError::from)?;}
 tx.execute("UPDATE state_meta SET change_seq=?1 WHERE id=1",[seq]).map_err(fd_state::DbError::from)?;db::put_setting(&tx,"appliance_id",&json!(identity))?;db::put_setting(&tx,"config.changed_at",&json!(now))?;
 for(field,key)in[("ownedLocations","owned_locations"),("notificationChannels","notification_channels"),("capabilityDefaults","capability_defaults"),("tierMetadata","tier_metadata")]{let mut value=document["core"][field].clone();if field=="ownedLocations"{for location in value.as_array_mut().into_iter().flatten(){location["role"]=json!("RecoveryReader");}}db::put_setting(&tx,key,&value)?;}
 let subsystem_table:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='subsystem')",[],|r|r.get(0)).map_err(fd_state::DbError::from)?;
 if subsystem_table{for builtin in document["core"]["builtinSubsystems"].as_array().into_iter().flatten(){let id=builtin["subsystemId"].as_str().ok_or_else(||invalid("restore.subsystem_invalid"))?;let enabled=builtin["enabled"].as_bool().ok_or_else(||invalid("restore.subsystem_invalid"))?;tx.execute("UPDATE subsystem SET enabled=?1 WHERE id=?2 AND class='BuiltIn'",params![enabled,id]).map_err(fd_state::DbError::from)?;}}
 db::put_setting(&tx,"restore.management",&document["management"])?;db::put_setting(&tx,"restore.imported_operation",&json!(operation_id))?;
 tx.commit().map_err(fd_state::DbError::from)?;Ok(())
 })
}
