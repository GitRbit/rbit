//! The versioned REST API: `/api/v1/…`, JSON, authenticated by personal access tokens.
//!
//! Milestone 2 workstream W (API & webhooks) owns this file.
//!
//! Merged outside the page router: no session cookies, no CSRF check, no HTML error
//! pages. This router brings its own layers (authentication, rate limiting, body limit,
//! timeout, JSON errors). It must not set a fallback (the page router owns that); answer
//! unknown `/api/…` paths from an explicit catch-all route instead.

use axum::Router;

use crate::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
}
