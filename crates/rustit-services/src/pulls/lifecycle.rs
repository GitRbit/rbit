//! Pull request state rules the shared timeline defers to.
//!
//! Milestone 2 workstream P (pull requests) owns this file.

use rustit_core::Issue;

use crate::{ServiceContext, ServiceError};

/// Called by `TimelineService::set_state` before reopening a pull request. Refuses (422,
/// `ServiceError::Invalid`) when it was merged, its head branch no longer exists, or
/// another open pull request has the same base and head. Stub: always allowed.
#[expect(clippy::unused_async, reason = "stub until the workstream implements the rules")]
pub async fn check_reopen(_ctx: &ServiceContext, _pull: &Issue) -> Result<(), ServiceError> {
    Ok(())
}
