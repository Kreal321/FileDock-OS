use std::collections::{HashSet, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{Mutex, Notify};
use tokio::time::MissedTickBehavior;

use fd_state::{CommitNotice, ResourceKind, ResourceRef, StateStore};

use crate::Controller;

const RESYNC: Duration = Duration::from_secs(10 * 60);

#[derive(Debug, thiserror::Error)]
pub enum ReconcileError {
    #[error("{0}")]
    Failed(String),
}

pub struct WorkQueue {
    pending: Mutex<VecDeque<ResourceRef>>,
    seen: Mutex<HashSet<(ResourceKind, String)>>,
    notify: Notify,
}

impl WorkQueue {
    pub fn new() -> Self {
        Self {
            pending: Mutex::new(VecDeque::new()),
            seen: Mutex::new(HashSet::new()),
            notify: Notify::new(),
        }
    }

    pub async fn enqueue(&self, resource: ResourceRef) {
        let key = (resource.kind, resource.id.clone());
        let mut seen = self.seen.lock().await;
        if seen.insert(key) {
            self.pending.lock().await.push_back(resource);
            self.notify.notify_one();
        }
    }

    pub async fn enqueue_all(&self, resources: impl IntoIterator<Item = ResourceRef>) {
        for resource in resources {
            self.enqueue(resource).await;
        }
    }

    async fn take(&self) -> ResourceRef {
        loop {
            if let Some(resource) = self.pending.lock().await.pop_front() {
                self.seen
                    .lock()
                    .await
                    .remove(&(resource.kind, resource.id.clone()));
                return resource;
            }
            self.notify.notified().await;
        }
    }
}

impl Default for WorkQueue {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Reconciler {
    store: Arc<StateStore>,
    queue: Arc<WorkQueue>,
    controllers: Vec<Arc<dyn Controller>>,
}

impl Reconciler {
    pub fn new(store: Arc<StateStore>, controllers: Vec<Arc<dyn Controller>>) -> Self {
        Self {
            store,
            queue: Arc::new(WorkQueue::new()),
            controllers,
        }
    }

    pub fn queue(&self) -> Arc<WorkQueue> {
        self.queue.clone()
    }

    pub async fn enqueue_commit(&self, notice: CommitNotice) {
        self.queue.enqueue_all(notice.resources).await;
    }

    pub async fn enqueue_event(&self, resource: ResourceRef) {
        self.queue.enqueue(resource).await;
    }

    /// Enqueue every live resource. Called at startup and every 10 minutes.
    pub async fn enqueue_all_kinds(&self) {
        for kind in ResourceKind::ALL {
            match self.store.list(kind, None, 10_000) {
                Ok(rows) => {
                    self.queue
                        .enqueue_all(rows.into_iter().map(|r| r.resource))
                        .await
                }
                Err(error) => tracing::error!(%kind, %error, "resync list failed"),
            }
        }
    }

    pub async fn run(self: Arc<Self>) {
        let worker = {
            let this = self.clone();
            tokio::spawn(async move { this.worker().await })
        };
        let resync = {
            let this = self.clone();
            tokio::spawn(async move {
                this.enqueue_all_kinds().await;
                let mut ticker = tokio::time::interval(RESYNC);
                ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
                ticker.tick().await;
                loop {
                    ticker.tick().await;
                    this.enqueue_all_kinds().await;
                }
            })
        };
        let _ = tokio::join!(worker, resync);
    }

    async fn worker(&self) {
        loop {
            let resource = self.queue.take().await;
            if let Some(controller) = self.controllers.iter().find(|c| c.kind() == resource.kind)
                && let Err(error) = controller.reconcile(&resource).await
            {
                tracing::warn!(%resource, %error, "reconcile failed");
            }
        }
    }
}
