//! A generation admission gate and leases covering admitted operations.

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use rbit_plugin_api::Generation;
use tokio::sync::Notify;

/// Why an operation could not enter a plugin generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum AdmissionError {
    /// The caller retained a handle to another generation.
    #[error("plugin generation mismatch: requested {requested:?}, current {current:?}")]
    GenerationMismatch { requested: Generation, current: Generation },
    /// The generation is quiescing or has already been disposed.
    #[error("plugin generation is closed")]
    Closed,
    /// The operation counter cannot accept another lease.
    #[error("plugin operation capacity exhausted")]
    Capacity,
}

/// A drain never succeeds while new operations can still be admitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DrainError {
    /// Close admission before waiting for the generation to drain.
    #[error("plugin admission is still open")]
    Open,
    /// Admitted work survived the cleanup deadline.
    #[error("plugin drain timed out with {in_flight} operations still in flight")]
    Timeout { in_flight: usize },
}

#[derive(Debug, Default)]
struct State {
    closed: bool,
    in_flight: usize,
}

#[derive(Debug)]
struct Inner {
    generation: Generation,
    state: Mutex<State>,
    drained: Notify,
}

impl Inner {
    fn state(&self) -> MutexGuard<'_, State> {
        // No plugin code runs while this lock is held. Recovering a poisoned lock keeps
        // cleanup available even if a host thread panicked during an operation.
        self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// Cheaply cloneable admission gate for exactly one plugin generation.
///
/// Closing and acquiring use the same short critical section. Once [`Self::close`]
/// returns, even callers holding an older routing snapshot cannot acquire a new lease.
#[derive(Debug, Clone)]
pub struct Admission {
    inner: Arc<Inner>,
}

impl Admission {
    pub fn new(generation: Generation) -> Self {
        Self { inner: Arc::new(Inner { generation, state: Mutex::default(), drained: Notify::new() }) }
    }

    pub fn generation(&self) -> Generation {
        self.inner.generation
    }

    /// Acquires authority to finish one operation on the requested generation.
    ///
    /// Keep the returned lease until the operation's final side effect or response body
    /// completes. The lease does not confer repository permissions; the host must still
    /// enforce authorization and revocation before committing mutations.
    pub fn try_acquire(&self, requested: Generation) -> Result<OperationLease, AdmissionError> {
        if requested != self.inner.generation {
            return Err(AdmissionError::GenerationMismatch { requested, current: self.inner.generation });
        }
        let mut state = self.inner.state();
        if state.closed {
            return Err(AdmissionError::Closed);
        }
        state.in_flight = state.in_flight.checked_add(1).ok_or(AdmissionError::Capacity)?;
        Ok(OperationLease { inner: Arc::clone(&self.inner) })
    }

    /// Permanently rejects new operations. Calling this repeatedly is harmless.
    pub fn close(&self) {
        self.inner.state().closed = true;
    }

    pub fn is_closed(&self) -> bool {
        self.inner.state().closed
    }

    pub fn in_flight(&self) -> usize {
        self.inner.state().in_flight
    }

    /// Waits for all leases to be dropped, bounded by `timeout`.
    ///
    /// A timeout leaves the gate closed and the outstanding leases tracked. It does not
    /// abort tasks, free native code or make a surviving operation safe to forget.
    pub async fn wait_for_drain(&self, timeout: Duration) -> Result<(), DrainError> {
        if !self.is_closed() {
            return Err(DrainError::Open);
        }
        let drained = async {
            loop {
                let notification = self.inner.drained.notified();
                tokio::pin!(notification);
                // Register before checking the count so the final lease cannot drop
                // between the check and the notification being registered.
                notification.as_mut().enable();
                if self.in_flight() == 0 {
                    return;
                }
                notification.await;
            }
        };
        tokio::time::timeout(timeout, drained).await.map_err(|_| DrainError::Timeout { in_flight: self.in_flight() })
    }
}

/// Non-cloneable ownership of one admitted operation.
///
/// Drop ends the operation even when a future is cancelled, a task panics or a response
/// stream is abandoned. This type deliberately exposes no way to reopen the generation.
#[derive(Debug)]
pub struct OperationLease {
    inner: Arc<Inner>,
}

impl OperationLease {
    pub fn generation(&self) -> Generation {
        self.inner.generation
    }
}

impl Drop for OperationLease {
    fn drop(&mut self) {
        let mut state = self.inner.state();
        state.in_flight -= 1;
        if state.in_flight == 0 {
            self.inner.drained.notify_waiters();
        }
    }
}
