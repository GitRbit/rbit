//! SSH keys: `/settings/keys` and repository deploy keys `/{owner}/{repo}/settings/keys`.
//!
//! Milestone 1 workstream H owns this file.

use axum::Router;

use crate::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
}
