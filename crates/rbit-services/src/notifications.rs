//! In-app notifications: participation-based threads per (user, issue), the unread
//! indicator in the header and the `/notifications` inbox.
//!
//! Milestone 2 workstream I (issues) owns this file.
//!
//! Hooks (`docs/design/milestone-2b.md` §3): the `Page` extractor calls
//! [`NotificationService::has_unread`] for signed-in users on every page; I's issue page
//! and P's pull request page call [`NotificationService::mark_thread_read`].

use rbit_core::{AccountId, IssueId};

use crate::{ServiceContext, ServiceError};

#[derive(Debug, Clone)]
pub struct NotificationService {
    #[expect(dead_code, reason = "stub until the workstream implements the service")]
    ctx: ServiceContext,
}

impl NotificationService {
    pub(crate) fn new(ctx: ServiceContext) -> Self {
        Self { ctx }
    }

    /// Whether `user` has an unread notification (one indexed `EXISTS`). Stub: `false`.
    #[expect(clippy::unused_async, clippy::unused_async_trait_impl, reason = "stub until the workstream implements the service")]
    pub async fn has_unread(&self, _user: AccountId) -> Result<bool, ServiceError> {
        Ok(false)
    }

    /// Marks `user`'s notification about `issue` read, if any. Failures are logged, never
    /// returned: it runs as a side effect of viewing a page. Stub: does nothing.
    #[expect(clippy::unused_async, reason = "stub until the workstream implements the service")]
    pub async fn mark_thread_read(&self, _user: AccountId, _issue: IssueId) {}

    /// Starts the notification fan-out subscriber and the daily pruning. Called once from
    /// [`crate::Services::spawn_background`]. Stub until the workstream lands.
    pub fn spawn_background(&self) {}

    /// Stops background work and waits (bounded) for it to finish. Called from
    /// [`crate::Services::shutdown`] before the stores close.
    #[expect(clippy::unused_async, reason = "stub until the workstream implements background work")]
    pub async fn shutdown(&self) {}
}
