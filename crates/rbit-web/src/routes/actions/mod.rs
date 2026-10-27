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

pub(crate) mod jobs;
mod runners;
mod runs;
mod settings;

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
    // Actions: artifacts (A).
    ("/{owner}/{repo}/actions/runs/{run}/artifacts/{artifact}", &[Method::GET]),
    ("/{owner}/{repo}/settings/actions", &[Method::GET, Method::POST]),
    ("/{owner}/{repo}/settings/actions/runners", &[Method::GET, Method::POST]),
    ("/{owner}/{repo}/settings/actions/runners/{runner}", &[Method::POST]),
    ("/{owner}/{repo}/settings/actions/secrets", &[Method::GET, Method::POST]),
    ("/{owner}/{repo}/settings/actions/variables", &[Method::GET, Method::POST]),
    ("/{owner}/{repo}/settings/actions/trusted", &[Method::POST]),
    ("/{owner}/settings/actions", &[Method::GET, Method::POST]),
    ("/{owner}/settings/actions/runners", &[Method::GET, Method::POST]),
    ("/{owner}/settings/actions/runners/{runner}", &[Method::POST]),
    ("/settings/actions", &[Method::GET, Method::POST]),
    ("/settings/actions/runners", &[Method::GET, Method::POST]),
    ("/settings/actions/runners/{runner}", &[Method::POST]),
    ("/admin/runners", &[Method::GET, Method::POST]),
    ("/admin/runners/{runner}", &[Method::POST]),
];

// Actions: artifacts (A).
/// Artifact downloads. Merged outside the page router (no 30-second timeout, no
/// compression: archives are already gzipped), only when Actions is enabled, so this
/// router applies the session, rate-limit, CSRF and security-header middleware itself.
pub fn file_routes(state: &AppState) -> Router<AppState> {
    use axum::middleware::{from_fn, from_fn_with_state};
    Router::new()
        .route("/{owner}/{repo}/actions/runs/{run}/artifacts/{artifact}", get(runs::artifact))
        .layer(from_fn_with_state(state.clone(), crate::middleware::error_pages))
        .layer(from_fn_with_state(state.clone(), crate::middleware::session))
        .layer(from_fn_with_state(state.clone(), crate::middleware::rate_limit))
        .layer(from_fn(crate::middleware::csrf_origin_check))
        .layer(from_fn_with_state(crate::middleware::SecurityHeaders::new(&state.config), crate::middleware::security_headers))
}

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
        .route("/{owner}/{repo}/settings/actions/runners", get(runners::repository).post(runners::repository_tokens))
        .route("/{owner}/{repo}/settings/actions/runners/{runner}", post(runners::repository_runner))
        .route("/{owner}/settings/actions/runners", get(runners::organization).post(runners::organization_tokens))
        .route("/{owner}/settings/actions/runners/{runner}", post(runners::organization_runner))
        .route("/settings/actions/runners", get(runners::personal).post(runners::personal_tokens))
        .route("/settings/actions/runners/{runner}", post(runners::personal_runner))
        .route("/admin/runners", get(runners::instance).post(runners::instance_tokens))
        .route("/admin/runners/{runner}", post(runners::instance_runner))
        .route("/{owner}/{repo}/actions/runs/{run}/jobs/{job}", get(jobs::show))
        .route("/{owner}/{repo}/actions/runs/{run}/jobs/{job}/logs", get(jobs::logs))
        .route("/{owner}/{repo}/actions/runs/{run}/jobs/{job}/logs/raw", get(jobs::raw))
        .route("/{owner}/{repo}/settings/actions", get(settings::repository).post(settings::save_repository))
        .route("/{owner}/{repo}/settings/actions/secrets", get(settings::to_repository).post(settings::repository_secret))
        .route("/{owner}/{repo}/settings/actions/variables", get(settings::to_repository).post(settings::repository_variable))
        .route("/{owner}/{repo}/settings/actions/trusted", post(settings::untrust))
        .route("/{owner}/settings/actions", get(settings::organization).post(settings::save_organization))
        .route("/settings/actions", get(settings::personal).post(settings::save_personal))
}
