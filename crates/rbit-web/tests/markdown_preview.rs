//! Previews use saved-content rendering and enforce repository privacy and page protections.

use std::sync::Arc;

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use rbit_core::{Config, Visibility};
use rbit_services::{CreateRepository, Services, SignUp, issues::NewIssueInput};
use rbit_storage::{Database, KvStore};
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
        config.repository.max_render_bytes = 1024;
        std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
        let db = Database::open(&config.storage.sqlite_path()).await.unwrap();
        let kv = KvStore::open(&config.storage.kv_path()).unwrap();
        let config = Arc::new(config);
        let services = Services::new(config.clone(), &db, &kv).await.unwrap();
        let owner = services
            .accounts
            .create_user(
                SignUp { username: "preview-owner".into(), email: "preview@example.com".into(), password: "correct horse battery".into() },
                Some(false),
            )
            .await
            .unwrap();
        for visibility in [Visibility::Public, Visibility::Private] {
            services
                .repositories
                .create(&owner, CreateRepository { name: visibility.as_str().into(), description: String::new(), visibility })
                .await
                .unwrap();
        }
        let repo = services.repositories.open(Some(&owner), "preview-owner", "public").await.unwrap();
        services
            .issues
            .create(&owner, &repo, NewIssueInput { title: "Existing issue".into(), body: "Saved text".into(), ..NewIssueInput::default() })
            .await
            .unwrap();
        let router = rbit_web::router(rbit_web::AppState::new(config, services.clone()));
        Self { _directory: directory, services, router }
    }

    async fn post(&self, query: &str, pairs: &[(&str, &str)], origin: &str) -> (StatusCode, String) {
        let encode = |value: &str| percent_encoding::utf8_percent_encode(value, percent_encoding::NON_ALPHANUMERIC).to_string();
        let encoded = pairs.iter().map(|(key, value)| format!("{}={}", encode(key), encode(value))).collect::<Vec<_>>().join("&");
        let request = Request::builder()
            .method("POST")
            .uri(format!("/issues/markdown-preview?{query}"))
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .header(header::HOST, "localhost")
            .header(header::ORIGIN, origin)
            .body(Body::from(encoded))
            .unwrap();
        let response = self.router.clone().oneshot(request).await.unwrap();
        let status = response.status();
        if status == StatusCode::OK {
            assert_eq!(response.headers()[header::CACHE_CONTROL], "private, no-store");
        }
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (status, String::from_utf8(body.to_vec()).unwrap())
    }
}

const PUBLIC: &str = "owner=preview-owner&repo=public";

#[tokio::test]
async fn discussion_previews_link_existing_references_and_sanitize_html() {
    let fixture = Fixture::new().await;
    let source = "**Draft** #1 @preview-owner [guide](docs/guide.md) <script>alert(1)</script><img src=x onerror=alert(1)><p style=\"color:red\">Hi</p>";
    let (status, html) = fixture.post(PUBLIC, &[("body", source)], "http://localhost").await;
    assert_eq!(status, StatusCode::OK);
    for expected in
        ["<strong>Draft</strong>", "href=\"/preview-owner/public/issues/1\"", "href=\"/preview-owner\"", "/blob/main/docs/guide.md"]
    {
        assert!(html.contains(expected), "missing {expected} in {html}");
    }
    for forbidden in ["<script", "onerror", "style="] {
        assert!(!html.contains(forbidden), "{html}");
    }
    let (status, html) = fixture.post(PUBLIC, &[("body", "")], "http://localhost").await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("Nothing to preview"));
}

#[tokio::test]
async fn file_previews_use_the_document_directory_and_do_not_link_issue_references() {
    let fixture = Fixture::new().await;
    let (status, html) = fixture
        .post(
            &format!("{PUBLIC}&revision=feature%2Fdocs"),
            &[("content", "[guide](guide.md) ![logo](../logo.png) #1 @preview-owner"), ("path", "docs/README.md")],
            "http://localhost",
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("/blob/feature/docs/docs/guide.md"), "{html}");
    assert!(html.contains("/raw/feature/docs/logo.png"), "{html}");
    assert!(!html.contains("/issues/1"));
    assert!(!html.contains("href=\"/preview-owner\""));
}

#[tokio::test]
async fn preview_rejects_hidden_repositories_cross_origin_and_oversized_drafts() {
    let fixture = Fixture::new().await;
    assert_eq!(fixture.post("owner=preview-owner&repo=private", &[("body", "secret")], "http://localhost").await.0, StatusCode::NOT_FOUND);
    assert_eq!(fixture.post("owner=preview-owner&repo=missing", &[("body", "secret")], "http://localhost").await.0, StatusCode::NOT_FOUND);
    assert_eq!(fixture.post(PUBLIC, &[("body", "hello")], "https://evil.example").await.0, StatusCode::FORBIDDEN);
    assert_eq!(fixture.post(PUBLIC, &[("body", &"x".repeat(65_537))], "http://localhost").await.0, StatusCode::BAD_REQUEST);
    // Unicode counts as bytes, as it does for saved issue text.
    assert_eq!(fixture.post(PUBLIC, &[("body", &"é".repeat(32_769))], "http://localhost").await.0, StatusCode::BAD_REQUEST);
    assert_eq!(
        fixture.post(&format!("{PUBLIC}&revision=main"), &[("content", "x"), ("path", &"x".repeat(1025))], "http://localhost").await.0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        fixture.post(&format!("{PUBLIC}&revision=main"), &[("content", "x"), ("path", "../README.md")], "http://localhost").await.0,
        StatusCode::BAD_REQUEST
    );
    let content = "x".repeat(fixture.services.edits.max_text_bytes() + 1);
    assert_eq!(
        fixture.post(&format!("{PUBLIC}&revision=main"), &[("content", &content), ("path", "README.md")], "http://localhost").await.0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(fixture.post(PUBLIC, &[("body", &"x".repeat(400_000))], "http://localhost").await.0, StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn preview_uses_the_shared_per_client_request_budget() {
    let fixture = Fixture::new().await;
    for _ in 0..60 {
        fixture.services.throttle.check_unsafe_request("127.0.0.1").unwrap();
    }
    assert_eq!(fixture.post(PUBLIC, &[("body", "hello")], "http://localhost").await.0, StatusCode::TOO_MANY_REQUESTS);
}
