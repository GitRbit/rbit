//! rbit Actions pages: the repository's runs, a run, a job and its logs, the dispatch form,
//! and the settings for runners, secrets and variables at repository, owner and instance
//! level.
//!
//! [`routes`] is merged into the page router only when `Services::actions` exists, so with
//! Actions disabled every path below answers 404 like any unknown page. [`PATHS`] lists every
//! path template this module serves, with its methods; the disabled-is-disabled tests walk
//! it.
//!
//! The Actions foundation (workstream F) owns the route list; workstream U (interface and
//! secrets) owns the handlers.

mod runs;

use axum::{
    Router,
    http::Method,
    routing::{get, post},
};

use crate::AppState;

/// Every page of Actions, with the methods it answers.
pub const PATHS: &[(&str, &[Method])] = &[
    ("/{owner}/{repo}/actions", &[Method::GET]),
    ("/{owner}/{repo}/actions/workflows/{workflow}", &[Method::GET]),
    ("/{owner}/{repo}/actions/workflows/{workflow}/dispatch", &[Method::POST]),
    ("/{owner}/{repo}/actions/runs/{run}", &[Method::GET]),
    ("/{owner}/{repo}/actions/runs/{run}/jobs", &[Method::GET]),
    ("/{owner}/{repo}/actions/runs/{run}/cancel", &[Method::POST]),
    ("/{owner}/{repo}/actions/runs/{run}/rerun", &[Method::POST]),
    ("/{owner}/{repo}/actions/runs/{run}/rerun-failed", &[Method::POST]),
    ("/{owner}/{repo}/actions/runs/{run}/approve", &[Method::POST]),
    ("/{owner}/{repo}/actions/runs/{run}/jobs/{job}", &[Method::GET]),
    ("/{owner}/{repo}/actions/runs/{run}/jobs/{job}/rerun", &[Method::POST]),
    ("/{owner}/{repo}/actions/runs/{run}/jobs/{job}/logs", &[Method::GET]),
    ("/{owner}/{repo}/actions/runs/{run}/jobs/{job}/logs/raw", &[Method::GET]),
    ("/{owner}/{repo}/settings/actions", &[Method::GET, Method::POST]),
    ("/{owner}/{repo}/settings/actions/runners", &[Method::GET, Method::POST]),
    ("/{owner}/{repo}/settings/actions/runners/{runner}", &[Method::POST]),
    ("/{owner}/{repo}/settings/actions/secrets", &[Method::GET, Method::POST]),
    ("/{owner}/{repo}/settings/actions/variables", &[Method::GET, Method::POST]),
    ("/{owner}/{repo}/settings/actions/trusted", &[Method::POST]),
    ("/{owner}/settings/actions", &[Method::GET, Method::POST]),
    ("/settings/actions", &[Method::GET, Method::POST]),
    ("/admin/runners", &[Method::GET, Method::POST]),
    ("/admin/runners/{runner}", &[Method::POST]),
];

/// The Actions pages. Merged only when Actions is enabled (see `routes::page_routes`).
pub fn routes() -> Router<AppState> {
    // Actions: interface and secrets (U).
    Router::new()
        .route("/{owner}/{repo}/actions", get(runs::list))
        .route("/{owner}/{repo}/actions/workflows/{workflow}", get(runs::workflow))
        .route("/{owner}/{repo}/actions/runs/{run}", get(runs::show))
        .route("/{owner}/{repo}/actions/runs/{run}/jobs", get(runs::jobs))
        .route("/{owner}/{repo}/actions/runs/{run}/cancel", post(runs::cancel))
        .route("/{owner}/{repo}/actions/runs/{run}/rerun", post(runs::rerun_all))
        .route("/{owner}/{repo}/actions/runs/{run}/rerun-failed", post(runs::rerun_failed))
        .route("/{owner}/{repo}/actions/runs/{run}/approve", post(runs::approve))
        .route("/{owner}/{repo}/actions/runs/{run}/jobs/{job}/rerun", post(runs::rerun_job))
}
