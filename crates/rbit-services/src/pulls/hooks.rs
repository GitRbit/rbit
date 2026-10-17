//! `pull_request` and `pull_request_review` webhook payloads.
//!
//! Milestone 2 workstream P (pull requests) owns this file.
//!
//! [`crate::WebhookService::handle_event`] calls [`payload`] for
//! `DomainEvent::Issue { kind: IssueKind::Pull, .. }` (including the label, milestone and
//! assignee actions I's services publish for pull requests) and every
//! `DomainEvent::PullRequestReview`.

use crate::{DomainEvent, ServiceContext, ServiceError, webhooks::HookPayload};

/// The payload for `event`, or `None` when the event produces no delivery (or the pull
/// request or repository is already gone). Stub: always `None`.
#[expect(clippy::unused_async, reason = "stub until the workstream implements the payloads")]
pub async fn payload(_ctx: &ServiceContext, _event: &DomainEvent, _base_url: &str) -> Result<Option<HookPayload>, ServiceError> {
    Ok(None)
}
