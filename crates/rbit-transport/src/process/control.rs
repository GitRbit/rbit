//! One caller-owned cancellation domain and absolute session deadline.

use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use super::TransportError;

/// Cooperative process interruption, without an implicit queue or timeout policy.
/// Callers must await controlled operations through cleanup before releasing permits.
#[derive(Debug, Clone)]
pub struct ProcessControl {
    cancellation: CancellationToken,
    deadline: Instant,
}

impl ProcessControl {
    /// Uses the caller's shared stop domain and absolute deadline across all Git stages.
    pub fn new(cancellation: CancellationToken, deadline: Instant) -> Self {
        Self { cancellation, deadline }
    }

    pub(super) fn request_scope(&self) -> (Self, CancellationToken) {
        let request = self.cancellation.child_token();
        (Self::new(request.clone(), self.deadline), request)
    }

    /// Rejects an already interrupted stage before spawning Git.
    ///
    /// # Errors
    /// Returns `Cancelled` or `TimedOut`; neither starts a child process.
    pub fn check(&self) -> Result<(), TransportError> {
        if self.cancellation.is_cancelled() {
            Err(TransportError::Cancelled)
        } else if Instant::now() >= self.deadline {
            Err(TransportError::TimedOut)
        } else {
            Ok(())
        }
    }

    /// Resolves to `Cancelled` or `TimedOut` when the session must stop.
    /// This is a signal, not proof that any process has finished cleaning up.
    pub async fn stopped(&self) -> TransportError {
        tokio::select! {
            biased;
            () = self.cancellation.cancelled() => TransportError::Cancelled,
            () = tokio::time::sleep_until(self.deadline) => TransportError::TimedOut,
        }
    }
}

pub(super) fn check(control: Option<&ProcessControl>) -> Result<(), TransportError> {
    control.map_or(Ok(()), ProcessControl::check)
}

pub(super) async fn stopped(control: Option<&ProcessControl>) -> TransportError {
    match control {
        Some(control) => control.stopped().await,
        None => std::future::pending().await,
    }
}
