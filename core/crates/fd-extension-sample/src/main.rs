//! Test-only implementation of the public extension contract.
use fd_api::extension::v1::{
    self as pb,
    subsystem_server::{Subsystem, SubsystemServer},
};
use std::{
    pin::Pin,
    sync::{Arc, Mutex},
};
use tonic::{Request, Response, Status};
#[derive(Clone)]
struct Sample {
    id: String,
    version: String,
    state: Arc<Mutex<Option<prost_types::Struct>>>,
}
#[tonic::async_trait]
impl Subsystem for Sample {
    async fn describe(&self, _: Request<pb::Empty>) -> Result<Response<pb::Description>, Status> {
        Ok(Response::new(pb::Description {
            subsystem_id: self.id.clone(),
            name: self.id.clone(),
            version: self.version.clone(),
            capabilities: vec!["cache.generic".into()],
        }))
    }
    async fn get_current_state(
        &self,
        _: Request<pb::Empty>,
    ) -> Result<Response<pb::State>, Status> {
        Ok(Response::new(pb::State {
            desired: self.state.lock().unwrap().clone(),
            current: None,
        }))
    }
    async fn configure(
        &self,
        r: Request<pb::ConfigureRequest>,
    ) -> Result<Response<pb::ConfigureResponse>, Status> {
        let r = r.into_inner();
        for path in r.data_paths.values() {
            tokio::fs::write(format!("{path}/sample.txt"), b"sample capability used")
                .await
                .map_err(|_| Status::failed_precondition("bound data is not writable"))?;
        }
        if tokio::fs::write("/var/lib/filedock-extension-outside", b"must be denied")
            .await
            .is_ok()
        {
            return Err(Status::failed_precondition(
                "sandbox allowed an unbound persistent write",
            ));
        }
        eprintln!("SANDBOX_DENIED: write outside bound folders failed");
        *self.state.lock().unwrap() = r.config;
        Ok(Response::new(pb::ConfigureResponse {
            accepted: true,
            message: String::new(),
        }))
    }
    async fn start(&self, _: Request<pb::Empty>) -> Result<Response<pb::Empty>, Status> {
        Ok(Response::new(pb::Empty {}))
    }
    async fn stop(&self, _: Request<pb::Empty>) -> Result<Response<pb::Empty>, Status> {
        Ok(Response::new(pb::Empty {}))
    }
    async fn restart(&self, _: Request<pb::Empty>) -> Result<Response<pb::Empty>, Status> {
        Ok(Response::new(pb::Empty {}))
    }
    async fn reload(&self, _: Request<pb::Empty>) -> Result<Response<pb::Empty>, Status> {
        Ok(Response::new(pb::Empty {}))
    }
    async fn health(&self, _: Request<pb::Empty>) -> Result<Response<pb::HealthReport>, Status> {
        Ok(Response::new(pb::HealthReport {
            status: if std::env::args().any(|a| a == "--broken") {
                pb::health_report::Status::Unhealthy
            } else {
                pb::health_report::Status::Healthy
            } as i32,
            detail: "sample".into(),
            at: None,
        }))
    }
    type EventsStream = Pin<Box<dyn tokio_stream::Stream<Item = Result<pb::Event, Status>> + Send>>;
    async fn events(&self, _: Request<pb::Empty>) -> Result<Response<Self::EventsStream>, Status> {
        Ok(Response::new(Box::pin(tokio_stream::pending())))
    }
    async fn diagnostics(
        &self,
        _: Request<pb::Empty>,
    ) -> Result<Response<pb::DiagnosticsReport>, Status> {
        Ok(Response::new(pb::DiagnosticsReport {
            data: None,
            log_lines: vec!["sample diagnostics".into()],
        }))
    }
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().nth(1).as_deref() == Some("--assert-bound-delete-refused") {
        if unsafe { libc::geteuid() } != 0 {
            return Err("Run this fixture helper only as root on a disposable VM".into());
        }
        let id = std::env::args().nth(2).ok_or("Folder id required")?;
        let socket =
            std::env::var("FD_CORE_SOCKET").unwrap_or_else(|_| "/run/filedock/core.sock".into());
        let channel = tonic::transport::Endpoint::from_static("http://localhost")
            .connect_with_connector(tower::service_fn(move |_| {
                let socket = socket.clone();
                async move {
                    tokio::net::UnixStream::connect(socket)
                        .await
                        .map(hyper_util::rt::TokioIo::new)
                }
            }))
            .await?;
        let mut request = Request::new(fd_api::core::v1::ApplyChangeRequest {
            changes: vec![fd_api::core::v1::ResourceChange {
                change: Some(fd_api::core::v1::resource_change::Change::Delete(
                    fd_api::core::v1::resource_change::Delete {
                        r#ref: Some(fd_api::core::v1::ResourceRef {
                            kind: fd_api::core::v1::ResourceKind::ManagedFolder as i32,
                            id,
                        }),
                        expected_generation: 1,
                    },
                )),
            }],
        });
        request
            .metadata_mut()
            .insert("fd-interface-version", "1.0".parse()?);
        request
            .metadata_mut()
            .insert("fd-actor", "console:fixture".parse()?);
        request
            .metadata_mut()
            .insert("fd-request-id", "binding-guard-fixture".parse()?);
        match fd_api::core::v1::core_control_client::CoreControlClient::new(channel)
            .apply_change(request)
            .await
        {
            Err(e)
                if e.code() == tonic::Code::InvalidArgument
                    && e.message().contains("bound folder") =>
            {
                println!("PASS: bound folder deletion refused");
                return Ok(());
            }
            _ => return Err("Binding guard did not refuse deletion".into()),
        }
    }
    let socket = std::env::var("FD_EXTENSION_SOCKET")?;
    let id = std::env::var("FD_EXTENSION_ID")?;
    let version = std::env::var("FD_EXTENSION_VERSION")?;
    let token = std::env::var("FD_EXTENSION_TOKEN")?;
    let bridge = std::env::var("FD_CORE_SOCKET")?;
    let _ = tokio::fs::remove_file(&socket).await;
    let listener = tokio::net::UnixListener::bind(&socket)?;
    let service = Sample {
        id: id.clone(),
        version: version.clone(),
        state: Default::default(),
    };
    tokio::spawn(async move {
        let _ = tonic::transport::Server::builder()
            .add_service(SubsystemServer::new(service))
            .serve_with_incoming(tokio_stream::wrappers::UnixListenerStream::new(listener))
            .await;
    });
    let channel = tonic::transport::Endpoint::from_static("http://localhost")
        .connect_with_connector(tower::service_fn(move |_| {
            let bridge = bridge.clone();
            async move {
                tokio::net::UnixStream::connect(bridge)
                    .await
                    .map(hyper_util::rt::TokioIo::new)
            }
        }))
        .await?;
    let mut request = Request::new(pb::RegisterRequest {
        subsystem_id: id,
        version,
        socket_path: socket,
        capabilities: vec!["cache.generic".into()],
    });
    request
        .metadata_mut()
        .insert("fd-extension-token", token.parse()?);
    pb::core_bridge_client::CoreBridgeClient::new(channel)
        .register(request)
        .await?;
    std::future::pending::<()>().await;
    Ok(())
}
