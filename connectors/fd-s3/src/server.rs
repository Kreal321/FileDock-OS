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
use crate::ops::{delete, get, put, put_mode, stat, store_for};
use crate::probe;

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
            subsystem_id: "fd-s3".into(),
            name: "Native S3 connector".into(),
            version: env!("CARGO_PKG_VERSION").into(),
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
        let _ = request;
        Ok(Response::new(ConfigureResponse::default()))
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
        Ok(Response::new(probe::run(&store, &req).await))
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
        let _ = request;
        let (_tx, rx) = tokio::sync::mpsc::channel(1);
        Ok(Response::new(Box::pin(
            tokio_stream::wrappers::ReceiverStream::new(rx),
        )))
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
        let (data, digest) = get(&store, &r.key, expected).await?;
        let verified = expected.is_none_or(|exp| exp == digest);
        let (tx, rx) = tokio::sync::mpsc::channel(4);
        tokio::spawn(async move {
            let _ = tx
                .send(Ok(GetObjectChunk {
                    part: Some(ext::get_object_chunk::Part::Data(data.into())),
                }))
                .await;
            let _ = tx
                .send(Ok(GetObjectChunk {
                    part: Some(ext::get_object_chunk::Part::Trailer(GetObjectTrailer {
                        sha256: digest,
                        verified,
                        etag: String::new(),
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
        let digest = hasher.finalize();
        if !header.sha256.is_empty() && header.sha256 != digest {
            return Err(Status::invalid_argument("sha256 mismatch"));
        }
        let r = header
            .r#ref
            .ok_or_else(|| Status::invalid_argument("ref"))?;
        let conn = connection(&self.config, &r.connection_id).await?;
        let store = store_for(&conn).await?;
        let etag = put(
            &store,
            &r.key,
            body,
            put_mode(header.mode, &header.if_match_etag),
            conn.checksums,
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
        let _ = request;
        Ok(Response::new(ext::AbortIncompleteUploadsResult::default()))
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
