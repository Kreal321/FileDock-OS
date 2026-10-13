//! Per-kind controllers and the work queue (research R6).

pub mod adopt;
pub mod queue;

use fd_state::{ResourceKind, ResourceRef};

pub use queue::{ReconcileError, Reconciler, WorkQueue};

#[async_trait::async_trait]
pub trait Controller: Send + Sync {
    fn kind(&self) -> ResourceKind;
    async fn reconcile(&self, resource: &ResourceRef) -> Result<(), ReconcileError>;
}
