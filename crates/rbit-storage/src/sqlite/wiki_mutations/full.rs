//! One owned FULL-synchronous writer; successful results acknowledge SQL only.
pub mod fencing;
#[cfg(test)]
mod fencing_tests;
mod jobs;
mod opening;
mod retention;
mod shutdown;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_quarantine;
#[cfg(test)]
mod tests_shutdown;

use super::{
    AcceptancePolicy, WikiMutationError, accept_on,
    ownership::{WikiCatalogLease, WikiCatalogLeaseError},
};
use crate::sqlite::pool::Pool;
use rbit_core::Timestamp;
use rbit_wikis::{
    WikiStoreId,
    commands::{WikiIntent, WikiWriteCommand},
    ownership::WikiCatalogOwnership,
};
use std::{
    fmt,
    future::Future,
    num::NonZeroU32,
    path::Path,
    sync::{Arc, Mutex},
};
use tokio::sync::{Semaphore, watch};
use tokio_util::task::TaskTracker;

/// A narrow clone retains the actual catalog lease, not a writer controller.
#[derive(Clone)]
pub struct WikiCatalogHandle(Arc<WikiCatalogLease>);
impl fmt::Debug for WikiCatalogHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("WikiCatalogHandle(..)")
    }
}
impl WikiCatalogOwnership for WikiCatalogHandle {
    type Error = WikiCatalogLeaseError;
    fn store_id(&self) -> WikiStoreId {
        self.0.store_id()
    }
    fn catalog_path(&self) -> &Path {
        self.0.catalog_path()
    }
    fn check_bound(&self) -> Result<(), Self::Error> {
        self.0.check_bound()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum WikiWriteError {
    #[error("wiki writer is busy")]
    Busy,
    #[error("wiki writer is closed")]
    Closed,
    #[error("wiki writer shutdown is unproved; process restart is required")]
    Quarantined,
    #[error("wiki catalog ownership failed")]
    Catalog(#[from] WikiCatalogLeaseError),
    #[error("wiki SQL connection failed")]
    Sql(#[from] sqlx::Error),
    #[error("wiki command acceptance failed")]
    Acceptance(WikiMutationError),
    /// No successful receipt is returned. Preserve request identity and reconcile.
    #[error("wiki worker completion is uncertain")]
    Uncertain,
    /// Exact pending bytes were found, but their originating synchronization mode
    /// is unknown. Preserve the same command; startup/replay recovery must fence it.
    #[error("pending wiki acceptance requires verified replay synchronization")]
    ReplayRequiresFence,
    #[error("wiki writes require the closed recovery owner")]
    FencedOwnerRequired,
    #[error("wiki protocol or object ownership does not match")]
    Binding,
    #[error("wiki replay synchronization did not complete")]
    ReplaySyncIncomplete,
    #[error("wiki protocol epoch exhausted")]
    EpochExhausted,
}

/// Constructed only after the FULL-connection acceptance transaction committed.
/// This is not Git durability, visible publication, auth or a validated prior head.
pub struct CommittedWikiAcceptance {
    intent: WikiIntent,
    // Last: exclusion survives the exact accepted payload/result lifetime.
    _catalog: WikiCatalogHandle,
}
impl fmt::Debug for CommittedWikiAcceptance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CommittedWikiAcceptance(..)")
    }
}
impl CommittedWikiAcceptance {
    pub fn intent(&self) -> &WikiIntent {
        &self.intent
    }
}

struct Writer {
    pool: sqlx::SqlitePool,
    commands: Pool,
    gate: Mutex<bool>,
    admission: Arc<Semaphore>,
    tasks: TaskTracker,
    catalog: WikiCatalogHandle,
    state: Arc<retention::NativeState>,
    protocol_claimed: std::sync::atomic::AtomicBool,
    protocol_permission: Arc<std::sync::atomic::AtomicBool>,
    completion: watch::Receiver<shutdown::Completion>,
    // Last: losing the last controller wakes the real pool-close supervisor.
    shutdown: watch::Sender<bool>,
}

/// Explicitly close/drain before stopping the runtime. Dropped callers do not abort
/// admitted commands; preserve the same request ID and reconcile uncertain ACKs.
#[derive(Clone)]
pub struct FullWikiWriter(Arc<Writer>);
impl fmt::Debug for FullWikiWriter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("FullWikiWriter(..)")
    }
}
impl FullWikiWriter {
    /// Consumes the noncloneable lease once. Opening itself is owned through caller
    /// cancellation; it cannot leak a detached writer or release its lease early.
    pub async fn open(lease: WikiCatalogLease) -> Result<Self, WikiWriteError> {
        tokio::spawn(opening::open(lease)).await.map_err(|_| WikiWriteError::Uncertain)?
    }

    pub fn catalog(&self) -> WikiCatalogHandle {
        self.0.catalog.clone()
    }

    /// Admission and worker ownership occur synchronously, before returning the
    /// future. At most one command exists; there is no wait queue before the pool.
    pub fn accept(
        &self,
        command: WikiWriteCommand,
        at: Timestamp,
        max_pending: NonZeroU32,
    ) -> impl Future<Output = Result<CommittedWikiAcceptance, WikiWriteError>> + Send + use<> {
        let job = self.start_operation(jobs::Admission::Legacy, move |pool, catalog| async move {
            let accepted =
                accept_on(&pool, command, at, max_pending, AcceptancePolicy::AllowNew).await.map_err(WikiWriteError::Acceptance)?;
            if !accepted.inserted {
                return Err(WikiWriteError::ReplayRequiresFence);
            }
            Ok(CommittedWikiAcceptance { intent: accepted.intent, _catalog: catalog })
        });
        async move { job?.await.map_err(|_| WikiWriteError::Uncertain)? }
    }

    /// Close admission and wait for actual worker-future destruction, then for the
    /// dedicated pool to close. Cancellation of this waiter leaves ownership intact.
    /// Returned receipts/native resources can intentionally still retain the lease.
    pub async fn close(&self) -> Result<(), WikiWriteError> {
        {
            let mut closed = self.0.gate.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            *closed = true;
            self.0.tasks.close();
            self.0.shutdown.send_replace(true);
        }
        let mut completion = self.0.completion.clone();
        loop {
            match *completion.borrow_and_update() {
                shutdown::Completion::Closed => return Ok(()),
                shutdown::Completion::Unproved => return Err(WikiWriteError::Quarantined),
                shutdown::Completion::Waiting => {}
            }
            completion.changed().await.map_err(|_| WikiWriteError::Uncertain)?;
        }
    }
}
