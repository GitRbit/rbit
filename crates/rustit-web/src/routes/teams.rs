//! Organisation teams: `/{org}/teams`, `/{org}/teams/{team}` and their settings.
//!
//! Milestone 2 workstream A (access) owns this file.
//!
//! Merged into the page router, so its routes get sessions, CSRF protection, security
//! headers and error pages automatically.

use axum::Router;

use crate::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
}
