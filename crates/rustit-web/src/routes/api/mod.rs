//! The versioned REST API: `/api/v1/…`, JSON, authenticated by personal access tokens.
//!
//! Milestone 2 workstream W (API & webhooks) owns this file.
//!
//! Merged outside the page router: no session cookies, no CSRF check, no HTML error
//! pages. This router brings its own layers (1 MiB bodies, a 30 s timeout, gzip,
//! `nosniff`, `no-store`); authentication and rate limiting happen in the [`Caller`]
//! extractor every handler takes, JSON errors in [`ApiError`]. It sets no fallback (the
//! page router owns that): unknown `/api/…` paths reach an explicit catch-all instead.
//!
//! Conventions: `snake_case` fields, numeric `id`s, RFC 3339 UTC timestamps, absolute URLs
//! from `server.public_url`, explicit `null`s; lists are paginated with `?page=` and
//! `?per_page=` and answer with `Link` and `X-Total-Count` (see [`pagination`]).
//!
//! The routes come from one table, [`table`]; `openapi.json` documents the same set, and
//! a test keeps the two equal.

mod accounts;
mod auth;
mod error;
mod hooks;
mod json;
mod pagination;
mod repos;

use std::{borrow::Cow, time::Duration};

use auth::Caller;
use axum::{
    Router,
    extract::DefaultBodyLimit,
    handler::Handler,
    http::{HeaderValue, StatusCode, header},
    response::IntoResponse,
    routing::{MethodFilter, MethodRouter, any, on},
};
use error::ApiError;
use tower_http::{compression::CompressionLayer, set_header::SetResponseHeaderLayer, timeout::TimeoutLayer};

use crate::AppState;

/// Every route is under this prefix.
pub const PREFIX: &str = "/api/v1";
/// The hand-written `OpenAPI` 3.1 description of [`table`].
const OPENAPI: &str = include_str!("openapi.json");
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_BODY_BYTES: usize = 1024 * 1024;

/// The HTTP methods the API uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ApiMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

impl ApiMethod {
    #[cfg(test)]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Get => "get",
            Self::Post => "post",
            Self::Put => "put",
            Self::Patch => "patch",
            Self::Delete => "delete",
        }
    }

    const fn filter(self) -> MethodFilter {
        match self {
            Self::Get => MethodFilter::GET,
            Self::Post => MethodFilter::POST,
            Self::Put => MethodFilter::PUT,
            Self::Patch => MethodFilter::PATCH,
            Self::Delete => MethodFilter::DELETE,
        }
    }
}

/// One endpoint: method, path below [`PREFIX`] (axum syntax) and handler.
pub struct ApiRoute {
    pub method: ApiMethod,
    pub path: Cow<'static, str>,
    handler: MethodRouter<AppState>,
}

impl std::fmt::Debug for ApiRoute {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApiRoute").field("method", &self.method).field("path", &self.path).finish_non_exhaustive()
    }
}

fn route<H, T>(method: ApiMethod, path: impl Into<Cow<'static, str>>, handler: H) -> ApiRoute
where
    H: Handler<T, AppState>,
    T: 'static,
{
    ApiRoute { method, path: path.into(), handler: on(method.filter(), handler) }
}

/// Every endpoint of the API.
pub fn table() -> Vec<ApiRoute> {
    use ApiMethod::{Delete, Get, Patch, Post, Put};
    let mut routes = vec![
        route(Get, "/version", accounts::version),
        route(Get, "/openapi.json", openapi),
        route(Get, "/user", accounts::me),
        route(Get, "/user/repos", accounts::my_repositories),
        route(Post, "/user/repos", accounts::create_for_user),
        route(Get, "/user/starred", accounts::starred),
        route(Put, "/user/starred/{owner}/{repo}", accounts::star),
        route(Delete, "/user/starred/{owner}/{repo}", accounts::unstar),
        route(Get, "/users/{username}", accounts::user),
        route(Get, "/users/{username}/repos", accounts::user_repositories),
        route(Get, "/orgs/{org}", accounts::organization),
        route(Get, "/orgs/{org}/members", accounts::members),
        route(Get, "/orgs/{org}/repos", accounts::org_repositories),
        route(Post, "/orgs/{org}/repos", accounts::create_for_org),
        route(Get, "/repos/{owner}/{repo}", repos::show),
        route(Patch, "/repos/{owner}/{repo}", repos::update),
        route(Delete, "/repos/{owner}/{repo}", repos::delete),
        route(Get, "/repos/{owner}/{repo}/branches", repos::branches),
        route(Get, "/repos/{owner}/{repo}/branches/{*branch}", repos::branch),
        route(Get, "/repos/{owner}/{repo}/tags", repos::tags),
        route(Get, "/repos/{owner}/{repo}/commits", repos::commits),
        route(Get, "/repos/{owner}/{repo}/commits/{*reference}", repos::commit),
    ];
    for prefix in ["/repos/{owner}/{repo}/hooks", "/orgs/{org}/hooks"] {
        let path = |suffix: &str| format!("{prefix}{suffix}");
        routes.extend([
            route(Get, path(""), hooks::list),
            route(Post, path(""), hooks::create),
            route(Get, path("/{id}"), hooks::show),
            route(Patch, path("/{id}"), hooks::update),
            route(Delete, path("/{id}"), hooks::delete),
            route(Post, path("/{id}/pings"), hooks::ping),
            route(Get, path("/{id}/deliveries"), hooks::deliveries),
            route(Get, path("/{id}/deliveries/{delivery}"), hooks::delivery),
            route(Post, path("/{id}/deliveries/{delivery}/attempts"), hooks::redeliver),
        ]);
    }
    routes
}

pub fn routes() -> Router<AppState> {
    let mut router = Router::new();
    for entry in table() {
        router = router.route(&format!("{PREFIX}{}", entry.path), entry.handler);
    }
    router
        .route("/api", any(not_found))
        .route("/api/", any(not_found))
        .route("/api/{*rest}", any(not_found))
        .layer(SetResponseHeaderLayer::if_not_present(header::CACHE_CONTROL, HeaderValue::from_static("no-store")))
        .layer(SetResponseHeaderLayer::overriding(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff")))
        .layer(CompressionLayer::new())
        .layer(TimeoutLayer::with_status_code(StatusCode::REQUEST_TIMEOUT, REQUEST_TIMEOUT))
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
}

/// `GET /openapi.json`
async fn openapi(_caller: Caller) -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "application/json")], OPENAPI)
}

/// Every other path under `/api`.
async fn not_found() -> ApiError {
    ApiError::not_found()
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeSet, sync::Arc};

    use axum::{
        body::Body,
        http::{Request, Response},
    };
    use rustit_core::{Config, TokenScope, User};
    use rustit_services::{CreateAccessToken, CreateRepository, Services, SignUp};
    use rustit_storage::{Database, KvStore};
    use serde_json::Value;
    use tower::ServiceExt;

    use super::*;

    /// `{*branch}` in axum is `{branch}` in `OpenAPI`.
    fn openapi_path(path: &str) -> String {
        path.replace("{*", "{")
    }

    #[test]
    fn openapi_documents_exactly_the_routes() {
        let document: Value = serde_json::from_str(OPENAPI).expect("openapi.json is valid JSON");
        assert_eq!(document["openapi"].as_str().map(|v| v.starts_with("3.1")), Some(true), "OpenAPI 3.1");
        assert_eq!(document["servers"][0]["url"], PREFIX);

        let methods = ["get", "post", "put", "patch", "delete", "head", "options", "trace"];
        let mut documented = BTreeSet::new();
        for (path, item) in document["paths"].as_object().expect("paths") {
            for method in methods {
                if let Some(operation) = item.get(method) {
                    assert!(operation["operationId"].is_string(), "{method} {path} has an operationId");
                    assert!(operation["responses"].is_object(), "{method} {path} documents its responses");
                    documented.insert((method.to_owned(), path.clone()));
                }
            }
        }
        let routed: BTreeSet<_> = table().iter().map(|route| (route.method.as_str().to_owned(), openapi_path(&route.path))).collect();
        let missing: Vec<_> = routed.difference(&documented).collect();
        let extra: Vec<_> = documented.difference(&routed).collect();
        assert!(missing.is_empty() && extra.is_empty(), "undocumented: {missing:?}; documented but not routed: {extra:?}");
        assert_eq!(routed.len(), table().len(), "no route is listed twice");
    }

    struct Fixture {
        _dir: tempfile::TempDir,
        services: Services,
        router: Router,
    }

    impl Fixture {
        async fn new(configure: impl FnOnce(&mut Config)) -> Self {
            let dir = tempfile::tempdir().unwrap();
            let mut config = Config::default();
            config.storage.data_dir = dir.path().join("data");
            "https://git.example.com".clone_into(&mut config.server.public_url);
            configure(&mut config);
            std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
            let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
            let kv = KvStore::open(&config.storage.kv_path()).unwrap();
            let config = Arc::new(config);
            let services = Services::new(Arc::clone(&config), &database, &kv).await.unwrap();
            let router = crate::router(AppState::new(config, services.clone()));
            Self { _dir: dir, services, router }
        }

        async fn user(&self, name: &str) -> User {
            let signup = SignUp { username: name.into(), email: format!("{name}@example.com"), password: "correct horse battery".into() };
            self.services.accounts.create_user(signup, Some(false)).await.unwrap()
        }

        async fn token(&self, user: &User, scope: TokenScope) -> String {
            let input = CreateAccessToken { name: format!("{scope:?}"), scope, expires_in_days: None };
            self.services.tokens.create(user, input).await.unwrap().1
        }

        async fn send(&self, method: &str, path: &str, token: Option<&str>, body: Option<&str>) -> (Response<Body>, Value) {
            let mut request = Request::builder().method(method).uri(path);
            if let Some(token) = token {
                request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
            }
            if body.is_some() {
                request = request.header(header::CONTENT_TYPE, "application/json");
            }
            let request = request.body(body.map_or_else(Body::empty, |body| Body::from(body.to_owned()))).unwrap();
            let response = self.router.clone().oneshot(request).await.unwrap();
            let (parts, body) = response.into_parts();
            let bytes = axum::body::to_bytes(body, usize::MAX).await.unwrap();
            let json = if bytes.is_empty() { Value::Null } else { serde_json::from_slice(&bytes).unwrap_or(Value::Null) };
            (Response::from_parts(parts, Body::empty()), json)
        }
    }

    #[tokio::test]
    async fn authentication_scopes_and_errors() {
        let fixture = Fixture::new(|_| {}).await;
        let alice = fixture.user("alice").await;
        let read = fixture.token(&alice, TokenScope::Read).await;
        let write = fixture.token(&alice, TokenScope::Write).await;

        let (response, json) = fixture.send("GET", "/api/v1/version", None, None).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(json["api"], "v1");
        assert_eq!(response.headers()[header::CONTENT_TYPE], "application/json");
        assert_eq!(response.headers()[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
        assert!(response.headers().get(header::SET_COOKIE).is_none(), "the API never sets cookies");

        let (response, json) = fixture.send("GET", "/api/v1/user", None, None).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert!(response.headers()[header::WWW_AUTHENTICATE].to_str().unwrap().starts_with("Bearer"));
        assert_eq!(json["message"], "Requires authentication");
        let (response, _) = fixture.send("GET", "/api/v1/user", Some("rit_not-a-real-token"), None).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let (response, json) = fixture.send("GET", "/api/v1/user", Some(&read), None).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!((json["login"].as_str(), json["email"].as_str()), (Some("alice"), Some("alice@example.com")));
        assert_eq!(json["url"], "https://git.example.com/api/v1/users/alice");

        // read → GET only; write may create.
        let body = r#"{"name":"demo","private":true}"#;
        let (response, json) = fixture.send("POST", "/api/v1/user/repos", Some(&read), Some(body)).await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN, "{json}");
        let (response, json) = fixture.send("POST", "/api/v1/user/repos", Some(&write), Some(body)).await;
        assert_eq!(response.status(), StatusCode::CREATED, "{json}");
        assert_eq!(response.headers()[header::LOCATION], "https://git.example.com/api/v1/repos/alice/demo");
        assert_eq!((json["full_name"].as_str(), json["private"].as_bool()), (Some("alice/demo"), Some(true)));
        let (response, json) = fixture.send("POST", "/api/v1/user/repos", Some(&write), Some(body)).await;
        assert_eq!(response.status(), StatusCode::CONFLICT, "{json}");
        let (response, json) = fixture.send("POST", "/api/v1/user/repos", Some(&write), Some(r#"{"name":"bad name"}"#)).await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(json["errors"][0]["field"], "name");

        // Bodies: malformed, the wrong type, too large.
        let (response, _) = fixture.send("POST", "/api/v1/user/repos", Some(&write), Some("{not json")).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let huge = format!(r#"{{"name":"x","description":"{}"}}"#, "a".repeat(MAX_BODY_BYTES));
        let (response, _) = fixture.send("POST", "/api/v1/user/repos", Some(&write), Some(&huge)).await;
        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);

        // Admin actions need the admin scope even for the owner.
        let (response, json) = fixture.send("PATCH", "/api/v1/repos/alice/demo", Some(&write), Some(r#"{"description":"x"}"#)).await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert!(json["message"].as_str().unwrap().contains("admin scope"));
        let (response, json) = fixture.send("GET", "/api/v1/repos/alice/demo", Some(&read), None).await;
        assert_eq!(json["permissions"], serde_json::json!({ "admin": false, "push": false, "pull": true }), "capped by the read scope");
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn hidden_things_are_404_and_unknown_paths_answer_json() {
        let fixture = Fixture::new(|_| {}).await;
        let alice = fixture.user("alice").await;
        let bob = fixture.user("bob").await;
        let input = CreateRepository { name: "vault".to_owned(), description: String::new(), visibility: rustit_core::Visibility::Private };
        fixture.services.repositories.create(&alice, input).await.unwrap();
        let bobs = fixture.token(&bob, TokenScope::Admin).await;

        for path in [
            "/api/v1/repos/alice/vault",
            "/api/v1/repos/alice/vault/branches",
            "/api/v1/repos/alice/vault/hooks",
            "/api/v1/repos/alice/nope",
        ] {
            let (response, json) = fixture.send("GET", path, Some(&bobs), None).await;
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
            assert_eq!(json["message"], "Not Found");
        }
        let (response, _) = fixture.send("DELETE", "/api/v1/repos/alice/vault", Some(&bobs), None).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let (response, _) = fixture.send("GET", "/api/v1/repos/alice/vault", None, None).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "anonymous callers too");

        for path in ["/api/v1/nope", "/api/v2/user", "/api", "/api/"] {
            let (response, json) = fixture.send("GET", path, None, None).await;
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
            assert_eq!(json["message"], "Not Found", "{path}");
        }
        let (response, json) = fixture.send("GET", "/api/v1/openapi.json", None, None).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert!(json["paths"].is_object());
    }

    #[tokio::test]
    async fn lists_are_paginated_with_links_and_totals() {
        let fixture = Fixture::new(|config| config.api.max_page_size = 2).await;
        let alice = fixture.user("alice").await;
        for name in ["a", "b", "c"] {
            let input = CreateRepository { name: name.to_owned(), description: String::new(), visibility: rustit_core::Visibility::Public };
            fixture.services.repositories.create(&alice, input).await.unwrap();
        }
        let (response, json) = fixture.send("GET", "/api/v1/users/alice/repos?per_page=50", None, None).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(json.as_array().unwrap().len(), 2, "per_page is capped by max_page_size");
        assert_eq!(response.headers()["x-total-count"], "3");
        let link = response.headers()[header::LINK].to_str().unwrap();
        assert!(link.contains("<https://git.example.com/api/v1/users/alice/repos?per_page=2&page=2>; rel=\"next\""), "{link}");
        assert!(link.contains("rel=\"last\""));

        let (response, json) = fixture.send("GET", "/api/v1/users/alice/repos?per_page=2&page=2", None, None).await;
        assert_eq!(json.as_array().unwrap().len(), 1);
        assert!(response.headers()[header::LINK].to_str().unwrap().contains("rel=\"prev\""));
        let (response, json) = fixture.send("GET", "/api/v1/users/alice/repos?page=x", None, None).await;
        assert_eq!((response.status(), json["errors"][0]["field"].as_str()), (StatusCode::UNPROCESSABLE_ENTITY, Some("page")));
    }

    #[tokio::test]
    async fn a_disabled_api_is_not_there() {
        let fixture = Fixture::new(|config| config.api.enabled = false).await;
        for path in ["/api/v1/version", "/api/v1/openapi.json", "/api/v1/user"] {
            let (response, _) = fixture.send("GET", path, None, None).await;
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
        }
    }
}
