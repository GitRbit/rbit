//! `issues` and `issue_comment` webhook payloads.
//!
//! Milestone 2 workstream I (issues) owns this file.
//!
//! [`crate::WebhookService::handle_event`] calls [`payload`] for
//! `DomainEvent::Issue { kind: IssueKind::Issue, .. }` and every
//! `DomainEvent::IssueComment` (comments on pull requests included: their `issue` object
//! carries a `pull_request` key). It queues the returned body for every hook subscribed
//! to [`HookPayload::event`] on the repository or its organisation.

use crate::{DomainEvent, ServiceContext, ServiceError, webhooks::HookPayload};

/// The payload for `event`, or `None` when the event produces no delivery (or the issue
/// or repository is already gone). Stub: always `None`.
#[expect(clippy::unused_async, reason = "stub until the workstream implements the payloads")]
pub async fn payload(_ctx: &ServiceContext, _event: &DomainEvent, _base_url: &str) -> Result<Option<HookPayload>, ServiceError> {
    Ok(None)
}
