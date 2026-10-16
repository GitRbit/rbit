//! Administrator audit log: `/admin/audit`.
//!
//! Milestone 1 workstream S owns this file.

use axum::Router;

use crate::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
}
