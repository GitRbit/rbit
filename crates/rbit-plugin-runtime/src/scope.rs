//! Cooperative task cancellation and bounded disposal for a plugin generation.

use std::{future::Future, sync::Arc, time::Duration};

use rbit_plugin_api::Generation;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::{Admission, AdmissionError, DrainError, OperationLease};

#[derive(Debug)]
struct Inner {
    admission: Admission,
    cancellation: CancellationToken,
}

impl Drop for Inner {
    fn drop(&mut self) {
        self.admission.close();
        self.cancellation.cancel();
    }
}

/// Owns asynchronous work for one activation of a plugin.
///
/// All asynchronous work must enter through [`Self::spawn`] or hold a lease obtained
/// through [`Self::try_acquire`]. Dropping the last scope handle closes admission and
/// signals cancellation, but only [`Self::dispose`] waits for cleanup to complete.
/// Tasks should retain their provided cancellation token rather than a scope clone,
/// which could otherwise keep the scope alive indefinitely.
#[derive(Debug, Clone)]
pub struct PluginScope {
    inner: Arc<Inner>,
}

impl PluginScope {
    pub fn new(generation: Generation) -> Self {
        Self { inner: Arc::new(Inner { admission: Admission::new(generation), cancellation: CancellationToken::new() }) }
    }

    pub fn generation(&self) -> Generation {
        self.inner.admission.generation()
    }

    /// Admission for host operations such as route handling or streamed response bodies.
    pub fn try_acquire(&self, generation: Generation) -> Result<OperationLease, AdmissionError> {
        self.inner.admission.try_acquire(generation)
    }

    pub fn is_closed(&self) -> bool {
        self.inner.admission.is_closed()
    }

    /// Includes tracked tasks and externally admitted host operations.
    pub fn in_flight(&self) -> usize {
        self.inner.admission.in_flight()
    }

    /// A child token signalled when this generation quiesces. Cancelling the returned
    /// token cannot close the whole plugin generation.
    pub fn cancellation_token(&self) -> CancellationToken {
        self.inner.cancellation.child_token()
    }

    /// Permanently closes admission and signals cancellation without waiting for work.
    ///
    /// A lifecycle controller can call this before plugin-specific asynchronous cleanup.
    /// The generation remains incomplete until [`Self::dispose`] confirms it drained.
    pub fn quiesce(&self) {
        self.inner.admission.close();
        self.inner.cancellation.cancel();
    }

    /// Starts an asynchronous task whose lifetime remains tracked even if its join handle
    /// is dropped. The task receives a cancellation token for cooperative cleanup.
    ///
    /// The factory must return promptly. Work that ignores cancellation remains tracked
    /// and causes disposal to time out; it is never silently detached from the scope.
    pub fn spawn<F, T>(&self, task: F) -> Result<JoinHandle<T::Output>, AdmissionError>
    where
        F: FnOnce(CancellationToken) -> T,
        T: Future + Send + 'static,
        T::Output: Send + 'static,
    {
        let lease = self.try_acquire(self.generation())?;
        let future = task(self.inner.cancellation.child_token());
        Ok(tokio::spawn(async move {
            let _lease = lease;
            future.await
        }))
    }

    /// Closes admission, signals cancellation and waits for admitted work to finish.
    ///
    /// Calls are idempotent and may wait concurrently. A timed-out scope stays closed;
    /// callers may inspect its surviving work and try waiting again, but must not report
    /// successful unload until this returns `Ok(())`.
    pub async fn dispose(&self, timeout: Duration) -> Result<(), DrainError> {
        self.quiesce();
        self.inner.admission.wait_for_drain(timeout).await
    }
}
