//! The timeline shared by issues and pull requests: opening a thread for a viewer,
//! comments, state changes, locking and the merged list of comments and events.
//!
//! The integrator owns this file. P0 declares only [`IssueContext`], which the hooks
//! (`views::issues::sidebar`) take; T0 adds `TimelineService` and the rest of
//! `docs/design/milestone-2b.md` §1.

use rbit_core::{Account, Issue, IssuePermissions};

use crate::RepoContext;

/// An issue or pull request opened for one viewer: its repository (already
/// access-checked), the row, its author and what the viewer may do with it.
#[derive(Debug, Clone)]
pub struct IssueContext {
    pub repo: RepoContext,
    pub issue: Issue,
    /// `None` once the author's account is deleted.
    pub author: Option<Account>,
    pub permissions: IssuePermissions,
}
