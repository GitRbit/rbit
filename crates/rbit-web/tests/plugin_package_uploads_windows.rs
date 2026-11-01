//! Windows ingress refuses package bytes until supported host root/ACL verification exists.
use axum::{
    body::Bytes,
    http::{Request, StatusCode, header},
    response::Response,
};
use http_body::{Body, Frame};
use rbit_core::{Config, TokenScope, User};
use rbit_services::{CreateAccessToken, Services};
use rbit_storage::{Database, KvStore, NewUser};
use std::{
    fs, io,
    path::PathBuf,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    task::{Context, Poll},
    time::Duration,
};
use tokio::sync::mpsc;
use tower::ServiceExt as _;
#[derive(Clone, Default)]
struct Observation {
    polls: Arc<AtomicUsize>,
    dropped: Arc<AtomicBool>,
}
struct ProbeBody {
    receiver: mpsc::Receiver<Result<Frame<Bytes>, io::Error>>,
    observation: Observation,
}
impl Body for ProbeBody {
    type Data = Bytes;
    type Error = io::Error;
    fn poll_frame(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Result<Frame<Bytes>, io::Error>>> {
        let this = self.get_mut();
        this.observation.polls.fetch_add(1, Ordering::SeqCst);
        this.receiver.poll_recv(cx)
    }
}
impl Drop for ProbeBody {
    fn drop(&mut self) {
        self.observation.dropped.store(true, Ordering::SeqCst);
    }
}
fn probe_body() -> (ProbeBody, mpsc::Sender<Result<Frame<Bytes>, io::Error>>, Observation) {
    // Bounded queue forces incremental backpressure instead of hiding an eager buffer.
    let (sender, receiver) = mpsc::channel(1);
    let observation = Observation::default();
    (ProbeBody { receiver, observation: observation.clone() }, sender, observation)
}
const PREFIX: &str = "/api/v1/admin/plugin-package-operations";
const BASE: &str = "https://git.example.com";
struct Fixture {
    database: Database,
    services: Services,
    router: axum::Router,
    root: PathBuf,
    admin: User,
    admin_token: String,
    read_token: String,
    user_token: String,
    // Persistent handles and service/router clones drop before directory cleanup.
    _kv: KvStore,
    directory: tempfile::TempDir,
}
impl Fixture {
    async fn new() -> Self {
        Self::with_api(true).await
    }
    async fn with_api(api_enabled: bool) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.storage.data_dir = directory.path().join("data");
        BASE.clone_into(&mut config.server.public_url);
        config.api.enabled = api_enabled;
        config.security.rate_limit = false;
        config.updates.check_for_updates = false;
        config.actions.enabled = false;
        config.i18n.enabled = false;
        fs::create_dir_all(config.storage.repositories_dir()).unwrap();
        let root = config.storage.data_dir.join("plugins").join("packages");
        let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
        let kv = database.open_kv(&config.storage.kv_path()).unwrap();
        let mut users = Vec::new();
        for (name, is_admin) in [("alice", true), ("bob", false)] {
            users.push(
                database
                    .accounts()
                    .create_user(NewUser {
                        name: name.parse().unwrap(),
                        email: format!("{name}@example.com").parse().unwrap(),
                        display_name: None,
                        password_hash: "unused".to_owned(),
                        is_admin,
                    })
                    .await
                    .unwrap(),
            );
        }
        let user = users.pop().unwrap();
        let admin = users.pop().unwrap();
        let config = Arc::new(config);
        let services = Services::new(Arc::clone(&config), &database, &kv).await.unwrap();
        let admin_token = token(&services, &admin, TokenScope::Admin).await;
        let read_token = token(&services, &admin, TokenScope::Read).await;
        let user_token = token(&services, &user, TokenScope::Admin).await;
        let other_admin = database
            .accounts()
            .create_user(NewUser {
                name: "carol".parse().unwrap(),
                email: "carol@example.com".parse().unwrap(),
                display_name: None,
                password_hash: "unused".to_owned(),
                is_admin: true,
            })
            .await
            .unwrap();
        assert!(other_admin.is_admin);
        let router = rbit_web::router(rbit_web::AppState::new(config, services.clone()));
        Self { directory, _kv: kv, database, services, router, root, admin, admin_token, read_token, user_token }
    }
    async fn shutdown(&self) {
        self.services.shutdown().await;
        self.database.close().await;
    }
    fn upload(id: &str, token: Option<&str>, body: axum::body::Body) -> Request<axum::body::Body> {
        let mut request =
            Request::builder().method("PUT").uri(format!("{PREFIX}/{id}")).header(header::CONTENT_TYPE, "application/octet-stream");
        if let Some(token) = token {
            request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
        }
        request.body(body).unwrap()
    }
    async fn request(&self, request: Request<axum::body::Body>) -> Response {
        tokio::time::timeout(Duration::from_secs(5), self.router.clone().oneshot(request)).await.unwrap().unwrap()
    }
    async fn status(&self, id: &str, token: Option<&str>) -> Response {
        let mut request = Request::builder().uri(format!("{PREFIX}/{id}"));
        if let Some(token) = token {
            request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
        }
        self.request(request.body(axum::body::Body::empty()).unwrap()).await
    }
}
async fn token(services: &Services, user: &User, scope: TokenScope) -> String {
    services.tokens.create(user, CreateAccessToken { name: format!("{scope:?}"), scope, expires_in_days: None }).await.unwrap().1
}
#[tokio::test]
#[cfg_attr(not(windows), ignore = "requires native Windows package upload policy")]
async fn package_upload_authorization_and_header_rejections_do_not_poll_any_body_frame() {
    let fixture = Fixture::new().await;
    let write_token = token(&fixture.services, &fixture.admin, TokenScope::Write).await;
    for (key, credential, expected) in [
        ("anonymous", None, StatusCode::UNAUTHORIZED),
        ("read-scope", Some(fixture.read_token.as_str()), StatusCode::FORBIDDEN),
        ("write-scope", Some(write_token.as_str()), StatusCode::FORBIDDEN),
        ("non-admin", Some(fixture.user_token.as_str()), StatusCode::FORBIDDEN),
        ("invalid-token", Some("invalid"), StatusCode::UNAUTHORIZED),
    ] {
        let (body, _sender, observation) = probe_body();
        let response = fixture.request(Fixture::upload(key, credential, axum::body::Body::new(body))).await;
        assert_eq!(response.status(), expected, "{key}");
        assert_eq!(observation.polls.load(Ordering::SeqCst), 0, "{key}");
        assert!(observation.dropped.load(Ordering::SeqCst), "{key}");
    }
    let session = fixture.services.accounts.start_session(&fixture.admin).await.unwrap();
    let (body, _sender, observation) = probe_body();
    let mut request = Fixture::upload("cookie-only", None, axum::body::Body::new(body));
    request.headers_mut().insert(header::COOKIE, format!("rbit_session={}", session.encode()).parse().unwrap());
    assert_eq!(fixture.request(request).await.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(observation.polls.load(Ordering::SeqCst), 0);
    for (key, name, value, expected) in [
        ("compressed", header::CONTENT_ENCODING, "gzip", StatusCode::SERVICE_UNAVAILABLE),
        ("mime", header::CONTENT_TYPE, "application/json", StatusCode::SERVICE_UNAVAILABLE),
        ("length", header::CONTENT_LENGTH, "999999999999999", StatusCode::SERVICE_UNAVAILABLE),
    ] {
        let (body, _sender, observation) = probe_body();
        let mut request = Fixture::upload(key, Some(&fixture.admin_token), axum::body::Body::new(body));
        request.headers_mut().insert(name, value.parse().unwrap());
        assert_eq!(fixture.request(request).await.status(), expected, "{key}");
        assert_eq!(observation.polls.load(Ordering::SeqCst), 0, "{key}");
    }
    assert!(!fixture.root.exists());
    fixture.shutdown().await;
}

#[tokio::test]
#[cfg_attr(not(windows), ignore = "requires native Windows package upload policy")]
async fn expired_revoked_suspended_and_disabled_api_uploads_are_rejected_before_body_polling() {
    let fixture = Fixture::new().await;
    let expired = rbit_auth::AccessTokenSecret::generate();
    fixture
        .database
        .access_tokens()
        .create(rbit_storage::NewAccessToken {
            account_id: fixture.admin.id(),
            name: "expired".into(),
            digest: expired.digest(),
            hint: expired.hint(),
            scope: TokenScope::Admin,
            expires_at: Some(rbit_core::Timestamp(1)),
        })
        .await
        .unwrap();
    let (revoked, secret) = fixture
        .services
        .tokens
        .create(&fixture.admin, CreateAccessToken { name: "revoked".into(), scope: TokenScope::Admin, expires_in_days: None })
        .await
        .unwrap();
    fixture.services.tokens.revoke(&fixture.admin, revoked.id).await.unwrap();
    for (key, token) in [("expired", expired.encode()), ("revoked", secret)] {
        let (body, _sender, observation) = probe_body();
        assert_eq!(
            fixture.request(Fixture::upload(key, Some(&token), axum::body::Body::new(body))).await.status(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(observation.polls.load(Ordering::SeqCst), 0);
    }
    assert_eq!(
        fixture.database.admin().set_suspended(fixture.admin.id(), true).await.unwrap(),
        rbit_storage::sqlite::admin::Moderation::Applied
    );
    let (body, _sender, observation) = probe_body();
    assert_eq!(
        fixture.request(Fixture::upload("suspended", Some(&fixture.admin_token), axum::body::Body::new(body))).await.status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(observation.polls.load(Ordering::SeqCst), 0);
    assert!(!fixture.root.exists());
    fixture.shutdown().await;
    let fixture = Fixture::with_api(false).await;
    let (body, _sender, observation) = probe_body();
    assert_eq!(
        fixture.request(Fixture::upload("disabled-api", Some(&fixture.admin_token), axum::body::Body::new(body))).await.status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(observation.polls.load(Ordering::SeqCst), 0);
    assert!(!fixture.root.exists());
    fixture.shutdown().await;
}

#[tokio::test]
#[cfg_attr(not(windows), ignore = "requires native Windows package upload policy")]
async fn windows_admin_put_is_unavailable_without_polling_and_operation_queries_remain_available() {
    let fixture = Fixture::new().await;
    let (body, _sender, observation) = probe_body();
    let response = fixture.request(Fixture::upload("valid-admin", Some(&fixture.admin_token), axum::body::Body::new(body))).await;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    assert_eq!(observation.polls.load(Ordering::SeqCst), 0);
    assert!(observation.dropped.load(Ordering::SeqCst));
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await.unwrap();
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["message"], "Windows package upload requires supported host root verification");
    assert_eq!(fixture.status("valid-admin", None).await.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(fixture.status("valid-admin", Some(&fixture.user_token)).await.status(), StatusCode::FORBIDDEN);
    let response = fixture.status("valid-admin", Some(&fixture.admin_token)).await;
    // Unknown journal state remains a real query, not a blanket Windows PUT refusal.
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    assert!(!fixture.root.exists());
    let temporary_directory = fixture.directory.path().to_owned();
    fixture.shutdown().await;
    drop(fixture);
    assert!(!temporary_directory.exists(), "closed storage must not leak a Windows fixture directory");
}
