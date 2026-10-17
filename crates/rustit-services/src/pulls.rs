//! Pull requests: creation across a fork network, `refs/pull/{n}/head`, head and base
//! updates, mergeability, reviews and merging on a temporary worktree.
//!
//! Milestone 2 workstream P (pull requests) owns this file and `pulls/**`.
//!
//! Hooks other code calls (signatures fixed by `docs/design/milestone-2b.md` §3):
//! [`hooks::payload`] (webhooks) and [`lifecycle::check_reopen`] (the timeline's reopen).

pub mod hooks;
pub mod json;
pub mod lifecycle;

use crate::ServiceContext;

#[derive(Debug, Clone)]
pub struct PullService {
    #[expect(dead_code, reason = "stub until the workstream implements the service")]
    ctx: ServiceContext,
}

impl PullService {
    pub(crate) fn new(ctx: ServiceContext) -> Self {
        Self { ctx }
    }

    /// Starts this service's background work (head and base updates on
    /// `RepositoryPushed`, the mergeability worker, the temporary-directory sweep). Called
    /// once from [`crate::Services::spawn_background`]. Stub until the workstream lands.
    pub fn spawn_background(&self) {}

    /// Stops background work (running merges and trial merges) and waits (bounded) for it
    /// to finish. Called from [`crate::Services::shutdown`] before the stores close.
    #[expect(clippy::unused_async, reason = "stub until the workstream implements background work")]
    pub async fn shutdown(&self) {}
}
