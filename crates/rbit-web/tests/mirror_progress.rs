//! Initial mirror progress must replace setup content and respect repository access.
use std::sync::Arc;

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
    response::Response,
};
use rbit_core::{Config, RepoId, Timestamp, Visibility};
use rbit_services::{CreateRepository, Services, SignUp};
use rbit_storage::{Database, KvStore};
use rbit_web::AppState;
use tower::ServiceExt;

struct Fixture {
    _directory: tempfile::TempDir,
    _kv: KvStore,
    database: Database,
    router: Router,
    cookie: String,
    repo: RepoId,
}

impl Fixture {
    async fn new(visibility: Visibility) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.storage.data_dir = directory.path().join("data");
        config.security.rate_limit = false;
        std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
        let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
        let kv = KvStore::open(&config.storage.kv_path()).unwrap();
        let config = Arc::new(config);
        let services = Services::new(config.clone(), &database, &kv).await.unwrap();
        let owner = services
            .accounts
            .create_user(
                SignUp { username: "alice".into(), email: "alice@example.com".into(), password: "correct horse battery".into() },
                Some(false),
            )
            .await
            .unwrap();
        let token = services.accounts.start_session(&owner).await.unwrap();
        let cookie = format!("rbit_session={}", token.encode());
        let repo = services
            .repositories
            .create(&owner, CreateRepository { name: "mirror".into(), description: String::new(), visibility })
            .await
            .unwrap()
            .repo
            .id;
        database.mirrors().create(repo, "https://example.com/upstream.git", 60).await.unwrap();
        let router = rbit_web::router(AppState::new(config, services));
        Self { _directory: directory, _kv: kv, database, router, cookie, repo }
    }

    async fn get(&self, path: &str, signed_in: bool) -> Response {
        let mut request = Request::builder().uri(path);
        if signed_in {
            request = request.header(header::COOKIE, &self.cookie);
        }
        self.router.clone().oneshot(request.body(Body::empty()).unwrap()).await.unwrap()
    }
}

async fn text(response: Response) -> String {
    String::from_utf8(to_bytes(response.into_body(), usize::MAX).await.unwrap().to_vec()).unwrap()
}

#[tokio::test]
async fn pending_mirrors_show_progress_and_completed_mirrors_open_the_repository() {
    let fixture = Fixture::new(Visibility::Public).await;
    let home = fixture.get("/alice/mirror", false).await;
    assert_eq!(home.status(), StatusCode::SEE_OTHER);
    assert_eq!(home.headers()[header::LOCATION], "/alice/mirror/mirror/progress");
    let progress = text(fixture.get("/alice/mirror/mirror/progress", false).await).await;
    assert!(progress.contains("data-mirror-progress"));
    assert!(progress.contains("Waiting for an available mirror worker"));
    let status = fixture.get("/alice/mirror/mirror/status", false).await;
    assert_eq!(status.headers()[header::CACHE_CONTROL], "no-store");
    let status: serde_json::Value = serde_json::from_str(&text(status).await).unwrap();
    assert_eq!(status["phase"], "queued");
    assert_eq!(status["complete"], false);

    fixture.database.mirrors().record_sync(fixture.repo, Timestamp::now(), None).await.unwrap();
    let progress = fixture.get("/alice/mirror/mirror/progress", false).await;
    assert_eq!(progress.status(), StatusCode::SEE_OTHER);
    assert_eq!(progress.headers()[header::LOCATION], "/alice/mirror");
    let status: serde_json::Value = serde_json::from_str(&text(fixture.get("/alice/mirror/mirror/status", false).await).await).unwrap();
    assert_eq!(status["complete"], true);
}

#[tokio::test]
async fn progress_checks_repository_access_and_keeps_errors_admin_only() {
    let fixture = Fixture::new(Visibility::Private).await;
    assert_eq!(fixture.get("/alice/mirror/mirror/status", false).await.status(), StatusCode::NOT_FOUND);
    assert_eq!(fixture.get("/alice/mirror/mirror/progress", false).await.status(), StatusCode::NOT_FOUND);
    fixture.database.mirrors().record_sync(fixture.repo, Timestamp::now(), Some("upstream diagnostic")).await.unwrap();
    let home = fixture.get("/alice/mirror", true).await;
    assert_eq!(home.status(), StatusCode::SEE_OTHER);
    assert_eq!(home.headers()[header::LOCATION], "/alice/mirror/mirror/progress");
    let status = text(fixture.get("/alice/mirror/mirror/status", true).await).await;
    assert!(status.contains("upstream diagnostic"));
    assert!(status.contains("failed"));

    let public = Fixture::new(Visibility::Public).await;
    public.database.mirrors().record_sync(public.repo, Timestamp::now(), Some("upstream diagnostic")).await.unwrap();
    let status = text(public.get("/alice/mirror/mirror/status", false).await).await;
    assert!(!status.contains("upstream diagnostic"));
    assert!(status.contains("failed"));
}
