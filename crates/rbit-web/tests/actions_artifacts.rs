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

/// An artifact as rbit-runner uploads it: a tar.gz with `files` under `w/`.
pub fn stored(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast()));
    for (name, data) in files {
        let mut header = tar::Header::new_gnu();
        header.set_size(data.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder.append_data(&mut header, format!("w/{name}"), *data).unwrap();
    }
    builder.into_inner().unwrap().finish().unwrap()
}

/// The files of a zip, by name.
pub fn zip_files(bytes: &[u8]) -> Vec<(String, String)> {
    use std::io::Read as _;
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    (0..archive.len())
        .map(|index| {
            let mut file = archive.by_index(index).unwrap();
            let mut text = String::new();
            file.read_to_string(&mut text).unwrap();
            (file.name().unwrap().into_owned(), text)
        })
        .collect()
}

/// The files of a tar.gz, by name.
pub fn tar_gz_files(bytes: &[u8]) -> Vec<(String, String)> {
    use std::io::Read as _;
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(bytes));
    archive
        .entries()
        .unwrap()
        .map(|entry| {
            let mut entry = entry.unwrap();
            let mut text = String::new();
            entry.read_to_string(&mut text).unwrap();
            (entry.path().unwrap().display().to_string(), text)
        })
        .collect()
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
    let archive = stored(&[("app", b"binary"), ("docs/README.md", b"read me")]);
    assert_eq!(upload(&fixture, &credential, task, "dist-linux", "", &archive).await.status, StatusCode::CREATED);
    let run_url = format!("/alice/rocket/actions/runs/{}", run.number);

    let alice = fixture.alice.clone();
    let shown = page(&fixture, &alice, &run_url).await;
    let html = String::from_utf8_lossy(&shown.body).into_owned();
    assert!(html.contains("dist-linux") && html.contains("Artifacts"), "{html}");
    assert!(html.contains(">in 3 months</time>"), "the default 90 days, not \"just now\": {html}");
    let artifact = fixture.database.actions_artifacts().of_run(run.id, rbit_core::Timestamp::now()).await.unwrap().remove(0);
    let download_url = format!("{run_url}/artifacts/{}", artifact.id.0);
    assert!(html.contains(&download_url));

    assert!(html.contains(&format!("{download_url}/zip")) && html.contains(&format!("{download_url}/tar.gz")), "{html}");
    let files = [("app".to_owned(), "binary".to_owned()), ("docs/README.md".to_owned(), "read me".to_owned())];
    for path in [download_url.clone(), format!("{download_url}/tar.gz")] {
        let download = page(&fixture, &alice, &path).await;
        assert_eq!(download.status, StatusCode::OK, "{path}");
        assert_eq!(tar_gz_files(&download.body), files, "{path}: the files at the top, without the runner's `w/`");
        assert_eq!(download.headers[header::CONTENT_TYPE], "application/gzip");
        assert_eq!(download.headers[header::CONTENT_DISPOSITION], "attachment; filename=\"dist-linux.tar.gz\"");
    }
    let download = page(&fixture, &alice, &format!("{download_url}/zip")).await;
    assert_eq!(download.status, StatusCode::OK);
    assert_eq!(zip_files(&download.body), files);
    assert_eq!(download.headers[header::CONTENT_TYPE], "application/zip");
    assert_eq!(download.headers[header::CONTENT_DISPOSITION], "attachment; filename=\"dist-linux.zip\"");
    assert_eq!(page(&fixture, &alice, &format!("{download_url}/rar")).await.status, StatusCode::NOT_FOUND);
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

#[tokio::test]
async fn a_task_left_behind_by_a_rerun_cannot_upload_into_the_new_attempt() {
    let fixture = Fixture::with(|config| config.actions.long_poll_seconds = 1).await;
    let credential = fixture.runner(&["linux"]).await;
    let run = seed(&fixture, &[payload(&template("build", &["linux"], &[]))]).await;
    let old = spec(&poll(&fixture, &credential, Some("old")).await.1).task_id.0;
    let repo = fixture.repo().await;
    fixture.actions().cancel(&fixture.alice, &repo, run.id).await.unwrap();
    fixture.actions().rerun(&fixture.alice, &repo, run.id, rbit_storage::sqlite::actions_runs::Rerun::All).await.unwrap();
    let new = spec(&poll(&fixture, &credential, Some("new")).await.1).task_id.0;

    assert_eq!(upload(&fixture, &credential, old, "dist", "", b"stale bytes").await.status, StatusCode::CONFLICT);
    assert_eq!(upload(&fixture, &credential, new, "dist", "", b"fresh bytes").await.status, StatusCode::CREATED);
}
