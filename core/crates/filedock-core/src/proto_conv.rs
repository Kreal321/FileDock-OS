use fd_api::core::v1 as pb;
use fd_state::{ResourceKind, ResourceRef, Severity, StoredResource, ValidationIssue};
use serde_json::Value;

pub fn kind_from_proto(kind: i32) -> Result<ResourceKind, tonic::Status> {
    match pb::ResourceKind::try_from(kind).ok() {
        Some(pb::ResourceKind::Pool) => Ok(ResourceKind::Pool),
        Some(pb::ResourceKind::ManagedFolder) => Ok(ResourceKind::ManagedFolder),
        Some(pb::ResourceKind::Export) => Ok(ResourceKind::Export),
        Some(pb::ResourceKind::ShareUser) => Ok(ResourceKind::ShareUser),
        Some(pb::ResourceKind::CloudConnection) => Ok(ResourceKind::CloudConnection),
        Some(pb::ResourceKind::ContainerApp) => Ok(ResourceKind::ContainerApp),
        Some(pb::ResourceKind::Extension) => Ok(ResourceKind::Extension),
        Some(pb::ResourceKind::ApplianceStateDestination) => {
            Ok(ResourceKind::ApplianceStateDestination)
        }
        Some(pb::ResourceKind::Settings) => Ok(ResourceKind::Settings),
        Some(pb::ResourceKind::ConnectorRoutes) => Ok(ResourceKind::ConnectorRoutes),
        _ => Err(tonic::Status::invalid_argument("unknown resource kind")),
    }
}

pub fn kind_to_proto(kind: ResourceKind) -> pb::ResourceKind {
    match kind {
        ResourceKind::Pool => pb::ResourceKind::Pool,
        ResourceKind::ManagedFolder => pb::ResourceKind::ManagedFolder,
        ResourceKind::Export => pb::ResourceKind::Export,
        ResourceKind::ShareUser => pb::ResourceKind::ShareUser,
        ResourceKind::CloudConnection => pb::ResourceKind::CloudConnection,
        ResourceKind::ContainerApp => pb::ResourceKind::ContainerApp,
        ResourceKind::Extension => pb::ResourceKind::Extension,
        ResourceKind::ApplianceStateDestination => pb::ResourceKind::ApplianceStateDestination,
        ResourceKind::Settings => pb::ResourceKind::Settings,
        ResourceKind::ConnectorRoutes => pb::ResourceKind::ConnectorRoutes,
    }
}

pub fn ref_from_proto(r: &pb::ResourceRef) -> Result<ResourceRef, tonic::Status> {
    Ok(ResourceRef {
        kind: kind_from_proto(r.kind)?,
        id: r.id.clone(),
    })
}

pub fn ref_to_proto(r: &ResourceRef) -> pb::ResourceRef {
    pb::ResourceRef {
        kind: kind_to_proto(r.kind) as i32,
        id: r.id.clone(),
    }
}

pub fn value_to_struct(value: &Value) -> prost_types::Struct {
    match value {
        Value::Object(map) => prost_types::Struct {
            fields: map
                .iter()
                .map(|(k, v)| (k.clone(), value_to_proto(v)))
                .collect(),
        },
        _ => prost_types::Struct::default(),
    }
}

pub fn value_to_proto(value: &Value) -> prost_types::Value {
    use prost_types::value::Kind;
    prost_types::Value {
        kind: Some(match value {
            Value::Null => Kind::NullValue(0),
            Value::Bool(b) => Kind::BoolValue(*b),
            Value::Number(n) => Kind::NumberValue(n.as_f64().unwrap_or(0.0)),
            Value::String(s) => Kind::StringValue(s.clone()),
            Value::Array(items) => Kind::ListValue(prost_types::ListValue {
                values: items.iter().map(value_to_proto).collect(),
            }),
            Value::Object(_) => Kind::StructValue(value_to_struct(value)),
        }),
    }
}

pub fn struct_to_value(s: &prost_types::Struct) -> Value {
    Value::Object(
        s.fields
            .iter()
            .map(|(k, v)| (k.clone(), proto_to_value(v)))
            .collect(),
    )
}

pub fn proto_to_value(value: &prost_types::Value) -> Value {
    use prost_types::value::Kind;
    match value.kind.as_ref() {
        Some(Kind::NullValue(_)) | None => Value::Null,
        Some(Kind::BoolValue(b)) => Value::Bool(*b),
        Some(Kind::NumberValue(n)) => serde_json::Number::from_f64(*n)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        Some(Kind::StringValue(s)) => Value::String(s.clone()),
        Some(Kind::ListValue(list)) => {
            Value::Array(list.values.iter().map(proto_to_value).collect())
        }
        Some(Kind::StructValue(s)) => struct_to_value(s),
    }
}

pub fn stored_to_proto(row: &StoredResource) -> pb::Resource {
    pb::Resource {
        r#ref: Some(ref_to_proto(&row.resource)),
        generation: row.generation,
        spec: Some(value_to_struct(&row.spec)),
        status: row.status.as_ref().map(value_to_struct),
        updated_at: None,
    }
}

pub fn issues_to_proto(issues: &[ValidationIssue]) -> pb::ValidationReport {
    pb::ValidationReport {
        valid: !issues.iter().any(ValidationIssue::is_error),
        issues: issues
            .iter()
            .map(|issue| pb::ValidationIssue {
                severity: match issue.severity {
                    Severity::Error => pb::validation_issue::Severity::Error as i32,
                    Severity::Warning => pb::validation_issue::Severity::Warning as i32,
                },
                code: issue.code.clone(),
                field: issue.field.clone(),
                message: issue.message.clone(),
                conflicting: issue.conflicting.as_ref().map(ref_to_proto),
            })
            .collect(),
    }
}
