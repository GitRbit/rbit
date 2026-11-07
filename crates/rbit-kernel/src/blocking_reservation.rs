//! Owned admission between an application's attempt decision and worker submission.

use tokio::{sync::OwnedSemaphorePermit, task::JoinHandle};

/// One admitted operation, movable but neither cloneable nor reusable.
///
/// Reserve before consuming a one-use proof or performing an attempt side effect.
/// Dropping an unsubmitted reservation releases its capacity. Once spawned, the
/// blocking worker owns it until completion, including cancellation of its waiter.
#[derive(Debug)]
pub struct BlockingReservation {
    _permit: OwnedSemaphorePermit,
}

impl BlockingReservation {
    pub(crate) fn new(permit: OwnedSemaphorePermit) -> Self {
        Self { _permit: permit }
    }

    /// Consumes admitted capacity to submit one synchronous operation to Tokio.
    ///
    /// There is no second admission decision: closing the budget after reservation
    /// does not revoke this already-admitted attempt. The reservation moves into the
    /// closure before submission and remains held through blocking-pool queueing,
    /// operation return or panic unwind. The handle preserves the operation's return
    /// value separately from Tokio join errors.
    ///
    /// A response timeout or abort cannot stop a running blocking operation. Long
    /// operations need cooperative cancellation; never treat a dropped handle as
    /// evidence that its capacity has been released.
    ///
    /// # Panics
    ///
    /// Panics outside a Tokio runtime. Unwinding that failed submission drops the
    /// unstarted reservation and releases capacity. A panic=abort process terminates.
    pub fn spawn<F, T>(self, operation: F) -> JoinHandle<T>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        tokio::task::spawn_blocking(move || {
            let _reservation = self;
            operation()
        })
    }
}
