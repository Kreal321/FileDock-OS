use std::sync::Arc;

use fd_api::core::v1::{
    self as pb, ListSecretsRequest, ListSecretsResponse, PutSecretRequest, secrets_server::Secrets,
};
use tonic::{Request, Response, Status};

use super::App;
use super::interceptors::check_metadata;

pub struct SecretsSvc {
    pub app: Arc<App>,
}

#[tonic::async_trait]
impl Secrets for SecretsSvc {
    async fn put_secret(
        &self,
        request: Request<PutSecretRequest>,
    ) -> Result<Response<pb::SecretInfo>, Status> {
        check_metadata(request.metadata(), self.app.config.interface_version, true)?;
        let req = request.into_inner();
        let info = self
            .app
            .secrets
            .lock()
            .await
            .put(&req.r#ref, &req.value, req.consumers)
            .map_err(|e| Status::invalid_argument(e.to_string()))?;
        Ok(Response::new(pb::SecretInfo {
            r#ref: info.r#ref,
            masked_hint: info.masked_hint,
            consumers: info.consumers,
            updated_at: None,
        }))
    }

    async fn delete_secret(
        &self,
        request: Request<pb::SecretRef>,
    ) -> Result<Response<pb::Empty>, Status> {
        check_metadata(request.metadata(), self.app.config.interface_version, true)?;
        self.app
            .secrets
            .lock()
            .await
            .delete(&request.get_ref().r#ref)
            .map_err(|e| Status::not_found(e.to_string()))?;
        Ok(Response::new(pb::Empty {}))
    }

    async fn list_secrets(
        &self,
        request: Request<ListSecretsRequest>,
    ) -> Result<Response<ListSecretsResponse>, Status> {
        check_metadata(request.metadata(), self.app.config.interface_version, false)?;
        let _ = request.into_inner();
        let secrets = self
            .app
            .secrets
            .lock()
            .await
            .list()
            .into_iter()
            .map(|s| pb::SecretInfo {
                r#ref: s.r#ref,
                masked_hint: s.masked_hint,
                consumers: s.consumers,
                updated_at: None,
            })
            .collect();
        Ok(Response::new(ListSecretsResponse {
            secrets,
            next_page_token: String::new(),
        }))
    }
}
