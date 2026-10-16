//! Releases: `/{owner}/{repo}/releases`, `/releases/tag/{tag}`, `/releases/new`, editing.
//!
//! File uploads and downloads are not here: see [`file_routes`].
//!
//! Milestone 2 workstream R (repository essentials) owns this file.
//!
//! Merged into the page router, so its routes get sessions, CSRF protection, security
//! headers and error pages automatically.

use axum::Router;

use crate::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
}

/// Attached-file uploads and downloads. Merged outside the page router (no 30-second
/// timeout, no 1 MiB body limit, no compression), so this router applies the session,
/// CSRF and security-header middleware itself.
pub fn file_routes(_state: &AppState) -> Router<AppState> {
    Router::new()
}
