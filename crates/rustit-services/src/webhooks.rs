//! Webhooks: configuration, event payloads, HMAC-signed deliveries with retries and
//! the delivery log. Deliveries are driven by `DomainEvent`s.
//!
//! Milestone 2 workstream W (API & webhooks) owns this file.

use crate::ServiceContext;

#[derive(Debug, Clone)]
pub struct WebhookService {
    #[expect(dead_code, reason = "stub until the workstream implements the service")]
    ctx: ServiceContext,
}

impl WebhookService {
    pub(crate) fn new(ctx: ServiceContext) -> Self {
        Self { ctx }
    }

    /// Starts this service's background work (event subscribers, schedules). Called once
    /// from [`crate::Services::spawn_background`]. Stub until the workstream lands.
    pub fn spawn_background(&self) {}

    /// Stops background work and waits (bounded) for it to finish. Called from
    /// [`crate::Services::shutdown`] before the stores close.
    #[expect(clippy::unused_async, reason = "stub until the workstream implements background work")]
    pub async fn shutdown(&self) {}
}
