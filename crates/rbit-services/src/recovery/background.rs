//! Recovery subscriber lifecycle: stop intake, finish the current event, then drain.

use super::RecoveryService;
use crate::{DomainEvent, ServiceError};
use rbit_kernel::TaskState;
use tokio::sync::broadcast::error::RecvError;

impl RecoveryService {
    /// Snapshot of the email subscriber only; disabled email leaves it idle.
    pub fn background_state(&self) -> TaskState {
        self.background.state()
    }

    /// Stops event intake and waits for any current account-mail operation to finish.
    /// Cancelling the wait leaves ownership intact; a later wait resumes draining.
    pub async fn shutdown(&self) {
        self.background.shutdown().await;
        if self.background.state() == TaskState::Failed {
            tracing::error!("recovery email subscriber failed before shutdown completed");
        }
    }

    // ------------------------------------------------------------------ background

    /// Starts this service's background subscribers. Called once at server start-up.
    ///
    /// Sends a verification link when an account is created, and when an address changes a
    /// link to the new address plus a notice to the previous one.
    /// Repeated calls share one subscriber. Embedders must call [`Self::shutdown`]
    /// before closing stores; dropping handles only signals cooperative cancellation.
    pub fn spawn_background(&self) {
        if !self.email_available() {
            tracing::info!("email is disabled; addresses will not be verified");
            return;
        }
        let mut events = self.ctx.events.subscribe();
        let service = self.email_events();
        let stop = self.background.stop_signal();
        self.background.start(async move {
            loop {
                let received = tokio::select! {
                    biased;
                    () = stop.cancelled() => break,
                    received = events.recv() => received,
                };
                match received {
                    Ok(event) => {
                        if let Err(error) = service.handle(&event).await {
                            tracing::warn!(?event, %error, "could not send email for event");
                        }
                    }
                    Err(RecvError::Lagged(skipped)) => tracing::warn!(skipped, "email subscriber fell behind; events skipped"),
                    Err(RecvError::Closed) => break,
                }
            }
        });
    }

    /// Reacts to one domain event; also used by tests without starting a subscriber.
    pub async fn handle(&self, event: &DomainEvent) -> Result<(), ServiceError> {
        self.email_events().handle(event).await
    }
}
