use crate::{Result, invalid};
use fd_state::{Db, ResourceKind, db};
use serde_json::{Value, json};
pub fn section_for(kind: ResourceKind) -> &'static str {
    match kind {
        ResourceKind::Pool => "pools",
        ResourceKind::ManagedFolder => "folders",
        ResourceKind::Export => "exports",
        ResourceKind::ShareUser => "shareUsers",
        ResourceKind::CloudConnection => "connections",
        ResourceKind::ContainerApp => "apps",
        ResourceKind::Extension => "subsystems",
        ResourceKind::ApplianceStateDestination => "stateDestination",
        ResourceKind::Settings => "settings",
        ResourceKind::ConnectorRoutes => "connectorRoutes",
    }
}
pub fn serialize(db: &Db, appliance_id: &str, management: Value) -> Result<Value> {
    db.with(|conn|{
 let tx=conn.transaction().map_err(fd_state::DbError::from)?;
 let mut core=json!({"pools":[],"folders":[],"exports":[],"shareUsers":[],"connections":[],"apps":[],"subsystems":[],"stateDestination":null,"connectorRoutes":{},"builtinSubsystems":[],"connectors":{"enabled":[]},"ownedLocations":[],"notificationChannels":[],"capabilityDefaults":{},"tierMetadata":null});let mut settings=json!({});
 {
 let mut stmt=tx.prepare("SELECT kind,id,spec FROM resource WHERE deleted=0 ORDER BY kind,id").map_err(fd_state::DbError::from)?;
 let rows=stmt.query_map([],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?))).map_err(fd_state::DbError::from)?;
 for row in rows {let (kind,id,raw)=row.map_err(fd_state::DbError::from)?;let kind=ResourceKind::parse(&kind).ok_or_else(||invalid("fdconfig.kind_unmapped"))?;let mut spec:Value=serde_json::from_str(&raw)?;
  if kind==ResourceKind::Settings{settings=spec;continue;}
  if kind==ResourceKind::ConnectorRoutes||kind==ResourceKind::ApplianceStateDestination {core[section_for(kind)]=spec;continue;}
  let object=spec.as_object_mut().ok_or_else(||invalid("fdconfig.spec_invalid"))?;object.insert("id".into(),json!(id));
  if kind==ResourceKind::Pool {object.remove("memberDisks");}
  core[section_for(kind)].as_array_mut().unwrap().push(spec);
 }
 }
 let exists:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='subsystem')",[],|r|r.get(0)).map_err(fd_state::DbError::from)?;
 if exists {let mut stmt=tx.prepare("SELECT id,enabled,version,approved_permissions,data_bindings,class FROM subsystem ORDER BY id").map_err(fd_state::DbError::from)?;
 for row in stmt.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,bool>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,String>(4)?,r.get::<_,String>(5)?))).map_err(fd_state::DbError::from)? {
 let(id,enabled,version,permissions,bindings,class)=row.map_err(fd_state::DbError::from)?;
 if class=="BuiltIn"{core["builtinSubsystems"].as_array_mut().unwrap().push(json!({"subsystemId":id,"enabled":enabled,"version":version,"approvedPermissions":serde_json::from_str::<Value>(&permissions)?,"dataBindings":serde_json::from_str::<Value>(&bindings)?}));}
 if enabled&&["fd-s3","rclone"].contains(&id.as_str()){core["connectors"]["enabled"].as_array_mut().unwrap().push(json!(id));}
 }
 }
 for(key,field)in[("owned_locations","ownedLocations"),("notification_channels","notificationChannels"),("capability_defaults","capabilityDefaults"),("tier_metadata","tierMetadata")]{if let Some(value)=db::get_setting(&tx,key)?{core[field]=value;}}
 core["notificationChannels"]=settings.get("notificationChannels").cloned().unwrap_or(json!([]));
 let name=settings["applianceName"].as_str().unwrap_or("filedock").to_owned();
 let doc=json!({"schema":"fdconfig/1","appliance":{"id":appliance_id,"name":name,"settings":settings},"core":core,"management":management});tx.commit().map_err(fd_state::DbError::from)?;Ok(doc)
})
}
