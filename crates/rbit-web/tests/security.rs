//! Regressions for redirect validation and the service worker's shared offline page.

use std::sync::Arc;

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use rbit_core::Config;
use rbit_services::{Services, SignUp};
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
