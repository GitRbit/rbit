//! Operational endpoints: `/healthz`, `/metrics`.
//!
//! Milestone 1 workstream O owns this file.

use axum::{Router, routing::get};

use crate::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/healthz", get(|| async { "ok" }))
}
