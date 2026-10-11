//! GenerateRecoveryKey and ConfirmRecoveryKey (FR-033, FR-034).

use std::collections::BTreeMap;
use std::pin::Pin;
use std::sync::Arc;

use fd_api::core::v1::{
    self as pb, BeginRestoreRequest, ConfirmRecoveryKeyRequest, ConfirmTakeoverRequest,
    DetectLocalStateResponse, GenerateRecoveryKeyRequest, GeneratedRecoveryKey,
    ListOwnedLocationsResponse, ListRestoreSnapshotsResponse, ManagementConfig, MapPoolsRequest,
    OpenLocalRestoreSourceRequest, OpenRestoreSourceRequest, RestoreFileChunk, RestoreSource,
    RestoreSourceRef, SetStateDestinationRequest, StateProtection, recovery_server::Recovery,
};
use tokio_stream::Stream;
use tonic::{Request, Response, Status, Streaming};

use super::App;
use super::interceptors::check_metadata;

pub struct RecoverySvc {
    pub app: Arc<App>,
}

#[tonic::async_trait]
impl Recovery for RecoverySvc {
    async fn generate_recovery_key(
        &self,
        request: Request<GenerateRecoveryKeyRequest>,
    ) -> Result<Response<GeneratedRecoveryKey>, Status> {
        check_metadata(request.metadata(), self.app.config.interface_version, true)?;
        let regenerate = request.into_inner().regenerate;
        let generated = self
            .app
            .recovery
            .lock()
            .await
            .generate(regenerate)
            .map_err(|e| Status::failed_precondition(e.to_string()))?;
        Ok(Response::new(GeneratedRecoveryKey {
            key_id: generated.key_id,
            recovery_key: generated.recovery_key,
            confirm_group_indexes: generated.confirm_group_indexes,
        }))
    }

    async fn confirm_recovery_key(
        &self,
        request: Request<ConfirmRecoveryKeyRequest>,
    ) -> Result<Response<pb::Empty>, Status> {
        check_metadata(request.metadata(), self.app.config.interface_version, true)?;
        let req = request.into_inner();
        let groups: BTreeMap<u32, String> = req.groups.into_iter().collect();
        self.app
            .recovery
            .lock()
            .await
            .confirm(&req.key_id, &groups)
            .map_err(|e| Status::invalid_argument(e.to_string()))?;
        Ok(Response::new(pb::Empty {}))
    }

    async fn set_state_destination(
        &self,
        request: Request<SetStateDestinationRequest>,
    ) -> Result<Response<pb::Operation>, Status> {
        check_metadata(request.metadata(), self.app.config.interface_version, true)?;
        let _ = request;
        Err(Status::unimplemented(
            "state destination is not available yet",
        ))
    }

    async fn get_state_protection(
        &self,
        request: Request<pb::Empty>,
    ) -> Result<Response<StateProtection>, Status> {
        check_metadata(request.metadata(), self.app.config.interface_version, false)?;
        let _ = request;
        Err(Status::unimplemented(
            "state protection is not available yet",
        ))
    }

    async fn put_management_config(
        &self,
        request: Request<ManagementConfig>,
    ) -> Result<Response<pb::Empty>, Status> {
        check_metadata(request.metadata(), self.app.config.interface_version, true)?;
        let _ = request;
        Ok(Response::new(pb::Empty {}))
    }

    type ExportBackupFileStream =
        Pin<Box<dyn Stream<Item = Result<pb::BundleChunk, Status>> + Send>>;

    async fn export_backup_file(
        &self,
        request: Request<pb::Empty>,
    ) -> Result<Response<Self::ExportBackupFileStream>, Status> {
        check_metadata(request.metadata(), self.app.config.interface_version, false)?;
        let _ = request;
        Err(Status::unimplemented(
            "backup file export is not available yet",
        ))
    }

    async fn detect_local_state(
        &self,
        request: Request<pb::Empty>,
    ) -> Result<Response<DetectLocalStateResponse>, Status> {
        check_metadata(request.metadata(), self.app.config.interface_version, false)?;
        let _ = request;
        Ok(Response::new(DetectLocalStateResponse {
            appliances: vec![],
        }))
    }

    async fn open_local_restore_source(
        &self,
        request: Request<OpenLocalRestoreSourceRequest>,
    ) -> Result<Response<RestoreSource>, Status> {
        check_metadata(request.metadata(), self.app.config.interface_version, true)?;
        let _ = request;
        Err(Status::unimplemented("local restore is not available yet"))
    }

    async fn open_restore_source(
        &self,
        request: Request<OpenRestoreSourceRequest>,
    ) -> Result<Response<RestoreSource>, Status> {
        check_metadata(request.metadata(), self.app.config.interface_version, true)?;
        let _ = request;
        Err(Status::unimplemented("restore source is not available yet"))
    }

    async fn upload_restore_file(
        &self,
        request: Request<Streaming<RestoreFileChunk>>,
    ) -> Result<Response<RestoreSource>, Status> {
        check_metadata(request.metadata(), self.app.config.interface_version, true)?;
        let _ = request;
        Err(Status::unimplemented(
            "restore file upload is not available yet",
        ))
    }

    async fn list_restore_snapshots(
        &self,
        request: Request<RestoreSourceRef>,
    ) -> Result<Response<ListRestoreSnapshotsResponse>, Status> {
        check_metadata(request.metadata(), self.app.config.interface_version, false)?;
        let _ = request;
        Err(Status::unimplemented(
            "restore snapshots are not available yet",
        ))
    }

    async fn begin_restore(
        &self,
        request: Request<BeginRestoreRequest>,
    ) -> Result<Response<pb::Operation>, Status> {
        check_metadata(request.metadata(), self.app.config.interface_version, true)?;
        let _ = request;
        Err(Status::unimplemented("restore is not available yet"))
    }

    async fn map_pools(
        &self,
        request: Request<MapPoolsRequest>,
    ) -> Result<Response<pb::Operation>, Status> {
        check_metadata(request.metadata(), self.app.config.interface_version, true)?;
        let _ = request;
        Err(Status::unimplemented("pool mapping is not available yet"))
    }

    async fn get_management_config_for_restore(
        &self,
        request: Request<RestoreSourceRef>,
    ) -> Result<Response<ManagementConfig>, Status> {
        check_metadata(request.metadata(), self.app.config.interface_version, false)?;
        let _ = request;
        Err(Status::unimplemented("restore config is not available yet"))
    }

    async fn get_restore_report(
        &self,
        request: Request<pb::OperationRef>,
    ) -> Result<Response<pb::RestoreReport>, Status> {
        check_metadata(request.metadata(), self.app.config.interface_version, false)?;
        let _ = request;
        Err(Status::unimplemented("restore report is not available yet"))
    }

    async fn list_owned_locations(
        &self,
        request: Request<pb::Empty>,
    ) -> Result<Response<ListOwnedLocationsResponse>, Status> {
        check_metadata(request.metadata(), self.app.config.interface_version, false)?;
        let _ = request;
        Ok(Response::new(ListOwnedLocationsResponse {
            locations: vec![],
        }))
    }

    async fn confirm_takeover(
        &self,
        request: Request<ConfirmTakeoverRequest>,
    ) -> Result<Response<pb::Operation>, Status> {
        check_metadata(request.metadata(), self.app.config.interface_version, true)?;
        let _ = request;
        Err(Status::unimplemented("takeover is not available yet"))
    }
}
