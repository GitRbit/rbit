//! Account security: `/settings/security` (two-factor auth, active sessions) and the sign-in second step.
//!
//! Milestone 1 workstream S owns this file.

use axum::Router;

use crate::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
}
