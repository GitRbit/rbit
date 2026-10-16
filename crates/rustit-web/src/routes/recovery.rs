//! Password reset and email verification: `/password/forgot`, `/password/reset/{token}`, `/email/verify/{token}`.
//!
//! Milestone 1 workstream E owns this file.

use axum::Router;

use crate::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
}
