//! Issue list, issue page and form view models, and the sidebar shared with pull
//! request pages.
//!
//! Milestone 2 workstream I (issues) owns this file and `templates/issues/**`.
//!
//! Hook (`docs/design/milestone-2b.md` §3): P's pull request page renders [`sidebar`]
//! with `templates/issues/sidebar.html` (which expects `sidebar: IssueSidebar`).

use rustit_core::User;
use rustit_services::timeline::IssueContext;

use crate::AppState;

/// Assignees, labels, milestone, subscription and moderation controls beside an issue
/// or pull request. The default renders nothing.
#[derive(Debug, Clone, Default)]
pub struct IssueSidebar {}

/// The sidebar for `ctx` as `viewer` sees it. Stub: [`IssueSidebar::default`].
pub fn sidebar(_state: &AppState, _ctx: &IssueContext, _viewer: Option<&User>) -> IssueSidebar {
    IssueSidebar::default()
}
