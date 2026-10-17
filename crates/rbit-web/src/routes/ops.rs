//! Operational endpoints for load balancers, orchestrators and monitoring.
//!
//! * `GET /healthz` — JSON `{status, database, kv}`; 503 when SQLite or redb fails a
//!   trivial read. Suitable for container health checks.
//! * `GET /readyz` — the same checks plus the repository directory; use it to decide
//!   whether to route traffic to this instance.
//! * `GET /metrics` — Prometheus text format, only with `[operations] metrics = true`
//!   (404 otherwise). With `metrics_token` set it requires `Authorization: Bearer <token>`.
//!
//! None of these touch sessions or render HTML, and they are never cached.

use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/healthz", get(healthz)).route("/readyz", get(readyz)).route("/metrics", get(metrics))
}

#[derive(Debug, Serialize)]
struct HealthBody {
    status: &'static str,
    database: &'static str,
    kv: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    repositories: Option<&'static str>,
}

fn check(ok: bool) -> &'static str {
    if ok { "ok" } else { "error" }
}

/// 200 with `status: "ok"` when every check passed, otherwise 503 `"unavailable"`.
/// `repositories` is only checked (and reported) by `/readyz`.
fn health_response(database: bool, kv: bool, repositories: Option<bool>) -> Response {
    let healthy = database && kv && repositories.unwrap_or(true);
    let status = if healthy { StatusCode::OK } else { StatusCode::SERVICE_UNAVAILABLE };
    let body = HealthBody {
        status: if healthy { "ok" } else { "unavailable" },
        database: check(database),
        kv: check(kv),
        repositories: repositories.map(check),
    };
    (status, [(header::CACHE_CONTROL, HeaderValue::from_static("no-store"))], Json(body)).into_response()
}

/// `GET /healthz`: are the database and the key/value store answering?
async fn healthz(State(state): State<AppState>) -> Response {
    let report = state.services.jobs.health().await;
    health_response(report.database, report.kv, None)
}

/// `GET /readyz`: healthy, and the repository directory is reachable.
async fn readyz(State(state): State<AppState>) -> Response {
    let report = state.services.jobs.health().await;
    let repositories = tokio::fs::metadata(state.config.storage.repositories_dir()).await.is_ok_and(|meta| meta.is_dir());
    health_response(report.database, report.kv, Some(repositories))
}

/// `GET /metrics`: Prometheus text exposition format.
async fn metrics(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let operations = &state.config.operations;
    if !operations.metrics {
        return StatusCode::NOT_FOUND.into_response();
    }
    if let Some(token) = operations.metrics_token()
        && !bearer_matches(&headers, token)
    {
        return (StatusCode::UNAUTHORIZED, [(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"))]).into_response();
    }
    (
        [
            (header::CONTENT_TYPE, HeaderValue::from_static("text/plain; version=0.0.4; charset=utf-8")),
            (header::CACHE_CONTROL, HeaderValue::from_static("no-store")),
        ],
        crate::metrics::render(),
    )
        .into_response()
}

/// Whether `Authorization: Bearer …` carries `expected`. Digests are compared so the
/// comparison time does not depend on how much of the token matched.
fn bearer_matches(headers: &HeaderMap, expected: &str) -> bool {
    let Some(presented) = headers.get(header::AUTHORIZATION).and_then(|value| value.to_str().ok()) else { return false };
    let Some((scheme, token)) = presented.split_once(' ') else { return false };
    scheme.eq_ignore_ascii_case("bearer") && Sha256::digest(token.trim().as_bytes()) == Sha256::digest(expected.as_bytes())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::body::{Body, to_bytes};
    use rbit_core::Config;
    use rbit_services::Services;
    use rbit_storage::{Database, KvStore};
    use tower::ServiceExt;

    use super::*;

    /// The full application router over a temporary data directory.
    async fn app(configure: impl FnOnce(&mut Config)) -> (tempfile::TempDir, Router) {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.storage.data_dir = dir.path().to_owned();
        configure(&mut config);
        std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
        let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
        let kv = KvStore::open(&config.storage.kv_path()).unwrap();
        let config = Arc::new(config);
        let services = Services::new(Arc::clone(&config), &database, &kv).await.unwrap();
        (dir, crate::router(AppState::new(config, services)))
    }

    async fn get(app: &Router, uri: &str, authorization: Option<&str>) -> (StatusCode, String) {
        let mut request = axum::http::Request::builder().uri(uri);
        if let Some(value) = authorization {
            request = request.header(header::AUTHORIZATION, value);
        }
        let response = app.clone().oneshot(request.body(Body::empty()).unwrap()).await.unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (status, String::from_utf8(body.to_vec()).unwrap())
    }

    #[tokio::test]
    async fn health_endpoints_report_each_store() {
        let (_dir, app) = app(|_| {}).await;
        let (status, body) = get(&app, "/healthz", None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, r#"{"status":"ok","database":"ok","kv":"ok"}"#);
        let (status, body) = get(&app, "/readyz", None).await;
        assert_eq!(status, StatusCode::OK);
        assert!(body.contains(r#""repositories":"ok""#), "{body}");
    }

    #[tokio::test]
    async fn metrics_are_hidden_unless_enabled() {
        let (_dir, app) = app(|_| {}).await;
        assert_eq!(get(&app, "/metrics", None).await.0, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn metrics_honour_the_bearer_token() {
        let (_dir, app) = app(|config| {
            config.operations.metrics = true;
            config.operations.metrics_token = Some("s3cret".into());
        })
        .await;
        get(&app, "/healthz", None).await;
        assert_eq!(get(&app, "/metrics", None).await.0, StatusCode::UNAUTHORIZED);
        assert_eq!(get(&app, "/metrics", Some("Bearer wrong")).await.0, StatusCode::UNAUTHORIZED);
        let (status, body) = get(&app, "/metrics", Some("Bearer s3cret")).await;
        assert_eq!(status, StatusCode::OK);
        assert!(body.contains(r#"route="/healthz""#), "{body}");
    }

    fn headers(value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(header::AUTHORIZATION, HeaderValue::from_str(value).unwrap());
        headers
    }

    #[test]
    fn bearer_tokens_must_match_exactly() {
        assert!(bearer_matches(&headers("Bearer s3cret"), "s3cret"));
        assert!(bearer_matches(&headers("bearer s3cret"), "s3cret"));
        assert!(!bearer_matches(&headers("Bearer s3cre"), "s3cret"));
        assert!(!bearer_matches(&headers("Basic s3cret"), "s3cret"));
        assert!(!bearer_matches(&HeaderMap::new(), "s3cret"));
    }

    #[test]
    fn any_failed_check_makes_the_instance_unavailable() {
        assert_eq!(health_response(true, true, None).status(), StatusCode::OK);
        assert_eq!(health_response(true, true, Some(true)).status(), StatusCode::OK);
        assert_eq!(health_response(false, true, None).status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(health_response(true, false, None).status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(health_response(true, true, Some(false)).status(), StatusCode::SERVICE_UNAVAILABLE);
    }
}
