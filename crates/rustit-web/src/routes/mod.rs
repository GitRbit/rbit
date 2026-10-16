//! URL map and middleware stack.

mod account;
mod admin;
mod auth;
mod git_http;
mod history;
mod listing;
mod home;
mod orgs;
mod pwa;
mod repo;
mod repo_create;
mod repo_settings;
mod settings;
mod stars;

use std::time::Duration;

use axum::{
    Router,
    http::StatusCode,
    middleware::{from_fn, from_fn_with_state},
    routing::{get, post},
};
use tower_http::{compression::CompressionLayer, timeout::TimeoutLayer, trace::TraceLayer};

use crate::{AppState, assets, error::WebError, middleware};

/// HTML pages must render quickly; anything slower is a bug worth surfacing.
const PAGE_TIMEOUT: Duration = Duration::from_secs(30);

/// Builds the complete application router.
pub fn router(state: AppState) -> Router {
    Router::new()
        .merge(page_routes(&state))
        .merge(static_routes())
        // Git traffic is merged without compression or timeouts: packfiles are already
        // compressed and large clones legitimately take minutes.
        .merge(git_routes())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// Server-rendered HTML. Layers run outermost-first: security headers → CSRF check →
/// session resolution → error-page rendering → handler.
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
        .fallback(|| async { WebError::NotFound })
        .layer(from_fn_with_state(state.clone(), middleware::error_pages))
        .layer(from_fn_with_state(state.clone(), middleware::session))
        .layer(from_fn(middleware::csrf_origin_check))
        .layer(from_fn(middleware::security_headers))
        .layer(TimeoutLayer::with_status_code(StatusCode::REQUEST_TIMEOUT, PAGE_TIMEOUT))
        .layer(CompressionLayer::new())
}

fn static_routes() -> Router<AppState> {
    Router::new()
        .route("/assets/{*path}", get(assets::serve))
        .route("/manifest.webmanifest", get(pwa::manifest))
        .route("/sw.js", get(pwa::service_worker))
        .route("/healthz", get(|| async { "ok" }))
        .layer(CompressionLayer::new())
}

/// Git smart-HTTP endpoints (`git clone http://host/owner/repo.git`).
fn git_routes() -> Router<AppState> {
    Router::new()
        .route("/{owner}/{repo}/info/refs", get(git_http::info_refs))
        .route("/{owner}/{repo}/git-upload-pack", post(git_http::upload_pack))
        .route("/{owner}/{repo}/git-receive-pack", post(git_http::receive_pack))
}
