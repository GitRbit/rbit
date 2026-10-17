//! URL map and middleware stack.

mod account;
mod admin;
mod api;
mod archives;
mod audit;
mod auth;
mod blame;
mod collaborators;
mod compare;
mod forks;
mod git_http;
mod history;
mod home;
mod issues;
mod labels;
mod listing;
mod milestones;
mod mirrors;
mod notifications;
mod ops;
mod orgs;
mod protection;
mod pulls;
mod pwa;
mod recovery;
mod releases;
mod repo;
mod repo_create;
mod repo_settings;
mod security;
mod settings;
mod ssh_keys;
mod stars;
mod teams;
mod themes;
mod transfer;
mod webhooks;

use std::time::Duration;

use axum::{
    Router,
    extract::DefaultBodyLimit,
    http::StatusCode,
    middleware::{from_fn, from_fn_with_state},
    routing::{get, post},
};
use tower_http::{compression::CompressionLayer, timeout::TimeoutLayer, trace::TraceLayer};

use crate::{AppState, assets, error::WebError, middleware};

/// HTML pages must render quickly; anything slower is a bug worth surfacing.
const PAGE_TIMEOUT: Duration = Duration::from_secs(30);

/// Upper bound for buffered request bodies on HTML routes (Git traffic streams instead).
const MAX_FORM_BYTES: usize = 1024 * 1024;

/// Builds the complete application router.
pub fn router(state: AppState) -> Router {
    Router::new()
        .merge(page_routes(&state))
        .merge(static_routes())
        // Git traffic is merged without compression or timeouts: packfiles are already
        // compressed and large clones legitimately take minutes.
        .merge(git_routes())
        // Milestone 2: the REST API and large downloads/uploads bring their own layers.
        .merge(api::routes())
        .merge(archives::routes(&state))
        .merge(releases::file_routes(&state))
        // Before any route: a hover preload must never reach a side-effecting handler.
        .layer(from_fn(middleware::preload_guard))
        .layer(from_fn(crate::metrics::track))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// Server-rendered HTML. Layers run outermost-first: render timing → compression → timeout → cache headers →
/// security headers → CSRF check → session resolution → error-page rendering → handler.
fn page_routes(state: &AppState) -> Router<AppState> {
    Router::new()
        .route("/", get(home::index))
        .route("/explore", get(home::explore))
        .route("/login", get(auth::login_form).post(auth::login))
        .route("/signup", get(auth::signup_form).post(auth::signup))
        .route("/logout", post(auth::logout))
        .route("/new", get(repo_create::new_form).post(repo_create::create))
        .route("/offline", get(pwa::offline))
        .route("/{owner}", get(account::profile))
        .route("/{owner}/{repo}", get(repo::home))
        .route("/{owner}/{repo}/tree/{*rest}", get(repo::tree))
        .route("/{owner}/{repo}/tree-commits/{*rest}", get(repo::entry_commits))
        .route("/{owner}/{repo}/blob/{*rest}", get(repo::blob))
        .route("/{owner}/{repo}/raw/{*rest}", get(repo::raw))
        .route("/{owner}/{repo}/commits", get(repo::commits_default))
        .route("/{owner}/{repo}/commits/{*rest}", get(repo::commits))
        .route("/{owner}/{repo}/commit/{revision}", get(history::commit))
        .route("/{owner}/{repo}/branches", get(history::branches))
        .route("/{owner}/{repo}/tags", get(history::tags))
        // Feature areas own their route tables.
        .merge(settings::routes())
        .merge(orgs::routes())
        .merge(admin::routes())
        .merge(repo_settings::routes())
        .merge(stars::routes())
        .merge(security::routes())
        .merge(audit::routes())
        .merge(recovery::routes())
        .merge(ssh_keys::routes())
        // Milestone 2 wave A.
        .merge(collaborators::routes())
        .merge(teams::routes())
        .merge(transfer::routes())
        .merge(webhooks::routes())
        .merge(releases::routes())
        .merge(blame::routes())
        .merge(compare::routes())
        .merge(forks::routes())
        .merge(mirrors::routes())
        // Frontend wave: themes (T).
        .merge(themes::routes())
        // Milestone 2 wave B (the shared timeline routes arrive with T0).
        .merge(issues::routes())
        .merge(labels::routes())
        .merge(milestones::routes())
        .merge(notifications::routes())
        .merge(pulls::routes())
        .merge(protection::routes())
        .fallback(|| async { WebError::NotFound })
        .layer(from_fn_with_state(state.clone(), middleware::error_pages))
        .layer(from_fn_with_state(state.clone(), middleware::session))
        .layer(from_fn_with_state(state.clone(), middleware::rate_limit))
        .layer(from_fn(middleware::csrf_origin_check))
        .layer(from_fn(middleware::security_headers))
        // Inside compression, so ETags are computed over the uncompressed page.
        .layer(from_fn(middleware::html_caching))
        .layer(TimeoutLayer::with_status_code(StatusCode::REQUEST_TIMEOUT, PAGE_TIMEOUT))
        .layer(CompressionLayer::new())
        // Forms are small; this bounds memory per request for every extractor that buffers.
        .layer(DefaultBodyLimit::max(MAX_FORM_BYTES))
        // Outermost, so the footer's render time covers every layer that runs before the page.
        .layer(from_fn(crate::timing::track))
}

fn static_routes() -> Router<AppState> {
    Router::new()
        .route("/assets/{*path}", get(assets::serve))
        .route("/manifest.webmanifest", get(pwa::manifest))
        .route("/sw.js", get(pwa::service_worker))
        .merge(ops::routes())
        .layer(CompressionLayer::new())
}

/// Git smart-HTTP endpoints (`git clone http://host/owner/repo.git`).
fn git_routes() -> Router<AppState> {
    Router::new()
        .route("/{owner}/{repo}/info/refs", get(git_http::info_refs))
        .route("/{owner}/{repo}/git-upload-pack", post(git_http::upload_pack))
        .route("/{owner}/{repo}/git-receive-pack", post(git_http::receive_pack))
}
