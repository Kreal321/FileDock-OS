//! Authenticated registration for the first-party native connector (T035).
use super::App;
use fd_api::extension::v1::{self as ext, core_bridge_server::CoreBridge};
use std::{pin::Pin, sync::Arc};
use tokio_stream::Stream;
use tonic::{Request, Response, Status};

pub struct Bridge {
    pub app: Arc<App>,
}
impl Bridge {
    fn authenticate<T>(&self, request: &Request<T>) -> Result<(), Status> {
        let token = request
            .metadata()
            .get("fd-extension-token")
            .and_then(|v| v.to_str().ok());
        if token != Some(self.app.extension_token.as_str()) {
            return Err(Status::unauthenticated("invalid extension token"));
        }
        Ok(())
    }
}
#[tonic::async_trait]
impl CoreBridge for Bridge {
    async fn register(
        &self,
        request: Request<ext::RegisterRequest>,
    ) -> Result<Response<ext::RegisterResponse>, Status> {
        self.authenticate(&request)?;
        let req = request.into_inner();
        // Phase 2 provisions a per-instance token solely for the bundled native S3 process.
        if req.subsystem_id != "fd-s3"
            || std::path::Path::new(&req.socket_path) != crate::runtime::s3_socket(&self.app)
        {
            return Err(Status::permission_denied(
                "token is bound to fd-s3 and its runtime socket",
            ));
        }
        let record = self
            .app
            .subsystems
            .get(&req.subsystem_id)
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::permission_denied("unknown subsystem"))?;
        let declared = record
            .manifest
            .get("capabilities")
            .and_then(|v| v.as_array())
            .ok_or_else(|| Status::permission_denied("missing manifest capabilities"))?;
        if !record.enabled
            || req
                .capabilities
                .iter()
                .any(|c| !declared.iter().any(|d| d.as_str() == Some(c)))
        {
            return Err(Status::permission_denied("undeclared capability"));
        }
        Ok(Response::new(ext::RegisterResponse {
            interface_version: self.app.config.interface_version.into(),
            granted_permissions: vec!["network.outbound".into(), "cloud.credentials".into()],
        }))
    }
    type StreamEventsStream = Pin<Box<dyn Stream<Item = Result<ext::Event, Status>> + Send>>;
    async fn stream_events(
        &self,
        request: Request<ext::StreamEventsRequest>,
    ) -> Result<Response<Self::StreamEventsStream>, Status> {
        self.authenticate(&request)?;
        Err(Status::permission_denied(
            "fd-s3 does not declare core.events.read",
        ))
    }
    async fn raise_alert(
        &self,
        request: Request<ext::RaiseAlertRequest>,
    ) -> Result<Response<ext::Empty>, Status> {
        self.authenticate(&request)?;
        Err(Status::permission_denied(
            "fd-s3 does not declare core.alerts.write",
        ))
    }
    async fn report_health(
        &self,
        request: Request<ext::HealthReport>,
    ) -> Result<Response<ext::Empty>, Status> {
        self.authenticate(&request)?;
        Ok(Response::new(ext::Empty {}))
    }
}
