//! Request metrics, exported in Prometheus format at `/metrics`.
//!
//! Milestone 1 workstream O owns this file.

use axum::{extract::Request, middleware::Next, response::Response};

/// Records one HTTP request. Applied to every route.
pub async fn track(request: Request, next: Next) -> Response {
    // TODO(workstream O): count requests by route and status, observe latency.
    next.run(request).await
}
