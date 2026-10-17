//! Branch protection settings: `/{owner}/{repo}/settings/branches`, `/settings/branches/new`,
//! `/settings/branches/{id}`.
//!
//! Milestone 2 workstream B (branch protection) owns this file.
//!
//! Merged into the page router, so its routes get sessions, CSRF protection, security
//! headers and error pages automatically. Repository pages load the repository with
//! `super::repo::RepoPage::load` and build the header with `loaded.header(&state, tab)`.

use axum::Router;

use crate::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
}
