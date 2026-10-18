//! Regressions for redirect validation and the service worker's shared offline page.

use std::sync::Arc;

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use rbit_core::{Config, Visibility};
use rbit_services::{CreateRepository, Services, SignUp};
use rbit_storage::{Database, KvStore};
use rbit_web::AppState;
use tower::ServiceExt;

struct Fixture {
    _directory: tempfile::TempDir,
    services: Services,
    router: Router,
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
        let router = rbit_web::router(AppState::new(config, services.clone()));
        Self { _directory: directory, services, router }
    }
}

#[tokio::test]
async fn ref_picker_page_and_fragment_enforce_repository_visibility() {
    let fixture = Fixture::new().await;
    let owner = fixture
        .services
        .accounts
        .create_user(
            SignUp { username: "picker-owner".into(), email: "picker@example.com".into(), password: "correct horse battery".into() },
            Some(false),
        )
        .await
        .unwrap();
    let token = fixture.services.accounts.start_session(&owner).await.unwrap();
    for (name, visibility) in [("public", Visibility::Public), ("private", Visibility::Private)] {
        fixture
            .services
            .repositories
            .create(&owner, CreateRepository { name: name.into(), description: String::new(), visibility })
            .await
            .unwrap();
        for suffix in ["ref-picker", "ref-picker/options"] {
            for signed_in in [false, true] {
                let mut request = Request::builder().uri(format!("/picker-owner/{name}/{suffix}?q=missing&page=999"));
                if signed_in {
                    request = request.header(header::COOKIE, format!("rbit_session={}", token.encode()));
                }
                let response = fixture.router.clone().oneshot(request.body(Body::empty()).unwrap()).await.unwrap();
                let expected = if name == "private" && !signed_in { StatusCode::NOT_FOUND } else { StatusCode::OK };
                assert_eq!(response.status(), expected, "{name}/{suffix}, signed in: {signed_in}");
                if expected == StatusCode::OK {
                    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
                    assert!(String::from_utf8_lossy(&body).contains("No matching branches or tags."));
                }
            }
        }
    }
}

#[tokio::test]
async fn anonymous_theme_changes_reject_hostile_redirects_without_panicking() {
    let fixture = Fixture::new().await;
    for next in ["%2F%0A%2Fevil.example", "%2F%09%2Fevil.example", "%2F%00", "%2F%7F"] {
        let request = Request::builder()
            .method("POST")
            .uri("/themes")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from(format!("theme=dark&next={next}")))
            .unwrap();
        let response = fixture.router.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        assert_eq!(response.headers()[header::LOCATION], "/");
    }
}

#[tokio::test]
async fn offline_page_is_identical_for_signed_in_and_anonymous_visitors() {
    let fixture = Fixture::new().await;
    let user = fixture
        .services
        .accounts
        .create_user(
            SignUp {
                username: "private-offline-user".into(),
                email: "offline@example.com".into(),
                password: "correct horse battery".into(),
            },
            Some(true),
        )
        .await
        .unwrap();
    let token = fixture.services.accounts.start_session(&user).await.unwrap();
    let anonymous = fixture.router.clone().oneshot(Request::builder().uri("/offline").body(Body::empty()).unwrap()).await.unwrap();
    let signed_in = fixture
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/offline")
                .header(header::COOKIE, format!("rbit_session={}; rbit-theme=dark", token.encode()))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(signed_in.status(), StatusCode::OK);
    assert_eq!(signed_in.headers()[header::ETAG], anonymous.headers()[header::ETAG]);
    let html = axum::body::to_bytes(signed_in.into_body(), usize::MAX).await.unwrap();
    assert!(!String::from_utf8_lossy(&html).contains("private-offline-user"));
}

#[tokio::test]
async fn partial_pages_revalidate_private_access_before_honoring_cached_validators() {
    let fixture = Fixture::new().await;
    let user = fixture
        .services
        .accounts
        .create_user(
            SignUp { username: "partial-owner".into(), email: "partial@example.com".into(), password: "correct horse battery".into() },
            Some(true),
        )
        .await
        .unwrap();
    fixture
        .services
        .repositories
        .create(&user, CreateRepository { name: "private-plans".into(), description: String::new(), visibility: Visibility::Private })
        .await
        .unwrap();
    let token = fixture.services.accounts.start_session(&user).await.unwrap();
    let cookie = format!("rbit_session={}", token.encode());
    let path = "/partial-owner/private-plans";
    let request = |etag: Option<&str>| {
        let mut request = Request::builder()
            .uri(path)
            .header(header::COOKIE, &cookie)
            .header("HX-Request", "true")
            .header("Rbit-Partial", path)
            .header("Rbit-Nav-Epoch", "integration-test");
        if let Some(etag) = etag {
            request = request.header(header::IF_NONE_MATCH, etag);
        }
        request.body(Body::empty()).unwrap()
    };
    let partial = fixture.router.clone().oneshot(request(None)).await.unwrap();
    assert_eq!(partial.status(), StatusCode::OK);
    assert_eq!(partial.headers()["rbit-partial"], "true");
    assert_eq!(partial.headers()[header::CACHE_CONTROL], "private, no-cache");
    let vary = partial.headers()[header::VARY].to_str().unwrap();
    for name in ["HX-Request", "Rbit-Partial", "Rbit-Nav-Epoch"] {
        assert!(vary.contains(name), "{vary}");
    }
    let etag = partial.headers()[header::ETAG].to_str().unwrap().to_owned();
    let html = axum::body::to_bytes(partial.into_body(), usize::MAX).await.unwrap();
    let html = String::from_utf8_lossy(&html);
    assert!(html.starts_with("<title>") && html.contains("private-plans"));
    assert!(!html.contains("<footer") && !html.contains("<!doctype"));
    let cached = fixture.router.clone().oneshot(request(Some(&etag))).await.unwrap();
    assert_eq!(cached.status(), StatusCode::NOT_MODIFIED);

    fixture.services.accounts.end_session(&token).await.unwrap();
    let revoked = fixture.router.clone().oneshot(request(Some(&etag))).await.unwrap();
    assert_eq!(revoked.status(), StatusCode::NOT_FOUND, "a validator cannot bypass revoked access");
    assert_eq!(revoked.headers()[header::CACHE_CONTROL], "private, no-cache");
    assert!(revoked.headers().get("rbit-partial").is_none());
    assert!(!String::from_utf8_lossy(&axum::body::to_bytes(revoked.into_body(), usize::MAX).await.unwrap()).contains("id=\"repo-subnav\""));
}
