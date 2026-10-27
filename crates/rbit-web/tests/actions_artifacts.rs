//! Actions workstream A: runners upload artifacts through their running task, list and
//! download them within the run; sizes, quotas, digests and names are checked, and the
//! sweeper removes expired ones with their files.

use axum::{
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode, header},
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tower::ServiceExt;

use crate::{
    actions_runner::Fixture,
    actions_runner_tasks::{payload, poll, seed, spec, template},
};

fn hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().fold(String::new(), |mut hex, byte| {
        use std::fmt::Write as _;
        let _ = write!(hex, "{byte:02x}");
        hex
    })
}

pub struct Answer {
    pub status: StatusCode,
    pub headers: axum::http::HeaderMap,
    pub body: Vec<u8>,
}

impl Answer {
    pub fn json(&self) -> Value {
        serde_json::from_slice(&self.body).unwrap_or(Value::Null)
    }
}

async fn call(fixture: &Fixture, method: Method, path: &str, credential: &str, digest: Option<&str>, body: Vec<u8>) -> Answer {
    let mut request = Request::builder()
        .method(method)
        .uri(format!("/api/actions/runner/v1/{path}"))
        .header(header::HOST, "git.example.com")
        .header("rbit-runner-protocol", "1")
        .header(header::AUTHORIZATION, format!("Bearer {credential}"));
    if let Some(digest) = digest {
        request = request.header("rbit-content-sha256", digest);
    }
    let response = fixture.router.clone().oneshot(request.body(Body::from(body)).unwrap()).await.unwrap();
    let (status, headers) = (response.status(), response.headers().clone());
    Answer { status, headers, body: to_bytes(response.into_body(), usize::MAX).await.unwrap().to_vec() }
}

pub async fn upload(fixture: &Fixture, credential: &str, task: i64, name: &str, query: &str, bytes: &[u8]) -> Answer {
    let digest = hex(bytes);
    call(fixture, Method::PUT, &format!("tasks/{task}/artifacts/{name}{query}"), credential, Some(&digest), bytes.to_vec()).await
}

#[tokio::test]
async fn runners_upload_list_and_download_the_artifacts_of_their_run() {
    let fixture = Fixture::with(|config| config.actions.max_artifact_bytes = 64).await;
    let credential = fixture.runner(&["linux"]).await;
    seed(&fixture, &[payload(&template("build", &["linux"], &[])), payload(&template("test", &["linux"], &[]))]).await;
    let task = spec(&poll(&fixture, &credential, Some("a")).await.1).task_id.0;

    let answer = upload(&fixture, &credential, task, "dist", "?retention_days=3", b"tar.gz bytes").await;
    assert_eq!(answer.status, StatusCode::CREATED, "{:?}", answer.json());
    assert_eq!(answer.json()["size"], 12);
    assert_eq!(answer.json()["sha256"], hex(b"tar.gz bytes"));

    let again = upload(&fixture, &credential, task, "dist", "", b"other").await;
    assert_eq!(again.status, StatusCode::CONFLICT, "names are unique per run attempt");
    assert_eq!(upload(&fixture, &credential, task, "dist", "?overwrite=true&retention_days=3", b"newer").await.status, StatusCode::CREATED);
    let wrong = call(&fixture, Method::PUT, &format!("tasks/{task}/artifacts/x"), &credential, Some(&hex(b"a")), b"b".to_vec()).await;
    assert_eq!(wrong.status, StatusCode::BAD_REQUEST, "the digest must match");
    assert_eq!(upload(&fixture, &credential, task, "big", "", &[0; 65]).await.status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(upload(&fixture, &credential, task, "bad%20name", "", b"x").await.status, StatusCode::BAD_REQUEST);
    // A name that would walk up a directory where a runner unpacks it (`%2E%2E` reaches the
    // handler as `..`; a literal `..` is resolved away by the client before it is sent).
    assert_eq!(upload(&fixture, &credential, task, "%2E%2E", "", b"x").await.status, StatusCode::BAD_REQUEST);
    let missing = call(&fixture, Method::PUT, &format!("tasks/{task}/artifacts/x"), &credential, None, b"x".to_vec()).await;
    assert_eq!(missing.status, StatusCode::BAD_REQUEST, "the digest header is required");

    // Another job of the run sees it; another runner gets nothing.
    let other_task = spec(&poll(&fixture, &credential, Some("b")).await.1).task_id.0;
    let list = call(&fixture, Method::GET, &format!("tasks/{other_task}/artifacts"), &credential, None, Vec::new()).await;
    assert_eq!(list.status, StatusCode::OK);
    let names: Vec<_> = list.json()["artifacts"].as_array().unwrap().iter().map(|artifact| artifact["name"].clone()).collect();
    assert_eq!(names, [Value::from("dist")]);
    let download = call(&fixture, Method::GET, &format!("tasks/{other_task}/artifacts/dist"), &credential, None, Vec::new()).await;
    assert_eq!(download.status, StatusCode::OK);
    assert_eq!(download.body, b"newer");
    assert_eq!(download.headers["rbit-content-sha256"], hex(b"newer").as_str());
    let absent = call(&fixture, Method::GET, &format!("tasks/{other_task}/artifacts/nope"), &credential, None, Vec::new()).await;
    assert_eq!(absent.status, StatusCode::NOT_FOUND);
    let stranger = fixture.runner(&["windows"]).await;
    assert_eq!(
        call(&fixture, Method::GET, &format!("tasks/{task}/artifacts"), &stranger, None, Vec::new()).await.status,
        StatusCode::FORBIDDEN
    );

    // The repository's quota counts every artifact; expired ones go with their files.
    let stored = fixture.database.actions_artifacts().stored_bytes(fixture.repo().await.repo.repo.id).await.unwrap();
    assert_eq!(stored, 5);
    let actions = fixture.actions();
    let now = rbit_core::Timestamp::now();
    let swept = actions.sweep(rbit_core::Timestamp(now.0 + 4 * 86_400)).await.unwrap();
    assert_eq!(swept.expired_artifacts, 1, "three days asked, so gone after four");
    assert_eq!(fixture.database.actions_artifacts().stored_bytes(fixture.repo().await.repo.repo.id).await.unwrap(), 0);
}

#[tokio::test]
async fn a_full_repository_takes_no_more_artifacts() {
    let fixture = Fixture::with(|config| {
        config.actions.max_artifact_bytes = 8;
        config.actions.max_artifact_storage_bytes = 10;
    })
    .await;
    let credential = fixture.runner(&["linux"]).await;
    seed(&fixture, &[payload(&template("build", &["linux"], &[]))]).await;
    let task = spec(&poll(&fixture, &credential, Some("a")).await.1).task_id.0;
    assert_eq!(upload(&fixture, &credential, task, "one", "", b"12345678").await.status, StatusCode::CREATED);
    let answer = upload(&fixture, &credential, task, "two", "", b"123").await;
    assert_eq!(answer.status, StatusCode::PAYLOAD_TOO_LARGE);
    assert!(answer.json()["message"].as_str().unwrap().contains("storage"), "{:?}", answer.json());
}

async fn page(fixture: &Fixture, user: &rbit_core::User, path: &str) -> Answer {
    let token = fixture.services.accounts.start_session(user).await.unwrap();
    let request = Request::builder()
        .uri(path)
        .header(header::HOST, "localhost:3000")
        .header(header::COOKIE, format!("rbit_session={}", token.encode()))
        .body(Body::empty())
        .unwrap();
    let response = fixture.router.clone().oneshot(request).await.unwrap();
    let (status, headers) = (response.status(), response.headers().clone());
    Answer { status, headers, body: to_bytes(response.into_body(), usize::MAX).await.unwrap().to_vec() }
}

#[tokio::test]
async fn run_pages_list_artifacts_and_readers_download_them() {
    let fixture = Fixture::new().await;
    let credential = fixture.runner(&["linux"]).await;
    let run = seed(&fixture, &[payload(&template("build", &["linux"], &[]))]).await;
    let task = spec(&poll(&fixture, &credential, Some("a")).await.1).task_id.0;
    assert_eq!(upload(&fixture, &credential, task, "dist-linux", "", b"archive").await.status, StatusCode::CREATED);
    let run_url = format!("/alice/rocket/actions/runs/{}", run.number);

    let alice = fixture.alice.clone();
    let shown = page(&fixture, &alice, &run_url).await;
    let html = String::from_utf8_lossy(&shown.body).into_owned();
    assert!(html.contains("dist-linux") && html.contains("Artifacts"), "{html}");
    assert!(html.contains(">in 3 months</time>"), "the default 90 days, not \"just now\": {html}");
    let artifact = fixture.database.actions_artifacts().of_run(run.id, rbit_core::Timestamp::now()).await.unwrap().remove(0);
    let download_url = format!("{run_url}/artifacts/{}", artifact.id.0);
    assert!(html.contains(&download_url));

    let download = page(&fixture, &alice, &download_url).await;
    assert_eq!(download.status, StatusCode::OK);
    assert_eq!(download.body, b"archive");
    assert_eq!(download.headers[header::CONTENT_TYPE], "application/gzip");
    assert_eq!(download.headers[header::CONTENT_DISPOSITION], "attachment; filename=\"dist-linux.tar.gz\"");
    assert_eq!(
        page(&fixture, &alice, &format!("/alice/rocket/actions/runs/{}/artifacts/{}", run.number + 1, artifact.id.0)).await.status,
        StatusCode::NOT_FOUND
    );

    let bob = fixture
        .services
        .accounts
        .create_user(
            rbit_services::SignUp { username: "bob".into(), email: "bob@example.com".into(), password: "correct horse battery".into() },
            Some(false),
        )
        .await
        .unwrap();
    assert_eq!(page(&fixture, &bob, &download_url).await.status, StatusCode::NOT_FOUND, "the repository is private");
}
