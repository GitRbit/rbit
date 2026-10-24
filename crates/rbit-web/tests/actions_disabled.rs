//! With `[actions] enabled = false` (the default), Actions does not exist: none of its pages
//! or runner RPCs is routed, no page shows a tab, card, nav entry or webhook event of it,
//! the `OpenAPI` document has no Actions path, and a task token is refused before anything is
//! looked up.
//!
//! The Actions foundation (workstream F) owns this file.

use std::sync::Arc;

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode, header},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use rbit_core::{Config, config::ActionsConfig};
use rbit_services::{CreateRepository, Services, SignUp};
use rbit_storage::{Database, KvStore};
use rbit_web::{
    AppState,
    routes::{actions, actions_runner},
};
use tower::ServiceExt;

use crate::query_count::QueryCounter;

struct Fixture {
    _directory: tempfile::TempDir,
    services: Services,
    router: Router,
    cookie: String,
}

impl Fixture {
    async fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.storage.data_dir = directory.path().join("data");
        config.security.rate_limit = false;
        std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
        let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
        let kv = KvStore::open(&config.storage.kv_path()).unwrap();
        let config = Arc::new(config);
        let services = Services::new(config.clone(), &database, &kv).await.unwrap();
        let alice = services
            .accounts
            .create_user(
                SignUp { username: "alice".into(), email: "alice@example.com".into(), password: "correct horse battery".into() },
                Some(true),
            )
            .await
            .unwrap();
        let input = CreateRepository { name: "rocket".into(), description: String::new(), visibility: rbit_core::Visibility::Public };
        services.repositories.create(&alice, input).await.unwrap();
        services
            .orgs
            .create(
                &alice,
                rbit_services::CreateOrganization { name: "acme".into(), display_name: String::new(), description: String::new() },
            )
            .await
            .unwrap();
        let token = services.accounts.start_session(&alice).await.unwrap();
        let cookie = format!("rbit_session={}", token.encode());
        let router = rbit_web::router(AppState::new(config, services.clone()));
        Self { _directory: directory, services, router, cookie }
    }

    async fn send(&self, method: Method, path: &str, extra: &[(header::HeaderName, &str)]) -> (StatusCode, String) {
        let mut request = Request::builder()
            .method(method)
            .uri(path)
            .header(header::COOKIE, &self.cookie)
            .header(header::HOST, "localhost:3000")
            .header(header::ORIGIN, "http://localhost:3000");
        for (name, value) in extra {
            request = request.header(name, *value);
        }
        let response = self.router.clone().oneshot(request.body(Body::empty()).unwrap()).await.unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (status, String::from_utf8_lossy(&body).into_owned())
    }

    async fn page(&self, path: &str) -> String {
        let (status, body) = self.send(Method::GET, path, &[]).await;
        assert_eq!(status, StatusCode::OK, "{path}");
        body
    }
}

const METHODS: [Method; 5] = [Method::GET, Method::POST, Method::PUT, Method::PATCH, Method::DELETE];

/// A path template with representative parameters.
fn concrete(template: &str) -> String {
    [("{owner}", "alice"), ("{repo}", "rocket"), ("{workflow}", "ci.yml"), ("{run}", "1"), ("{job}", "1"), ("{runner}", "1")]
        .into_iter()
        .fold(template.to_owned(), |path, (parameter, value)| path.replace(parameter, value))
}

#[test]
fn actions_are_off_by_default() {
    assert!(!ActionsConfig::default().enabled);
    assert!(!Config::default().actions.enabled);
}

#[tokio::test]
async fn no_actions_page_or_runner_rpc_is_routed() {
    let fixture = Fixture::new().await;
    assert!(fixture.services.actions.is_none());
    for (template, methods) in actions::PATHS {
        assert!(!methods.is_empty(), "{template}");
        let path = concrete(template);
        // The same path with the Actions segments renamed, which never existed: an Actions
        // path must answer exactly like it. (Two-segment paths such as `/admin/runners`
        // match `/{owner}/{repo}`, so methods other than GET answer 405 there, as for any
        // such path.)
        let control = path.replace("actions", "not-actions").replace("runners", "not-runners");
        for method in METHODS {
            let (status, _) = fixture.send(method.clone(), &path, &[]).await;
            let (expected, _) = fixture.send(method.clone(), &control, &[]).await;
            assert_eq!(status, expected, "{method} {path} answers like {control}");
            assert!(matches!(status, StatusCode::NOT_FOUND | StatusCode::METHOD_NOT_ALLOWED), "{method} {path}: {status}");
        }
        assert_eq!(fixture.send(Method::GET, &path, &[]).await.0, StatusCode::NOT_FOUND, "GET {path}");
    }
    for path in ["/alice/rocket/actions?workflow=ci.yml&status=success", "/acme/settings/actions"] {
        assert_eq!(fixture.send(Method::GET, path, &[]).await.0, StatusCode::NOT_FOUND, "{path}");
    }
    let proto = [(header::CONTENT_TYPE, "application/proto")];
    for path in actions_runner::PATHS.iter().chain(&["/api/actions/runner.v1.RunnerService/FetchSingleTask"]) {
        for method in METHODS {
            let (status, _) = fixture.send(method.clone(), path, &proto).await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{method} {path}: 404, not the Connect 501");
        }
    }
}

#[tokio::test]
async fn the_openapi_document_has_no_actions_path() {
    let fixture = Fixture::new().await;
    let (status, body) = fixture.send(Method::GET, "/api/v1/openapi.json", &[]).await;
    assert_eq!(status, StatusCode::OK);
    let document: serde_json::Value = serde_json::from_str(&body).unwrap();
    let actions: Vec<_> = document["paths"].as_object().unwrap().keys().filter(|path| path.contains("/actions")).collect();
    assert!(actions.is_empty(), "{actions:?}");
}

#[tokio::test]
async fn no_page_offers_actions() {
    let fixture = Fixture::new().await;
    for path in [
        "/alice/rocket",
        "/alice/rocket/settings",
        "/acme",
        "/acme/settings",
        "/settings/profile",
        "/settings/tokens",
        "/admin",
        "/admin/repositories",
        "/alice/rocket/settings/hooks/new",
        "/acme/settings/hooks/new",
    ] {
        let page = fixture.page(path).await;
        for marker in
            ["data-repo-tab=\"actions\"", "/alice/rocket/actions", "/settings/actions", "/admin/runners", "workflow_run", "workflow_job"]
        {
            assert!(!page.contains(marker), "{path} shows {marker}");
        }
    }
}

#[tokio::test]
async fn task_tokens_are_refused_without_a_lookup() {
    let fixture = Fixture::new().await;
    let task_token = format!("rbat_{}", "A".repeat(43));

    let basic = format!("Basic {}", STANDARD.encode(format!("x-access-token:{task_token}")));
    let counter = QueryCounter::new();
    let (status, _) = counter
        .measure(fixture.send(Method::GET, "/alice/rocket.git/info/refs?service=git-upload-pack", &[(header::AUTHORIZATION, &basic)]))
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(counter.queries().is_empty(), "no query for a task token on Git: {:?}", counter.queries());

    let bearer = format!("Bearer {task_token}");
    let counter = QueryCounter::new();
    let (status, _) = counter.measure(fixture.send(Method::GET, "/api/v1/user", &[(header::AUTHORIZATION, &bearer)])).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(counter.queries().is_empty(), "no query for a task token on the API: {:?}", counter.queries());

    // The spy sees queries: a personal access token is looked up.
    let counter = QueryCounter::new();
    let token = format!("Bearer rbit_{}", "A".repeat(43));
    let (status, _) = counter.measure(fixture.send(Method::GET, "/api/v1/user", &[(header::AUTHORIZATION, &token)])).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(!counter.queries().is_empty(), "the spy works");
}

#[tokio::test]
async fn no_background_work_starts() {
    let fixture = Fixture::new().await;
    fixture.services.spawn_background();
    assert!(fixture.services.actions.is_none());
    assert_eq!(fixture.services.actions_background_tasks(), 0);
    fixture.services.shutdown().await;
}
