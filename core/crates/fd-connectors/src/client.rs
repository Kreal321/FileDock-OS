//! CloudRemote client with session pinning (FR-118).

use std::sync::Arc;

use fd_api::extension::v1 as ext;
use fd_state::resources::RouteTarget;
use tokio::sync::Mutex;
use tonic::transport::Channel;

#[derive(Debug, thiserror::Error)]
pub enum TrailerError {
    #[error("the Get stream ended without a verified trailer")]
    MissingTrailer,
    #[error("checksum mismatch")]
    DataLoss,
}

/// A write session resolves its route once and keeps that connector.
pub struct Session {
    pub connection_id: String,
    pub target: RouteTarget,
    pub routes_generation: u64,
}

pub struct CloudRemoteClient {
    inner: Mutex<Option<ext::cloud_remote_client::CloudRemoteClient<Channel>>>,
    session: Session,
}

impl CloudRemoteClient {
    pub fn pin(session: Session) -> Self {
        Self {
            inner: Mutex::new(None),
            session,
        }
    }

    pub fn session(&self) -> &Session {
        &self.session
    }

    /// A routes-generation change does not move this session.
    pub fn still_valid_for(&self, current_generation: u64) -> bool {
        let _ = current_generation;
        true
    }

    pub async fn attach(&self, client: ext::cloud_remote_client::CloudRemoteClient<Channel>) {
        *self.inner.lock().await = Some(client);
    }

    /// Collect a Get stream. Data from a stream that doesn't end in a verified trailer is discarded.
    pub async fn get_verified(
        mut stream: tonic::Streaming<ext::GetObjectChunk>,
    ) -> Result<(Vec<u8>, ext::GetObjectTrailer), TrailerError> {
        let mut data = Vec::new();
        let mut trailer = None;
        while let Some(chunk) = stream
            .message()
            .await
            .map_err(|_| TrailerError::MissingTrailer)?
        {
            match chunk.part {
                Some(ext::get_object_chunk::Part::Data(bytes)) => data.extend_from_slice(&bytes),
                Some(ext::get_object_chunk::Part::Trailer(t)) => trailer = Some(t),
                None => {}
            }
        }
        match trailer {
            Some(t) if t.verified => Ok((data, t)),
            Some(_) => Err(TrailerError::DataLoss),
            None => Err(TrailerError::MissingTrailer),
        }
    }
}

impl CloudRemoteClient {
    pub fn shared(session: Session) -> Arc<Self> {
        Arc::new(Self::pin(session))
    }
}

/// Refuse unavailable routed targets; callers report the same target and never substitute one.
pub fn require_running(enabled: bool, state: &str) -> Result<(), &'static str> {
    if enabled && state == "Running" {
        Ok(())
    } else {
        Err("connector.unavailable")
    }
}
#[cfg(test)]
mod availability_tests {
    #[test]
    fn no_unavailable_state_is_treated_as_running() {
        for state in ["Failed", "Stopped", "Starting", "Backoff", ""] {
            assert_eq!(
                super::require_running(true, state),
                Err("connector.unavailable")
            );
        }
        assert!(super::require_running(false, "Running").is_err());
        assert!(super::require_running(true, "Running").is_ok());
    }
}
