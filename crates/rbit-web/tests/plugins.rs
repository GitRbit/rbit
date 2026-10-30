//! Plugin controls remain on one router across runtime transitions.

use std::sync::Arc;

use axum::{
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode, header},
};
use rbit_core::{Config, Visibility};
use rbit_services::{CreateRepository, Services, SignUp};
use rbit_storage::{Database, KvStore};
use tower::ServiceExt as _;

struct Fixture {
    _directory: tempfile::TempDir,
    services: Services,
    #[cfg(feature = "actions")]
    database: Database,
    router: axum::Router,
    #[cfg(feature = "actions")]
    admin: rbit_core::User,
    admin_cookie: String,
    user_cookie: String,
}

impl Fixture {
    async fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.storage.data_dir = directory.path().join("data");
        config.security.rate_limit = false;
        config.updates.check_for_updates = false;
        std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
        let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
        let kv = KvStore::open(&config.storage.kv_path()).unwrap();
        let config = Arc::new(config);
        let services = Services::new(config.clone(), &database, &kv).await.unwrap();
        let admin = services
            .accounts
            .create_user(
                SignUp { username: "alice".into(), email: "alice@example.com".into(), password: "correct horse battery".into() },
                Some(true),
            )
            .await
            .unwrap();
        let user = services
            .accounts
            .create_user(
                SignUp { username: "bob".into(), email: "bob@example.com".into(), password: "correct horse battery".into() },
                Some(false),
            )
            .await
            .unwrap();
        services
            .repositories
            .create(&admin, CreateRepository { name: "rocket".into(), description: String::new(), visibility: Visibility::Public })
            .await
            .unwrap();
        let cookie = |token: rbit_auth::SessionToken| format!("rbit_session={}", token.encode());
        let admin_cookie = cookie(services.accounts.start_session(&admin).await.unwrap());
        let user_cookie = cookie(services.accounts.start_session(&user).await.unwrap());
        let router = rbit_web::router(rbit_web::AppState::new(config, services.clone()));
        Self {
            _directory: directory,
            services,
            #[cfg(feature = "actions")]
            database,
            router,
            #[cfg(feature = "actions")]
            admin,
            admin_cookie,
            user_cookie,
        }
    }

    async fn request(
        &self,
        method: Method,
        path: &str,
        cookie: Option<&str>,
        origin: &str,
        body: &str,
    ) -> (StatusCode, String, axum::http::HeaderMap) {
        let mut request = Request::builder()
            .method(method)
            .uri(path)
            .header(header::HOST, "localhost:3000")
            .header(header::ORIGIN, origin)
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
        if let Some(cookie) = cookie {
            request = request.header(header::COOKIE, cookie);
        }
        let response = self.router.clone().oneshot(request.body(Body::from(body.to_owned())).unwrap()).await.unwrap();
        let (parts, body) = response.into_parts();
        let bytes = to_bytes(body, usize::MAX).await.unwrap();
        (parts.status, String::from_utf8(bytes.to_vec()).unwrap(), parts.headers)
    }

    async fn get(&self, path: &str) -> (StatusCode, String, axum::http::HeaderMap) {
        self.request(Method::GET, path, Some(&self.admin_cookie), "http://localhost:3000", "").await
    }
}

#[tokio::test]
async fn plugin_administration_is_host_owned_and_requires_an_administrator() {
    let fixture = Fixture::new().await;
    let (status, body, _) = fixture.get("/admin/plugins").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("Plugins"));
    assert_eq!(
        fixture.request(Method::GET, "/admin/plugins", Some(&fixture.user_cookie), "http://localhost:3000", "").await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        fixture
            .request(Method::POST, "/admin/plugins/actions", Some(&fixture.user_cookie), "http://localhost:3000", "enabled=true")
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        fixture
            .request(Method::POST, "/admin/plugins/actions", Some(&fixture.admin_cookie), "https://other.example", "enabled=true")
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert!(fixture.services.actions.is_none());
    if !cfg!(feature = "actions") {
        assert!(body.contains("This build does not include Actions"));
    }
}

#[tokio::test]
async fn runtime_refusals_report_the_revision_needed_to_refresh_an_open_page() {
    let fixture = Fixture::new().await;
    let revision = fixture.services.plugin_revision().to_string();
    let (status, _, headers) = fixture.get("/alice/rocket/actions").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(headers["rbit-plugin-revision"], revision);
    for authorization in [format!("Bearer rbat_{}", "a".repeat(43)), format!("Bearer {}", "a".repeat(4097))] {
        let request =
            Request::builder().uri("/api/v1/repos/alice/rocket").header(header::AUTHORIZATION, authorization).body(Body::empty()).unwrap();
        let response = fixture.router.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(response.headers()["rbit-plugin-revision"], revision);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    }
}

#[cfg(feature = "actions")]
#[tokio::test]
async fn one_router_reflects_enable_and_disable_in_pages_navigation_and_openapi() {
    let fixture = Fixture::new().await;
    let repo = fixture.services.repositories.open(Some(&fixture.admin), "alice", "rocket").await.unwrap();
    fixture.database.actions_settings().set_enabled(repo.repo.repo.id, true, rbit_core::Timestamp::now()).await.unwrap();
    let revision = fixture.services.plugin_revision();
    for enabled in [false, true, false] {
        if enabled || fixture.services.actions.is_some() {
            let (status, _, headers) = fixture
                .request(
                    Method::POST,
                    "/admin/plugins/actions",
                    Some(&fixture.admin_cookie),
                    "http://localhost:3000",
                    if enabled { "enabled=true" } else { "enabled=false" },
                )
                .await;
            assert_eq!(status, StatusCode::SEE_OTHER);
            assert_eq!(headers["hx-redirect"], "/admin/plugins");
        }
        let (status, _, _) = fixture.get("/alice/rocket/actions").await;
        assert_eq!(status, if enabled { StatusCode::OK } else { StatusCode::NOT_FOUND });
        let (_, body, _) = fixture.get("/alice/rocket").await;
        assert_eq!(body.contains("data-repo-tab=\"actions\""), enabled);
        let (_, body, _) = fixture.get("/api/v1/openapi.json").await;
        let document: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(document["paths"].as_object().unwrap().keys().any(|path| path.contains("/actions/")), enabled);
        assert_eq!(fixture.services.actions.is_some(), enabled);
    }
    assert!(fixture.services.plugin_revision() > revision);
    assert_eq!(fixture.services.actions_background_tasks(), 0);
    // Stale clients cannot dispatch after the public contribution disappears.
    assert_eq!(
        fixture
            .request(
                Method::POST,
                "/alice/rocket/actions/workflows/build/dispatch",
                Some(&fixture.admin_cookie),
                "http://localhost:3000",
                ""
            )
            .await
            .0,
        StatusCode::NOT_FOUND
    );
}
