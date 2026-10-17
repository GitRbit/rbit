//! Pull request pages: `/{owner}/{repo}/pulls`, `/pulls/new`, `/pull/{n}` with its
//! Commits and Files changed tabs, review forms and the merge box (an issue number
//! answers 301 to `/issues/{n}`).
//!
//! Milestone 2 workstream P (pull requests) owns this file.
//!
//! Merged into the page router, so its routes get sessions, CSRF protection, security
//! headers and error pages automatically. Repository pages load the repository with
//! `super::repo::RepoPage::load` and build the header with `loaded.header(&state, tab)`.

use axum::Router;

use crate::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
}
