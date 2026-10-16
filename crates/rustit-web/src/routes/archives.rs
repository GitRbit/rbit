//! Source archives: `/{owner}/{repo}/archive/{revision}.zip` and `.tar.gz`.
//!
//! Milestone 2 workstream R (repository essentials) owns this file.
//!
//! Merged outside the page router (archives stream for longer than the page timeout and
//! are already compressed), so this router applies the session and security-header
//! middleware itself.

use axum::Router;

use crate::AppState;

pub fn routes(_state: &AppState) -> Router<AppState> {
    Router::new()
}
