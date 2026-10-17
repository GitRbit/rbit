//! Issue pages: `/{owner}/{repo}/issues`, `/issues/new`, `/issues/{n}` (a pull request
//! number answers 301 to `/pull/{n}`), plus the sidebar's forms (labels, assignees,
//! milestone, subscription, deletion).
//!
//! Milestone 2 workstream I (issues) owns this file.
//!
//! Merged into the page router, so its routes get sessions, CSRF protection, security
//! headers and error pages automatically. Repository pages load the repository with
//! `super::repo::RepoPage::load` and build the header with `loaded.header(&state, tab)`.

use axum::Router;

use crate::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
}
