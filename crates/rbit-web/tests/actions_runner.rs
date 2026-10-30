//! The runner protocol over HTTP (`docs/design/actions.md` §12.6): registration with
//! single-use tokens, declarations, unregistering, and every refusal — a missing or wrong
//! protocol version, a wrong credential, oversized and malformed bodies, throttling.
//!
//! The server side of the runner protocol (stream S) owns this file.

use std::sync::Arc;

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use rbit_core::{Config, User, Visibility, actions::RunnerLabel};
use rbit_services::{
    ActionsService, CreateRepository, RepoContext, Services, SignUp,
    actions::runners::{ManagedScope, RegistrationOptions},
};
use rbit_storage::{Database, KvStore};
use rbit_web::AppState;
use serde_json::{Value, json};
use tower::ServiceExt;

pub struct Fixture {
    _directory: tempfile::TempDir,
    pub services: Services,
    pub database: Database,
    pub router: Router,
    pub alice: User,
}

/// A response's status, headers and JSON body (`Value::Null` when empty or not JSON).
pub struct Answer {
    pub status: StatusCode,
    pub headers: axum::http::HeaderMap,
    pub json: Value,
}

impl Fixture {
    pub async fn new() -> Self {
        Self::with(|_| {}).await
    }

    pub async fn with(configure: impl FnOnce(&mut Config)) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.storage.data_dir = directory.path().join("data");
        config.security.rate_limit = false;
        config.actions.enabled = true;
        "https://git.example.com".clone_into(&mut config.server.public_url);
        configure(&mut config);
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
        let input = CreateRepository { name: "rocket".into(), description: String::new(), visibility: Visibility::Private };
        let repo = services.repositories.create(&alice, input).await.unwrap();
        database.actions_settings().set_enabled(repo.repo.id, true, rbit_core::Timestamp::now()).await.unwrap();
        let router = rbit_web::router(AppState::new(config, services.clone()));
        Self { _directory: directory, services, database, router, alice }
    }

    pub fn actions(&self) -> ActionsService {
        self.services.actions.snapshot().unwrap()
    }

    pub async fn repo(&self) -> RepoContext {
        self.services.repositories.open(Some(&self.alice), "alice", "rocket").await.unwrap()
    }

    /// A registration token of `alice/rocket`.
    pub async fn registration(&self, options: RegistrationOptions) -> String {
        let repo = self.repo().await;
        let issued = self.actions().create_registration_token(&self.alice, ManagedScope::Repository(&repo), options).await.unwrap();
        issued.secret.expose().to_owned()
    }

    /// Sends `body` to `path` below the protocol's base with the current protocol version.
    pub async fn call(&self, path: &str, credential: Option<&str>, body: impl Into<Body>) -> Answer {
        self.send(path, &[("rbit-runner-protocol", "1")], credential, body.into()).await
    }

    pub async fn send(&self, path: &str, headers: &[(&str, &str)], credential: Option<&str>, body: Body) -> Answer {
        let mut request = Request::builder()
            .method("POST")
            .uri(format!("/api/actions/runner/v1/{path}"))
            .header(header::HOST, "git.example.com")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::USER_AGENT, "rbit-runner/0.2.0");
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        if let Some(credential) = credential {
            request = request.header(header::AUTHORIZATION, format!("Bearer {credential}"));
        }
        let response = self.router.clone().oneshot(request.body(body).unwrap()).await.unwrap();
        let (status, headers) = (response.status(), response.headers().clone());
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        Answer { status, headers, json: serde_json::from_slice(&bytes).unwrap_or(Value::Null) }
    }

    /// Registers a runner of `alice/rocket` with `labels` (container backend) and returns
    /// its credential.
    pub async fn runner(&self, labels: &[&str]) -> String {
        let token = self.registration(RegistrationOptions::default()).await;
        let answer = self.call("register", None, register_body(&token, "build-1", labels).to_string()).await;
        assert_eq!(answer.status, StatusCode::OK, "{}", answer.json);
        answer.json["credential"].as_str().unwrap().to_owned()
    }
}

pub fn capabilities() -> Value {
    json!({"backends": ["container", "host"], "os": "linux", "arch": "x86_64", "capacity": 2, "version": "0.2.0", "ephemeral": false})
}

pub fn labels_json(labels: &[&str]) -> Value {
    labels.iter().map(|label| json!({"name": label, "backend": "container"})).collect()
}

pub fn register_body(token: &str, name: &str, labels: &[&str]) -> Value {
    json!({"token": token, "name": name, "labels": labels_json(labels), "capabilities": capabilities()})
}

pub fn declare_body(labels: &[&str]) -> Value {
    json!({"name": "build-1", "labels": labels_json(labels), "capabilities": capabilities()})
}

#[tokio::test]
async fn runners_register_once_per_token_and_get_a_credential() {
    let fixture = Fixture::new().await;
    let token = fixture.registration(RegistrationOptions::default()).await;
    let answer = fixture.call("register", None, register_body(&token, "build-1", &["linux"]).to_string()).await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.json);
    let credential = answer.json["credential"].as_str().unwrap();
    assert!(rbit_workflow::protocol::valid_runner_credential(credential), "{credential}");
    assert_eq!(answer.json["intervals"], json!({"long_poll_seconds": 25, "heartbeat_seconds": 10}));
    let id = answer.json["runner_id"].as_i64().unwrap();
    let stored = fixture.database.actions_runners().find(rbit_core::actions::RunnerId(id)).await.unwrap().unwrap();
    assert_eq!((stored.name.as_str(), stored.capabilities.os.as_str(), stored.protocol), ("build-1", "linux", 1));
    assert_eq!(stored.labels, vec!["linux".parse::<RunnerLabel>().unwrap()]);

    // Single use.
    let again = fixture.call("register", None, register_body(&token, "build-2", &["linux"]).to_string()).await;
    assert_eq!(again.status, StatusCode::UNAUTHORIZED, "{}", again.json);
    assert_eq!(again.json["code"], "unauthenticated");
    let unknown = fixture.call("register", None, register_body("rbrg_nope", "build-2", &["linux"]).to_string()).await;
    assert_eq!(unknown.status, StatusCode::UNAUTHORIZED);

    let audit = fixture.services.audit.recent_for(&fixture.alice, 10).await.unwrap();
    assert!(audit.iter().any(|entry| entry.action == rbit_services::audit::AuditAction::ActionsRegistrationTokenCreated));
}

#[tokio::test]
async fn pinned_labels_bind_registration_and_declarations() {
    let fixture = Fixture::new().await;
    let pinned = RegistrationOptions { labels: Some(vec!["linux".parse().unwrap()]), uses: None, ..RegistrationOptions::default() };
    let token = fixture.registration(pinned).await;
    let refused = fixture.call("register", None, register_body(&token, "w", &["windows"]).to_string()).await;
    assert_eq!((refused.status, refused.json["code"].as_str()), (StatusCode::FORBIDDEN, Some("forbidden")));
    let answer = fixture.call("register", None, register_body(&token, "l", &["linux"]).to_string()).await;
    assert_eq!(answer.status, StatusCode::OK);
    let credential = answer.json["credential"].as_str().unwrap();

    let refused = fixture.call("declare", Some(credential), declare_body(&["linux", "gpu"]).to_string()).await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN, "{}", refused.json);
    assert!(refused.json["message"].as_str().unwrap().contains("gpu"));
    let declared = fixture.call("declare", Some(credential), declare_body(&["linux"]).to_string()).await;
    assert_eq!(declared.status, StatusCode::OK, "{}", declared.json);
}

#[tokio::test]
async fn declarations_record_capabilities_and_label_changes_wake_the_scheduler() {
    let fixture = Fixture::new().await;
    let credential = fixture.runner(&["linux"]).await;
    let version = fixture.actions().tasks_version();
    let same = fixture.call("declare", Some(&credential), declare_body(&["linux"]).to_string()).await;
    assert_eq!(same.status, StatusCode::OK, "{}", same.json);
    assert_eq!(fixture.actions().tasks_version(), version, "nothing new to offer");

    let mut body = declare_body(&["linux", "arm"]);
    body["labels"][1]["backend"] = json!("host");
    body["capabilities"]["ephemeral"] = json!(true);
    let changed = fixture.call("declare", Some(&credential), body.to_string()).await;
    assert_eq!(changed.status, StatusCode::OK, "{}", changed.json);
    assert!(fixture.actions().tasks_version() > version, "new labels may match waiting jobs");
    let id = rbit_core::actions::RunnerId(changed.json["runner_id"].as_i64().unwrap());
    let stored = fixture.database.actions_runners().find(id).await.unwrap().unwrap();
    assert!(stored.ephemeral && stored.capabilities.runs_on_host());

    let invalid = fixture.call("declare", Some(&credential), declare_body(&["has space"]).to_string()).await;
    assert_eq!((invalid.status, invalid.json["code"].as_str()), (StatusCode::BAD_REQUEST, Some("bad_request")));
}

#[tokio::test]
async fn runners_unregister_themselves() {
    let fixture = Fixture::new().await;
    let credential = fixture.runner(&["linux"]).await;
    let answer = fixture.call("unregister", Some(&credential), "{}").await;
    assert_eq!(answer.status, StatusCode::NO_CONTENT);
    let gone = fixture.call("declare", Some(&credential), declare_body(&["linux"]).to_string()).await;
    assert_eq!(gone.status, StatusCode::UNAUTHORIZED, "the credential died with the runner");
    let repo = fixture.repo().await;
    assert_eq!(fixture.actions().runners(&fixture.alice, ManagedScope::Repository(&repo)).await.unwrap(), []);
}

#[tokio::test]
async fn calls_need_a_protocol_version_a_credential_and_a_valid_body() {
    let fixture = Fixture::new().await;
    let credential = fixture.runner(&["linux"]).await;
    let body = || Body::from(declare_body(&["linux"]).to_string());
    for headers in [&[][..], &[("rbit-runner-protocol", "2")][..], &[("rbit-runner-protocol", "one")][..]] {
        let answer = fixture.send("declare", headers, Some(&credential), body()).await;
        assert_eq!(answer.status, StatusCode::UPGRADE_REQUIRED, "{headers:?}");
        assert_eq!((answer.json["code"].as_str(), &answer.json["supported_versions"]), (Some("unsupported_protocol"), &json!([1])));
    }
    let answer = fixture.call("declare", None, declare_body(&["linux"]).to_string()).await;
    assert_eq!(answer.status, StatusCode::UNAUTHORIZED);
    assert!(answer.headers.contains_key(header::WWW_AUTHENTICATE));
    let forged = format!("rbrn_{}", "A".repeat(43));
    assert_eq!(fixture.call("declare", Some(&forged), declare_body(&["linux"]).to_string()).await.status, StatusCode::UNAUTHORIZED);
    assert_eq!(fixture.call("declare", Some("rbrn_short"), declare_body(&["linux"]).to_string()).await.status, StatusCode::UNAUTHORIZED);

    let malformed = fixture.call("declare", Some(&credential), "{\"name\": ").await;
    assert_eq!((malformed.status, malformed.json["code"].as_str()), (StatusCode::BAD_REQUEST, Some("bad_request")));
    let huge = format!("{{\"name\": \"{}\"}}", "x".repeat(70_000));
    let answer = fixture.call("declare", Some(&credential), huge).await;
    assert_eq!((answer.status, answer.json["code"].as_str()), (StatusCode::PAYLOAD_TOO_LARGE, Some("too_large")));
    let answer = fixture.call("unregister", Some(&credential), "x".repeat(2_000)).await;
    assert_eq!(answer.status, StatusCode::PAYLOAD_TOO_LARGE, "unregister takes at most 1 KiB");

    let mut gzipped = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    std::io::Write::write_all(&mut gzipped, declare_body(&["linux"]).to_string().as_bytes()).unwrap();
    let answer = fixture
        .send(
            "declare",
            &[("rbit-runner-protocol", "1"), ("content-encoding", "gzip")],
            Some(&credential),
            gzipped.finish().unwrap().into(),
        )
        .await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.json);
}

#[tokio::test]
async fn failed_authentication_and_registration_are_throttled() {
    let fixture = Fixture::with(|config| config.security.rate_limit = true).await;
    let credential = fixture.runner(&["linux"]).await;
    // A working runner never runs out: only failures spend the budget.
    for _ in 0..120 {
        assert_eq!(fixture.call("declare", Some(&credential), declare_body(&["linux"]).to_string()).await.status, StatusCode::OK);
    }
    let forged = format!("rbrn_{}", "B".repeat(43));
    let mut status = StatusCode::UNAUTHORIZED;
    for _ in 0..200 {
        status = fixture.call("declare", Some(&forged), declare_body(&["linux"]).to_string()).await.status;
        if status != StatusCode::UNAUTHORIZED {
            break;
        }
    }
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    let answer = fixture.call("declare", Some(&credential), declare_body(&["linux"]).to_string()).await;
    assert_eq!((answer.status, answer.json["code"].as_str()), (StatusCode::TOO_MANY_REQUESTS, Some("throttled")));
    assert!(answer.headers.contains_key(header::RETRY_AFTER));

    let mut status = StatusCode::UNAUTHORIZED;
    for _ in 0..100 {
        status = fixture.call("register", None, register_body("rbrg_guess", "x", &["linux"]).to_string()).await.status;
        if status != StatusCode::UNAUTHORIZED {
            break;
        }
    }
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "registration guesses run out");
}
