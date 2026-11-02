//! Host administrator ingress, incremental bodies and cancellation through the real router.
use axum::{
    body::{Bytes, to_bytes},
    http::{Request, StatusCode, header},
    response::Response,
};
use http_body::{Body, Frame};
use rbit_core::{Config, TokenScope, User};
use rbit_services::{CreateAccessToken, Services};
use rbit_storage::{Database, KvStore, NewUser};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{fmt::Write as _, fs, path::PathBuf, time::Duration};
use std::{
    io,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    task::{Context, Poll},
};
use tokio::sync::{Notify, mpsc};
use tower::ServiceExt as _;

#[derive(Clone, Default)]
struct Observation {
    polls: Arc<AtomicUsize>,
    dropped: Arc<AtomicBool>,
    changed: Arc<Notify>,
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
        this.observation.changed.notify_one();
        this.receiver.poll_recv(cx)
    }
}
impl Drop for ProbeBody {
    fn drop(&mut self) {
        self.observation.dropped.store(true, Ordering::SeqCst);
        self.observation.changed.notify_one();
    }
}
fn probe_body() -> (ProbeBody, mpsc::Sender<Result<Frame<Bytes>, io::Error>>, Observation) {
    // Bounded queue forces incremental backpressure instead of hiding an eager buffer.
    let (sender, receiver) = mpsc::channel(1);
    let observation = Observation::default();
    (ProbeBody { receiver, observation: observation.clone() }, sender, observation)
}
async fn wait_for(observation: &Observation, predicate: impl Fn() -> bool) {
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            let changed = observation.changed.notified();
            if predicate() {
                break;
            }
            changed.await;
        }
    })
    .await
    .expect("observable event must occur without sleeps or retries");
}

const PREFIX: &str = "/api/v1/admin/plugin-package-operations";
const BASE: &str = "https://git.example.com";
struct Fixture {
    _directory: tempfile::TempDir,
    _kv: KvStore,
    database: Database,
    services: Services,
    router: axum::Router,
    root: PathBuf,
    admin: User,
    admin_token: String,
    read_token: String,
    user_token: String,
    other_admin_token: String,
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
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt as _;
            builder.mode(0o700);
        }
        builder.create(&root).unwrap();
        #[cfg(unix)]
        for path in [&root, root.parent().unwrap(), config.storage.data_dir.as_path()] {
            fs::File::open(path).unwrap().sync_all().unwrap();
        }
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
        let other_admin_token = token(&services, &other_admin, TokenScope::Admin).await;
        let router = rbit_web::router(rbit_web::AppState::new(config, services.clone()));
        Self {
            _directory: directory,
            _kv: kv,
            database,
            services,
            router,
            root,
            admin,
            admin_token,
            read_token,
            user_token,
            other_admin_token,
        }
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
async fn json_body(response: Response) -> Value {
    serde_json::from_slice(&to_bytes(response.into_body(), 64 * 1024).await.unwrap()).unwrap()
}
fn hash(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        write!(encoded, "{byte:02x}").unwrap();
    }
    encoded
}
fn archive(kind: &Value, version: &str) -> Vec<u8> {
    let payload = b"not executable";
    let manifest = serde_json::to_vec(&json!({
        "schema_version":1,
        "plugin":{"id":"example.package", "version":version, "host_api":"^1", "capabilities":["example.read"]},
        "kind":kind,
        "files":[{"path":"payload.bin", "size":payload.len(), "sha256":hash(payload)}],
        "budgets":{"memory_bytes":1024,"concurrent_calls":1,"call_timeout_ms":1000,"storage_bytes":0,"event_queue_capacity":0}
    }))
    .unwrap();
    let mut bytes = Vec::new();
    for (path, body) in [("package.json", manifest.as_slice()), ("payload.bin", payload.as_slice())] {
        let mut header = tar::Header::new_ustar();
        header.set_path(path).unwrap();
        header.set_size(body.len() as u64);
        header.set_mode(0o644);
        header.set_uid(0);
        header.set_gid(0);
        header.set_mtime(0);
        header.set_entry_type(tar::EntryType::Regular);
        header.set_cksum();
        bytes.extend_from_slice(header.as_bytes());
        bytes.extend_from_slice(body);
        bytes.resize(bytes.len().next_multiple_of(512), 0);
    }
    bytes.resize(bytes.len() + 1024, 0);
    bytes
}
struct RunningRequest(Option<tokio::task::JoinHandle<Response>>);
impl RunningRequest {
    fn new(router: axum::Router, request: Request<axum::body::Body>) -> Self {
        Self(Some(tokio::spawn(async move { router.oneshot(request).await.unwrap() })))
    }
    fn abort(&self) {
        self.0.as_ref().unwrap().abort();
    }
    async fn response(mut self) -> Response {
        tokio::time::timeout(Duration::from_secs(5), self.0.as_mut().unwrap()).await.unwrap().unwrap()
    }
}
impl Drop for RunningRequest {
    fn drop(&mut self) {
        if let Some(task) = &self.0 {
            task.abort();
        }
    }
}

#[tokio::test]
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
        ("compressed", header::CONTENT_ENCODING, "gzip", StatusCode::UNSUPPORTED_MEDIA_TYPE),
        ("mime", header::CONTENT_TYPE, "application/json", StatusCode::UNSUPPORTED_MEDIA_TYPE),
        ("length", header::CONTENT_LENGTH, "999999999999999", StatusCode::PAYLOAD_TOO_LARGE),
    ] {
        let (body, _sender, observation) = probe_body();
        let mut request = Fixture::upload(key, Some(&fixture.admin_token), axum::body::Body::new(body));
        request.headers_mut().insert(name, value.parse().unwrap());
        assert_eq!(fixture.request(request).await.status(), expected, "{key}");
        assert_eq!(observation.polls.load(Ordering::SeqCst), 0, "{key}");
    }
    assert_eq!(fs::read_dir(&fixture.root).unwrap().count(), 0);
    fixture.services.shutdown().await;
}

#[tokio::test]
async fn two_stalled_uploads_reject_a_third_unpolled_and_disconnect_wakes_the_owned_reader() {
    let fixture = Fixture::new().await;
    let (first_body, first_sender, first) = probe_body();
    let first_request = RunningRequest::new(
        fixture.router.clone(),
        Fixture::upload("first", Some(&fixture.admin_token), axum::body::Body::new(first_body)),
    );
    wait_for(&first, || first.polls.load(Ordering::SeqCst) > 0).await;
    let (second_body, second_sender, second) = probe_body();
    let second_request = RunningRequest::new(
        fixture.router.clone(),
        Fixture::upload("second", Some(&fixture.admin_token), axum::body::Body::new(second_body)),
    );
    wait_for(&second, || second.polls.load(Ordering::SeqCst) > 0).await;
    // No journal begins before canonical verification, despite two live owned uploads.
    assert_eq!(fixture.status("first", Some(&fixture.admin_token)).await.status(), StatusCode::NOT_FOUND);
    let (third_body, _third_sender, third) = probe_body();
    let rejected = fixture.request(Fixture::upload("third", Some(&fixture.admin_token), axum::body::Body::new(third_body))).await;
    assert_eq!(rejected.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(third.polls.load(Ordering::SeqCst), 0);
    assert!(third.dropped.load(Ordering::SeqCst));
    first_request.abort();
    // The Drop-body marker precedes receiver field destruction. Observe both
    // events within the original aggregate two-second observation budget.
    tokio::time::timeout(Duration::from_secs(2), async {
        tokio::join!(wait_for(&first, || first.dropped.load(Ordering::SeqCst)), first_sender.closed());
    })
    .await
    .expect("cancelled upload must drop its body and close the receiver");
    // first_sender remains live: cancellation, not source progress or EOF, woke the bridge.
    assert!(first_sender.is_closed());
    let bytes = archive(&json!({"type":"data"}), "1.0.0");
    second_sender.send(Ok(Frame::data(Bytes::from(bytes)))).await.unwrap();
    drop(second_sender);
    assert_eq!(second_request.response().await.status(), StatusCode::OK);
    drop(first_sender);
    tokio::time::timeout(Duration::from_secs(5), fixture.services.shutdown()).await.unwrap();
    assert_eq!(fs::read_dir(&fixture.root).unwrap().count(), 1);
}

#[tokio::test]
async fn chunked_canonical_packages_are_queryable_and_replays_cannot_change_identity_or_activate_code() {
    let fixture = Fixture::new().await;
    for (key, kind, version) in [
        ("data", json!({"type":"data"}), "1.0.0"),
        ("native", json!({"type":"native", "target":"x86_64-unknown-linux-gnu", "entrypoint":"payload.bin"}), "2.0.0"),
        ("wasm", json!({"type":"wasm", "entrypoint":"payload.bin"}), "3.0.0"),
    ] {
        let bytes = archive(&kind, version);
        let digest = hash(&bytes);
        let (body, sender, observation) = probe_body();
        let request =
            RunningRequest::new(fixture.router.clone(), Fixture::upload(key, Some(&fixture.admin_token), axum::body::Body::new(body)));
        wait_for(&observation, || observation.polls.load(Ordering::SeqCst) > 0).await;
        for chunk in bytes.chunks(127) {
            sender.send(Ok(Frame::data(Bytes::copy_from_slice(chunk)))).await.unwrap();
        }
        drop(sender);
        let response = request.response().await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::LOCATION], format!("{BASE}{PREFIX}/{key}"));
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        assert_eq!(json_body(response).await["package"]["archive_sha256"], digest);
        assert_eq!(fixture.status(key, None).await.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(fixture.status(key, Some(&fixture.user_token)).await.status(), StatusCode::FORBIDDEN);
        let status = json_body(fixture.status(key, Some(&fixture.admin_token)).await).await;
        assert_eq!(status["stage"], "complete");
        let operation = fixture
            .database
            .plugin_installations()
            .operation(&rbit_storage::sqlite::plugin_installations::OperationId::new(key).unwrap())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(operation.actor, Some(fixture.admin.id()));
        assert_eq!(status["package"]["archive_sha256"], digest);
        assert_eq!(fs::read(fixture.root.join(format!("{digest}.rbit-plugin"))).unwrap(), bytes);
        let replay = fixture.request(Fixture::upload(key, Some(&fixture.admin_token), axum::body::Body::from(bytes.clone()))).await;
        assert_eq!(replay.status(), StatusCode::OK);
        let foreign_replay = fixture.request(Fixture::upload(key, Some(&fixture.other_admin_token), axum::body::Body::from(bytes))).await;
        assert_eq!(foreign_replay.status(), StatusCode::CONFLICT);
        let changed = archive(&json!({"type":"data"}), version);
        if key != "data" {
            assert_eq!(
                fixture.request(Fixture::upload(key, Some(&fixture.admin_token), axum::body::Body::from(changed))).await.status(),
                StatusCode::CONFLICT
            );
        }
    }
    assert_eq!(fs::read_dir(&fixture.root).unwrap().count(), 3);
    assert!(!fixture.root.join("payload.bin").exists());
    assert_eq!(fixture.database.plugin_settings().desired_enabled("example.package").await.unwrap(), None);
    assert!(fixture.services.actions.is_none());
    assert!(!fixture.services.i18n_enabled());
    fixture.services.shutdown().await;
}

#[tokio::test]
async fn discarded_committed_upload_response_is_recoverable_by_the_known_operation_key() {
    let fixture = Fixture::new().await;
    let bytes = archive(&json!({"type":"data"}), "1.0.0");
    let digest = hash(&bytes);
    // Deliberately lose the response without interpreting its status/body. This is a
    // postcommit response-loss test; it does not simulate uncertain SQLite COMMIT.
    drop(fixture.request(Fixture::upload("lost-response", Some(&fixture.admin_token), axum::body::Body::from(bytes.clone()))).await);
    let status = fixture.status("lost-response", Some(&fixture.admin_token)).await;
    assert_eq!(status.status(), StatusCode::OK);
    let status = json_body(status).await;
    assert_eq!(status["stage"], "complete");
    assert_eq!(status["package"]["archive_sha256"], digest);
    assert_eq!(
        fixture.request(Fixture::upload("lost-response", Some(&fixture.admin_token), axum::body::Body::from(bytes))).await.status(),
        StatusCode::OK
    );
    assert_eq!(fs::read_dir(&fixture.root).unwrap().count(), 1);
    fixture.services.shutdown().await;
}

#[tokio::test]
async fn chunked_trailing_bytes_fail_exact_eof_without_recording_or_publishing() {
    let fixture = Fixture::new().await;
    let bytes = archive(&json!({"type":"data"}), "1.0.0");
    let (body, sender, observation) = probe_body();
    let request =
        RunningRequest::new(fixture.router.clone(), Fixture::upload("trailing", Some(&fixture.admin_token), axum::body::Body::new(body)));
    wait_for(&observation, || observation.polls.load(Ordering::SeqCst) > 0).await;
    sender.send(Ok(Frame::data(Bytes::from(bytes)))).await.unwrap();
    sender.send(Ok(Frame::data(Bytes::from_static(b"unexpected trailing private bytes")))).await.unwrap();
    drop(sender);
    let response = request.response().await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let failure = json_body(response).await;
    assert_eq!(failure["message"], "Invalid canonical package archive");
    assert_eq!(fixture.status("trailing", Some(&fixture.admin_token)).await.status(), StatusCode::NOT_FOUND);
    assert_eq!(fs::read_dir(&fixture.root).unwrap().count(), 0);
    fixture.services.shutdown().await;
}

#[tokio::test]
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
    fixture.database.admin().set_suspended(fixture.admin.id(), true).await.unwrap();
    let (body, _sender, observation) = probe_body();
    assert_eq!(
        fixture.request(Fixture::upload("suspended", Some(&fixture.admin_token), axum::body::Body::new(body))).await.status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(observation.polls.load(Ordering::SeqCst), 0);
    assert_eq!(fs::read_dir(&fixture.root).unwrap().count(), 0);
    fixture.services.shutdown().await;
    let fixture = Fixture::with_api(false).await;
    let (body, _sender, observation) = probe_body();
    assert_eq!(
        fixture.request(Fixture::upload("disabled-api", Some(&fixture.admin_token), axum::body::Body::new(body))).await.status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(observation.polls.load(Ordering::SeqCst), 0);
    assert_eq!(fs::read_dir(&fixture.root).unwrap().count(), 0);
    fixture.services.shutdown().await;
}

// Wrap the actual Hyper request body, not an in-process fabricated stream. A
// positive observation means at least one data frame was consumed and the next
// poll of the real network source returned Pending.
struct NetworkBody {
    inner: axum::body::Body,
    observation: Observation,
    saw_data: bool,
}
impl Body for NetworkBody {
    type Data = Bytes;
    type Error = axum::Error;
    fn poll_frame(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Result<Frame<Bytes>, axum::Error>>> {
        let this = self.get_mut();
        let result = Pin::new(&mut this.inner).poll_frame(cx);
        match &result {
            Poll::Ready(Some(Ok(frame))) if frame.data_ref().is_some_and(|bytes| !bytes.is_empty()) => this.saw_data = true,
            Poll::Pending if this.saw_data => {
                this.observation.polls.fetch_add(1, Ordering::SeqCst);
                this.observation.changed.notify_one();
            }
            _ => {}
        }
        result
    }
}
impl Drop for NetworkBody {
    fn drop(&mut self) {
        self.observation.dropped.store(true, Ordering::SeqCst);
        self.observation.changed.notify_one();
    }
}
struct NetworkServer {
    cancel: tokio_util::sync::CancellationToken,
    task: tokio::task::JoinHandle<io::Result<()>>,
}
impl NetworkServer {
    async fn shutdown(mut self) {
        self.cancel.cancel();
        tokio::time::timeout(Duration::from_secs(5), &mut self.task).await.unwrap().unwrap().unwrap();
    }
}
impl Drop for NetworkServer {
    fn drop(&mut self) {
        self.cancel.cancel();
        self.task.abort();
    }
}
async fn stalled_connection(address: std::net::SocketAddr, id: &str, token: &str) -> tokio::net::TcpStream {
    use tokio::io::AsyncWriteExt as _;
    let mut socket = tokio::net::TcpStream::connect(address).await.unwrap();
    let request = format!(
        "PUT {PREFIX}/{id} HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/octet-stream\r\nTransfer-Encoding: chunked\r\n\r\n1\r\nx\r\n"
    );
    socket.write_all(request.as_bytes()).await.unwrap();
    socket.flush().await.unwrap();
    socket
}

#[tokio::test]
async fn actual_peer_disconnect_wakes_hyper_body_and_both_owned_slots_become_reusable() {
    let fixture = Fixture::new().await;
    let first = Observation::default();
    let second = Observation::default();
    let observations = Arc::new(vec![("network-first", first.clone()), ("network-second", second.clone())]);
    let router = fixture.router.clone().layer(axum::middleware::map_request(move |mut request: Request<axum::body::Body>| {
        let observation =
            observations.iter().find(|(id, _)| request.uri().path() == format!("{PREFIX}/{id}")).map(|(_, value)| value.clone());
        async move {
            if let Some(observation) = observation {
                let body = std::mem::replace(request.body_mut(), axum::body::Body::empty());
                *request.body_mut() = axum::body::Body::new(NetworkBody { inner: body, observation, saw_data: false });
            }
            request
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let cancel = tokio_util::sync::CancellationToken::new();
    let shutdown = cancel.clone();
    let server = NetworkServer {
        cancel,
        task: tokio::spawn(async move { axum::serve(listener, router).with_graceful_shutdown(shutdown.cancelled_owned()).await }),
    };
    let first_socket = stalled_connection(address, "network-first", &fixture.admin_token).await;
    wait_for(&first, || first.polls.load(Ordering::SeqCst) > 0).await;
    let second_socket = stalled_connection(address, "network-second", &fixture.admin_token).await;
    wait_for(&second, || second.polls.load(Ordering::SeqCst) > 0).await;
    let (body, _sender, third) = probe_body();
    assert_eq!(
        fixture.request(Fixture::upload("network-third", Some(&fixture.admin_token), axum::body::Body::new(body))).await.status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(third.polls.load(Ordering::SeqCst), 0);
    // Actual TCP peers close while the server is demonstrably pending on their
    // incomplete chunked bodies. No in-process request future is aborted here.
    drop(first_socket);
    drop(second_socket);
    wait_for(&first, || first.dropped.load(Ordering::SeqCst)).await;
    wait_for(&second, || second.dropped.load(Ordering::SeqCst)).await;
    // Ownership includes cleanup after the body is dropped. Wait only on the
    // documented pre-admission 503, proving no busy attempt polls or publishes.
    let (reuse_request, reuse_sender, reuse) = acquire_stalled(&fixture, "network-reuse-one").await;
    let (other_request, other_sender, other) = acquire_stalled(&fixture, "network-reuse-two").await;
    reuse_request.abort();
    other_request.abort();
    wait_for(&reuse, || reuse.dropped.load(Ordering::SeqCst)).await;
    wait_for(&other, || other.dropped.load(Ordering::SeqCst)).await;
    drop(reuse_sender);
    drop(other_sender);
    tokio::time::timeout(Duration::from_secs(5), fixture.services.shutdown()).await.unwrap();
    server.shutdown().await;
    assert_eq!(fs::read_dir(&fixture.root).unwrap().count(), 0);
}

// Slot release follows real staging cleanup, not merely the network body Drop.
// Only documented pre-admission 503s may be observed while waiting for settlement;
// every such attempt must remain unpolled and side-effect-free. This is an eventual
// admission assertion, not a retry of a failed installation or a test rerun.
async fn acquire_stalled(fixture: &Fixture, id: &str) -> (RunningRequest, mpsc::Sender<Result<Frame<Bytes>, io::Error>>, Observation) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let (body, sender, observation) = probe_body();
            let mut request =
                RunningRequest::new(fixture.router.clone(), Fixture::upload(id, Some(&fixture.admin_token), axum::body::Body::new(body)));
            let pending = async {
                loop {
                    let changed = observation.changed.notified();
                    if observation.polls.load(Ordering::SeqCst) > 0 {
                        break;
                    }
                    changed.await;
                }
            };
            tokio::select! {
                () = pending => return (request, sender, observation),
                response = request.0.as_mut().unwrap() => {
                    let response = response.unwrap();
                    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
                    assert_eq!(observation.polls.load(Ordering::SeqCst), 0);
                    assert!(observation.dropped.load(Ordering::SeqCst));
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
            }
        }
    })
    .await
    .expect("both owned admission slots must become reusable after disconnected workers settle")
}
