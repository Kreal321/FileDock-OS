use std::pin::Pin;
use std::sync::Arc;

use fd_api::extension::v1::{
    self as ext, ConfigureRequest, ConfigureResponse, Description, DiagnosticsReport, Empty,
    GetObjectChunk, GetObjectRequest, GetObjectTrailer, HealthReport, ListObjectsRequest,
    ObjectInfo, ObjectRef, ProbeRequest, ProbeResult, PutObjectResult, State,
    cloud_remote_server::{CloudRemote, CloudRemoteServer},
    subsystem_server::{Subsystem, SubsystemServer},
};
use tokio::net::UnixListener;
use tokio::sync::Mutex;
use tokio_stream::{Stream, wrappers::UnixListenerStream};
use tonic::transport::Server;
use tonic::{Request, Response, Status, Streaming};

use crate::config::{Config, RoutedConnection};
use crate::integrity::Hasher;
use crate::ops::{delete, get_with_metadata, put_mode, put_with_attributes, stat, store_for};
use crate::probe;
use futures::TryStreamExt;
use object_store::ObjectStore;
use std::os::unix::fs::PermissionsExt;

#[derive(Clone)]
struct App {
    config: Arc<Mutex<Config>>,
}

pub async fn serve(config: Config) -> Result<(), Box<dyn std::error::Error>> {
    let _ = std::fs::remove_file(&config.socket);
    if let Some(parent) = config.socket.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let listener = UnixListener::bind(&config.socket)?;
    std::fs::set_permissions(&config.socket, std::fs::Permissions::from_mode(0o600))?;
    if let Ok(token) = std::env::var("FD_EXTENSION_TOKEN") {
        let path = config.core_socket.clone();
        let channel = tonic::transport::Endpoint::from_static("http://localhost")
            .connect_with_connector(tower::service_fn(move |_| {
                let path = path.clone();
                async move {
                    tokio::net::UnixStream::connect(path)
                        .await
                        .map(hyper_util::rt::TokioIo::new)
                }
            }))
            .await?;
        let mut client = ext::core_bridge_client::CoreBridgeClient::new(channel);
        let mut request = Request::new(ext::RegisterRequest {
            subsystem_id: std::env::var("FD_EXTENSION_ID").unwrap_or_else(|_| "fd-s3".into()),
            version: std::env::var("FD_EXTENSION_VERSION")
                .unwrap_or_else(|_| env!("CARGO_PKG_VERSION").into()),
            socket_path: config.socket.to_string_lossy().into_owned(),
            capabilities: vec!["cloud.remote".into()],
        });
        request
            .metadata_mut()
            .insert("fd-extension-token", token.parse()?);
        client.register(request).await?;
    }
    let app = App {
        config: Arc::new(Mutex::new(config.clone())),
    };
    tracing::info!(socket = %config.socket.display(), "fd-s3 listening; Core discovers this built-in from its manifest");
    Server::builder()
        .add_service(SubsystemServer::new(app.clone()))
        .add_service(CloudRemoteServer::new(app))
        .serve_with_incoming(UnixListenerStream::new(listener))
        .await?;
    Ok(())
}

#[tonic::async_trait]
impl Subsystem for App {
    async fn describe(&self, _: Request<Empty>) -> Result<Response<Description>, Status> {
        Ok(Response::new(Description {
            subsystem_id: std::env::var("FD_EXTENSION_ID").unwrap_or_else(|_| "fd-s3".into()),
            name: "Native S3 connector".into(),
            version: std::env::var("FD_EXTENSION_VERSION")
                .unwrap_or_else(|_| env!("CARGO_PKG_VERSION").into()),
            capabilities: vec!["cloud.remote".into()],
        }))
    }

    async fn get_current_state(&self, _: Request<Empty>) -> Result<Response<State>, Status> {
        Ok(Response::new(State::default()))
    }

    async fn configure(
        &self,
        request: Request<ConfigureRequest>,
    ) -> Result<Response<ConfigureResponse>, Status> {
        let req = request.into_inner();
        let value = req
            .config
            .as_ref()
            .map(struct_json)
            .ok_or_else(|| Status::invalid_argument("config required"))?;
        let rows = value
            .get("connections")
            .and_then(|v| v.as_array())
            .ok_or_else(|| Status::invalid_argument("connections required"))?;
        let mut config = self.config.lock().await;
        let mut connections: std::collections::BTreeMap<String, RoutedConnection> =
            std::collections::BTreeMap::new();
        for input in rows {
            // The sample extension consumes the generic cloud connection envelope.
            let flattened;
            let row = if let Some(def) = input.get("spec").and_then(|s| s.get("definition")) {
                flattened = serde_json::json!({"id":input["id"],"endpoint":def["endpoint"],"region":def["region"],"bucket":def["bucket"],"virtual_hosted":def["addressingStyle"]=="Virtual","access_key_ref":def["accessKeyRef"],"secret_key_ref":def["secretKeyRef"],"storage_class":def["storageClass"],"bandwidth_limit":def["bandwidthLimit"],"checksums":false});
                &flattened
            } else {
                input
            };
            let text = |key: &str| {
                row.get(key)
                    .and_then(|v| v.as_str())
                    .map(str::to_owned)
                    .ok_or_else(|| Status::invalid_argument(format!("missing {key}")))
            };
            let secret = |reference: &str| -> Result<std::path::PathBuf, Status> {
                let path = std::path::PathBuf::from(
                    req.secret_files
                        .get(reference)
                        .ok_or_else(|| Status::invalid_argument("missing secret file"))?,
                );
                let actual = path
                    .canonicalize()
                    .map_err(|_| Status::invalid_argument("secret file does not exist"))?;
                let root = config
                    .secrets_dir
                    .canonicalize()
                    .map_err(|_| Status::invalid_argument("secret directory does not exist"))?;
                if !actual.starts_with(root) {
                    return Err(Status::permission_denied(
                        "secret outside connector runtime",
                    ));
                }
                Ok(actual)
            };
            let bandwidth_limit = row.get("bandwidth_limit").and_then(positive_u64);
            let id = text("id")?;
            let throttle_key = row
                .get("source_connection")
                .and_then(|v| v.as_str())
                .unwrap_or(&id)
                .to_owned();
            let throttle = config
                .connections
                .values()
                .chain(connections.values())
                .find(|c| c.throttle_key == throttle_key && c.bandwidth_limit == bandwidth_limit)
                .and_then(|c| c.throttle.clone())
                .or_else(|| bandwidth_limit.map(crate::throttle::TokenBucket::new));
            let connection = RoutedConnection {
                id,
                throttle_key,
                storage_class: row
                    .get("storage_class")
                    .and_then(|v| v.as_str())
                    .map(str::to_owned),
                kms_key_id: row
                    .get("kms_key_id")
                    .and_then(|v| v.as_str())
                    .map(str::to_owned),
                bandwidth_limit,
                throttle,
                endpoint: text("endpoint")?,
                region: text("region")?,
                bucket: text("bucket")?,
                virtual_hosted: row
                    .get("virtual_hosted")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                access_key_file: secret(&text("access_key_ref")?)?,
                secret_key_file: secret(&text("secret_key_ref")?)?,
                checksums: row
                    .get("checksums")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
            };
            if !connection.endpoint.starts_with("https://") {
                return Err(Status::failed_precondition("connection.https_required"));
            }
            connections.insert(connection.id.clone(), connection);
        }
        config.connections = connections;
        Ok(Response::new(ConfigureResponse {
            accepted: true,
            message: String::new(),
        }))
    }

    async fn start(&self, _: Request<Empty>) -> Result<Response<Empty>, Status> {
        Ok(Response::new(Empty {}))
    }
    async fn stop(&self, _: Request<Empty>) -> Result<Response<Empty>, Status> {
        Ok(Response::new(Empty {}))
    }
    async fn restart(&self, _: Request<Empty>) -> Result<Response<Empty>, Status> {
        Ok(Response::new(Empty {}))
    }
    async fn reload(&self, _: Request<Empty>) -> Result<Response<Empty>, Status> {
        Ok(Response::new(Empty {}))
    }
    async fn health(&self, _: Request<Empty>) -> Result<Response<HealthReport>, Status> {
        Ok(Response::new(HealthReport {
            status: ext::health_report::Status::Healthy as i32,
            detail: String::new(),
            at: None,
        }))
    }

    type EventsStream = Pin<Box<dyn Stream<Item = Result<ext::Event, Status>> + Send>>;

    async fn events(&self, _: Request<Empty>) -> Result<Response<Self::EventsStream>, Status> {
        let (_tx, rx) = tokio::sync::mpsc::channel(1);
        Ok(Response::new(Box::pin(
            tokio_stream::wrappers::ReceiverStream::new(rx),
        )))
    }

    async fn diagnostics(&self, _: Request<Empty>) -> Result<Response<DiagnosticsReport>, Status> {
        Ok(Response::new(DiagnosticsReport::default()))
    }
}

#[tonic::async_trait]
impl CloudRemote for App {
    async fn probe(&self, request: Request<ProbeRequest>) -> Result<Response<ProbeResult>, Status> {
        let req = request.into_inner();
        let conn = connection(&self.config, &req.connection_id).await?;
        let store = store_for(&conn).await?;
        let mut result = probe::run(&store, &req, conn.checksums).await;
        if !req.read_only
            && ["write", "read", "verify"]
                .iter()
                .all(|name| result.checks.iter().any(|c| c.name == *name && c.ok))
        {
            result.provider_checksums = probe::provider_checksums(&conn, &req.prefix).await;
        }
        Ok(Response::new(result))
    }

    async fn stat(&self, request: Request<ObjectRef>) -> Result<Response<ObjectInfo>, Status> {
        let r = request.into_inner();
        let conn = connection(&self.config, &r.connection_id).await?;
        let store = store_for(&conn).await?;
        let (size, etag) = stat(&store, &r.key).await?;
        Ok(Response::new(ObjectInfo {
            key: r.key,
            size,
            etag,
            last_modified: None,
        }))
    }

    type ListStream = Pin<Box<dyn Stream<Item = Result<ObjectInfo, Status>> + Send>>;

    async fn list(
        &self,
        request: Request<ListObjectsRequest>,
    ) -> Result<Response<Self::ListStream>, Status> {
        let req = request.into_inner();
        let conn = connection(&self.config, &req.connection_id).await?;
        let store = store_for(&conn).await?;
        let prefix = object_store::path::Path::from(req.prefix);
        let entries: Vec<_> = store
            .list(Some(&prefix))
            .try_collect()
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        let stream = tokio_stream::iter(entries.into_iter().map(|entry| {
            Ok(ObjectInfo {
                key: entry.location.to_string(),
                size: entry.size,
                etag: entry.e_tag.unwrap_or_default(),
                last_modified: None,
            })
        }));
        Ok(Response::new(Box::pin(stream)))
    }

    type GetStream = Pin<Box<dyn Stream<Item = Result<GetObjectChunk, Status>> + Send>>;

    async fn get(
        &self,
        request: Request<GetObjectRequest>,
    ) -> Result<Response<Self::GetStream>, Status> {
        let req = request.into_inner();
        let r = req.r#ref.ok_or_else(|| Status::invalid_argument("ref"))?;
        let conn = connection(&self.config, &r.connection_id).await?;
        let store = store_for(&conn).await?;
        let expected = (!req.expected_sha256.is_empty()).then_some(req.expected_sha256.as_str());
        let (data, digest, etag) = get_with_metadata(&store, &r.key, expected).await?;
        if let Some(bucket) = &conn.throttle {
            bucket.acquire(data.len()).await;
        }
        let verified = expected.is_none_or(|exp| exp == digest);
        let (tx, rx) = tokio::sync::mpsc::channel(4);
        tokio::spawn(async move {
            let _ = tx
                .send(Ok(GetObjectChunk {
                    part: Some(ext::get_object_chunk::Part::Data(data)),
                }))
                .await;
            let _ = tx
                .send(Ok(GetObjectChunk {
                    part: Some(ext::get_object_chunk::Part::Trailer(GetObjectTrailer {
                        sha256: digest,
                        verified,
                        etag,
                    })),
                }))
                .await;
        });
        Ok(Response::new(Box::pin(
            tokio_stream::wrappers::ReceiverStream::new(rx),
        )))
    }

    async fn put(
        &self,
        request: Request<Streaming<ext::PutObjectChunk>>,
    ) -> Result<Response<PutObjectResult>, Status> {
        let mut stream = request.into_inner();
        let header = match stream.message().await? {
            Some(chunk) => match chunk.part {
                Some(ext::put_object_chunk::Part::Header(h)) => h,
                _ => return Err(Status::invalid_argument("first message must be a header")),
            },
            None => return Err(Status::invalid_argument("empty put")),
        };
        let mut body = Vec::new();
        let mut hasher = Hasher::new();
        while let Some(chunk) = stream.message().await? {
            if let Some(ext::put_object_chunk::Part::Data(bytes)) = chunk.part {
                hasher.update(&bytes);
                body.extend_from_slice(&bytes);
            }
        }
        if header.size != body.len() as u64 {
            return Err(Status::invalid_argument("size mismatch"));
        }
        let digest = hasher.finalize();
        if !header.sha256.is_empty() && header.sha256 != digest {
            return Err(Status::invalid_argument("sha256 mismatch"));
        }
        let r = header
            .r#ref
            .ok_or_else(|| Status::invalid_argument("ref"))?;
        let conn = connection(&self.config, &r.connection_id).await?;
        let store = store_for(&conn).await?;
        if let Some(bucket) = &conn.throttle {
            bucket.acquire(body.len()).await;
        }
        let mut attributes = object_store::Attributes::new();
        if let Some(class) = &conn.storage_class {
            attributes.insert(object_store::Attribute::StorageClass, class.clone().into());
        }
        let etag = put_with_attributes(
            &store,
            &r.key,
            body,
            put_mode(header.mode, &header.if_match_etag),
            conn.checksums,
            attributes,
        )
        .await?;
        Ok(Response::new(PutObjectResult {
            etag,
            verification: if conn.checksums {
                ext::put_object_result::Verification::ProviderChecksum as i32
            } else {
                ext::put_object_result::Verification::ReadBack as i32
            },
        }))
    }

    async fn delete(
        &self,
        request: Request<ext::DeleteObjectRequest>,
    ) -> Result<Response<Empty>, Status> {
        let r = request
            .into_inner()
            .r#ref
            .ok_or_else(|| Status::invalid_argument("ref"))?;
        let conn = connection(&self.config, &r.connection_id).await?;
        let store = store_for(&conn).await?;
        delete(&store, &r.key).await?;
        Ok(Response::new(Empty {}))
    }

    async fn abort_incomplete_uploads(
        &self,
        request: Request<ext::AbortIncompleteUploadsRequest>,
    ) -> Result<Response<ext::AbortIncompleteUploadsResult>, Status> {
        let request = request.into_inner();
        let conn = connection(&self.config, &request.connection_id).await?;
        let aborted = crate::sweep::abort(
            &conn,
            &request.prefix,
            request.older_than_hours,
            request.root_owner_checked,
        )
        .await?;
        Ok(Response::new(ext::AbortIncompleteUploadsResult { aborted }))
    }
}

async fn connection(config: &Mutex<Config>, id: &str) -> Result<RoutedConnection, Status> {
    config
        .lock()
        .await
        .connections
        .get(id)
        .cloned()
        .ok_or_else(|| Status::not_found(format!("connection {id}")))
}

fn struct_json(value: &prost_types::Struct) -> serde_json::Value {
    serde_json::Value::Object(
        value
            .fields
            .iter()
            .map(|(k, v)| (k.clone(), proto_json(v)))
            .collect(),
    )
}
fn proto_json(value: &prost_types::Value) -> serde_json::Value {
    use prost_types::value::Kind;
    match value.kind.as_ref() {
        Some(Kind::StructValue(v)) => struct_json(v),
        Some(Kind::ListValue(v)) => {
            serde_json::Value::Array(v.values.iter().map(proto_json).collect())
        }
        Some(Kind::StringValue(v)) => serde_json::json!(v),
        Some(Kind::NumberValue(v)) => serde_json::json!(v),
        Some(Kind::BoolValue(v)) => serde_json::json!(v),
        _ => serde_json::Value::Null,
    }
}

// google.protobuf.Struct transports JSON numbers as doubles, including integral limits.
fn positive_u64(value: &serde_json::Value) -> Option<u64> {
    value.as_u64().filter(|n| *n > 0).or_else(|| {
        value
            .as_f64()
            .filter(|n| n.is_finite() && *n > 0.0 && n.fract() == 0.0 && *n < u64::MAX as f64)
            .map(|n| n as u64)
    })
}
#[cfg(test)]
mod tests {
    #[test]
    fn accepts_integral_protobuf_numbers_for_bandwidth_limits() {
        for value in [serde_json::json!(65536), serde_json::json!(65536.0)] {
            assert_eq!(super::positive_u64(&value), Some(65536));
        }
        for value in [
            serde_json::json!(0.0),
            serde_json::json!(-1.0),
            serde_json::json!(1.5),
            serde_json::json!("65536"),
        ] {
            assert_eq!(super::positive_u64(&value), None);
        }
    }
}
