//! Immediate admission for synchronous CPU or filesystem work on Tokio's blocking pool.

use std::{num::NonZeroUsize, sync::Arc};

use tokio::{
    sync::{Semaphore, TryAcquireError},
    task::JoinHandle,
};

/// Why a worker was not submitted. Rejections never enqueue work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum AdmissionError {
    /// All configured slots are held by queued or running workers.
    #[error("blocking work capacity is exhausted")]
    Busy,
    /// The owner has stopped accepting work, usually during shutdown.
    #[error("blocking work admission is closed")]
    Closed,
}

/// Shared capacity for one class of expensive synchronous operations.
///
/// Clones share the same limit. Admission never waits, so incoming requests cannot
/// build an unbounded queue behind this budget. A slot covers both time waiting in
/// Tokio's blocking pool and time executing the operation.
///
/// Dropping a request, timing out its response or aborting the returned handle does
/// not cancel an already-running operation. Its slot stays held until the closure
/// returns or unwinds. Early interruption requires cooperative cancellation.
#[derive(Debug, Clone)]
pub struct BlockingBudget {
    slots: Arc<Semaphore>,
    capacity: NonZeroUsize,
}

impl BlockingBudget {
    /// Creates a budget with explicitly nonzero capacity.
    ///
    /// # Panics
    ///
    /// Panics if `capacity` exceeds [`Semaphore::MAX_PERMITS`].
    pub fn new(capacity: NonZeroUsize) -> Self {
        Self { slots: Arc::new(Semaphore::new(capacity.get())), capacity }
    }

    /// Submits `operation` only when a slot is immediately available.
    ///
    /// The closure owns the permit, not the future awaiting its result. Tokio's
    /// [`JoinHandle`] preserves panic/cancellation errors separately from the
    /// operation's own return value (which may itself be a `Result`).
    ///
    /// `tokio::time::timeout_at` can bound response waiting without releasing the
    /// slot early. Reject expired requests before submission when work should not
    /// start at all. Closing admission does not stop existing work.
    ///
    /// # Errors
    ///
    /// Returns [`AdmissionError::Busy`] on saturation or [`AdmissionError::Closed`]
    /// after [`Self::close`]. The rejected closure is dropped without being run.
    ///
    /// # Panics
    ///
    /// Panics outside a Tokio runtime, just like [`tokio::task::spawn_blocking`].
    pub fn try_spawn<F, T>(&self, operation: F) -> Result<JoinHandle<T>, AdmissionError>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        let permit = Arc::clone(&self.slots).try_acquire_owned().map_err(|error| match error {
            TryAcquireError::NoPermits => AdmissionError::Busy,
            TryAcquireError::Closed => AdmissionError::Closed,
        })?;
        Ok(tokio::task::spawn_blocking(move || {
            let _permit = permit;
            operation()
        }))
    }

    /// Stops new admission across every clone; queued/running work retains its slot.
    ///
    /// A racing submission may already have acquired a slot and is admitted. The
    /// owner must await its tracked jobs before tearing down their dependencies.
    pub fn close(&self) {
        self.slots.close();
    }

    /// Whether this budget has stopped accepting work.
    pub fn is_closed(&self) -> bool {
        self.slots.is_closed()
    }

    /// Configured maximum of admitted workers.
    pub fn capacity(&self) -> NonZeroUsize {
        self.capacity
    }

    /// Slots not currently held, for diagnostics rather than admission decisions.
    ///
    /// This snapshot can change immediately and may be nonzero after closure.
    pub fn available(&self) -> usize {
        self.slots.available_permits()
    }
}
