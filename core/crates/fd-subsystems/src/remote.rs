//! Out-of-process Subsystem adapter; all requests have bounded deadlines.
use crate::adapter::{AdapterError, Description, Health, SubsystemAdapter};
use fd_api::extension::v1::{self as pb, subsystem_client::SubsystemClient};
use serde_json::Value;
use std::{path::PathBuf, time::Duration};
#[derive(Clone)]
pub struct Remote {
    pub id: String,
    pub socket: PathBuf,
}
impl Remote {
    pub async fn client(&self) -> Result<SubsystemClient<tonic::transport::Channel>, AdapterError> {
        let socket = self.socket.clone();
        let channel = tonic::transport::Endpoint::from_static("http://localhost")
            .connect_timeout(Duration::from_secs(2))
            .timeout(Duration::from_secs(5))
            .connect_with_connector(tower::service_fn(move |_| {
                let socket = socket.clone();
                async move {
                    tokio::net::UnixStream::connect(socket)
                        .await
                        .map(hyper_util::rt::TokioIo::new)
                }
            }))
            .await
            .map_err(|_| AdapterError::Failed("extension.unavailable".into()))?;
        Ok(SubsystemClient::new(channel))
    }
    pub async fn configure_full(
        &self,
        config: Value,
        secret_files: std::collections::HashMap<String, String>,
        data_paths: std::collections::HashMap<String, String>,
    ) -> Result<(), AdapterError> {
        let response = self
            .client()
            .await?
            .configure(pb::ConfigureRequest {
                config: Some(to_struct(&config)),
                secret_files,
                data_paths,
            })
            .await
            .map_err(error)?
            .into_inner();
        if !response.accepted {
            return Err(AdapterError::Failed(
                "extension.configuration_refused".into(),
            ));
        }
        Ok(())
    }
    pub async fn events(&self) -> Result<tonic::Streaming<pb::Event>, AdapterError> {
        Ok(self
            .client()
            .await?
            .events(pb::Empty {})
            .await
            .map_err(error)?
            .into_inner())
    }
}
fn error(_: tonic::Status) -> AdapterError {
    AdapterError::Failed("extension.rpc_failed".into())
}
fn from_struct(value: Option<prost_types::Struct>) -> Value {
    value
        .map(|s| {
            Value::Object(
                s.fields
                    .into_iter()
                    .map(|(k, v)| (k, from_value(v)))
                    .collect(),
            )
        })
        .unwrap_or(Value::Null)
}
fn from_value(value: prost_types::Value) -> Value {
    use prost_types::value::Kind;
    match value.kind {
        Some(Kind::NumberValue(n)) => serde_json::json!(n),
        Some(Kind::BoolValue(b)) => Value::Bool(b),
        Some(Kind::StringValue(s)) => Value::String(s),
        Some(Kind::StructValue(s)) => from_struct(Some(s)),
        Some(Kind::ListValue(l)) => Value::Array(l.values.into_iter().map(from_value).collect()),
        _ => Value::Null,
    }
}
pub fn to_struct(value: &Value) -> prost_types::Struct {
    prost_types::Struct {
        fields: value
            .as_object()
            .into_iter()
            .flatten()
            .map(|(k, v)| (k.clone(), to_value(v)))
            .collect(),
    }
}
fn to_value(value: &Value) -> prost_types::Value {
    use prost_types::value::Kind;
    prost_types::Value {
        kind: Some(match value {
            Value::Null => Kind::NullValue(0),
            Value::Bool(b) => Kind::BoolValue(*b),
            Value::Number(n) => Kind::NumberValue(n.as_f64().unwrap_or(0.)),
            Value::String(s) => Kind::StringValue(s.clone()),
            Value::Array(a) => Kind::ListValue(prost_types::ListValue {
                values: a.iter().map(to_value).collect(),
            }),
            Value::Object(_) => Kind::StructValue(to_struct(value)),
        }),
    }
}
#[async_trait::async_trait]
impl SubsystemAdapter for Remote {
    fn id(&self) -> &str {
        &self.id
    }
    async fn describe(&self) -> Result<Description, AdapterError> {
        let r = self
            .client()
            .await?
            .describe(pb::Empty {})
            .await
            .map_err(error)?
            .into_inner();
        if r.subsystem_id != self.id {
            return Err(AdapterError::Failed("extension.identity_mismatch".into()));
        }
        Ok(Description {
            id: r.subsystem_id,
            name: r.name,
            version: r.version,
            capabilities: r.capabilities,
        })
    }
    async fn state(&self) -> Result<Value, AdapterError> {
        let r = self
            .client()
            .await?
            .get_current_state(pb::Empty {})
            .await
            .map_err(error)?
            .into_inner();
        Ok(serde_json::json!({"desired":from_struct(r.desired),"current":from_struct(r.current)}))
    }
    async fn configure(&self, desired: Value) -> Result<(), AdapterError> {
        self.configure_full(desired, Default::default(), Default::default())
            .await
    }
    async fn start(&self) -> Result<(), AdapterError> {
        self.client()
            .await?
            .start(pb::Empty {})
            .await
            .map_err(error)?;
        Ok(())
    }
    async fn stop(&self) -> Result<(), AdapterError> {
        self.client()
            .await?
            .stop(pb::Empty {})
            .await
            .map_err(error)?;
        Ok(())
    }
    async fn restart(&self) -> Result<(), AdapterError> {
        self.client()
            .await?
            .restart(pb::Empty {})
            .await
            .map_err(error)?;
        Ok(())
    }
    async fn reload(&self) -> Result<(), AdapterError> {
        self.client()
            .await?
            .reload(pb::Empty {})
            .await
            .map_err(error)?;
        Ok(())
    }
    async fn health(&self) -> Result<Health, AdapterError> {
        let r = self
            .client()
            .await?
            .health(pb::Empty {})
            .await
            .map_err(error)?
            .into_inner();
        Ok(Health {
            healthy: r.status == pb::health_report::Status::Healthy as i32,
            detail: fd_secrets::redact::redact_text(&r.detail),
        })
    }
    async fn diagnostics(&self) -> Result<Value, AdapterError> {
        let r = self
            .client()
            .await?
            .diagnostics(pb::Empty {})
            .await
            .map_err(error)?
            .into_inner();
        Ok(fd_secrets::redact::redact_value(
            serde_json::json!({"data":from_struct(r.data),"logLines":r.log_lines}),
        ))
    }
}
