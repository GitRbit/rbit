//! Uploads that outlive their lease: an artifact or cache body still streaming when the
//! sweeper fails its task, or when a cancel and re-run replace its attempt, is not stored.
//! The lease is checked when the upload starts and again once its body has arrived.

use axum::{
    body::{Body, Bytes, to_bytes},
    http::{Method, Request, StatusCode, header},
};
use rbit_core::Timestamp;
use rbit_storage::sqlite::actions_runs::Rerun;
use tokio::sync::mpsc;
use tower::ServiceExt;

use crate::{
    actions_artifacts::hex,
    actions_runner::Fixture,
    actions_runner_tasks::{payload, poll, seed, spec, template},
};

const PARTS: [&[u8]; 3] = [b"first part, ", b"second part, ", b"last part"];

/// Starts a `PUT` of `path` whose body arrives part by part. Returns once the server has
/// taken the first part — past the checks made before the body — with the sender of the
/// last part and the pending answer.
async fn start_upload(
    fixture: &Fixture,
    credential: &str,
    path: &str,
) -> (mpsc::Sender<Result<Bytes, std::io::Error>>, tokio::task::JoinHandle<StatusCode>) {
    let (sender, receiver) = mpsc::channel(1);
    let stream = futures_util::stream::unfold(receiver, |mut receiver| async move { receiver.recv().await.map(|part| (part, receiver)) });
    let request = Request::builder()
        .method(Method::PUT)
        .uri(format!("/api/actions/runner/v1/{path}"))
        .header(header::HOST, "git.example.com")
        .header("rbit-runner-protocol", "1")
        .header("rbit-content-sha256", hex(&PARTS.concat()))
        .header(header::AUTHORIZATION, format!("Bearer {credential}"))
        .body(Body::from_stream(stream))
        .unwrap();
    let router = fixture.router.clone();
    let answer = tokio::spawn(async move {
        let response = router.oneshot(request).await.unwrap();
        let status = response.status();
        let _ = to_bytes(response.into_body(), usize::MAX).await;
        status
    });
    // The channel holds one part: the second fits only once the server read the first.
    sender.send(Ok(Bytes::from_static(PARTS[0]))).await.unwrap();
    sender.send(Ok(Bytes::from_static(PARTS[1]))).await.unwrap();
    (sender, answer)
}

async fn finish(sender: mpsc::Sender<Result<Bytes, std::io::Error>>, answer: tokio::task::JoinHandle<StatusCode>) -> StatusCode {
    sender.send(Ok(Bytes::from_static(PARTS[2]))).await.unwrap();
    drop(sender);
    answer.await.unwrap()
}

/// A running task of a new run: (run, task id).
async fn running(fixture: &Fixture, credential: &str, key: &str) -> (rbit_core::actions::Run, i64) {
    let run = seed(fixture, &[payload(&template("build", &["linux"], &[]))]).await;
    (run, spec(&poll(fixture, credential, Some(key)).await.1).task_id.0)
}

fn uploads(task: i64) -> [String; 2] {
    [format!("tasks/{task}/artifacts/report"), format!("tasks/{task}/caches?key=deps&version={}", hex(b"deps"))]
}

#[tokio::test]
async fn an_upload_whose_task_was_swept_meanwhile_is_not_stored() {
    let fixture = Fixture::with(|config| config.actions.long_poll_seconds = 1).await;
    let credential = fixture.runner(&["linux"]).await;
    for index in 0..2 {
        let (run, task) = running(&fixture, &credential, &format!("swept-{index}")).await;
        let path = &uploads(task)[index];
        let (sender, answer) = start_upload(&fixture, &credential, path).await;
        let swept = fixture.actions().sweep(Timestamp(Timestamp::now().0 + 601)).await.unwrap();
        assert_eq!(swept.zombies, 1);
        assert_eq!(finish(sender, answer).await, StatusCode::CONFLICT, "{path}: the lease ended during the upload");
        let artifacts = fixture.database.actions_artifacts().of_run(run.id, Timestamp::now()).await.unwrap();
        assert_eq!(artifacts.len(), 0, "{path}: {artifacts:?}");
    }
    let repo = fixture.repo().await.repo.repo.id;
    assert_eq!(fixture.database.actions_caches().stored_bytes(repo).await.unwrap(), 0, "no cache entry either");
}

#[tokio::test]
async fn an_upload_of_an_attempt_replaced_meanwhile_is_not_stored() {
    let fixture = Fixture::with(|config| config.actions.long_poll_seconds = 1).await;
    let credential = fixture.runner(&["linux"]).await;
    let repo = fixture.repo().await;
    for index in 0..2 {
        let (run, task) = running(&fixture, &credential, &format!("old-{index}")).await;
        let path = &uploads(task)[index];
        let (sender, answer) = start_upload(&fixture, &credential, path).await;
        fixture.actions().cancel(&fixture.alice, &repo, run.id).await.unwrap();
        fixture.actions().rerun(&fixture.alice, &repo, run.id, Rerun::All).await.unwrap();
        assert_eq!(finish(sender, answer).await, StatusCode::CONFLICT, "{path}: a re-run replaced the attempt during the upload");
        let artifacts = fixture.database.actions_artifacts().of_run(run.id, Timestamp::now()).await.unwrap();
        assert_eq!(artifacts.len(), 0, "{path}: {artifacts:?}");
        // The new attempt is left waiting; the next iteration's poll would take it.
        let repo_id = repo.repo.repo.id;
        fixture.database.actions_runs().cancel_run(repo_id, run.id, Timestamp::now()).await.unwrap();
    }
    assert_eq!(fixture.database.actions_caches().stored_bytes(repo.repo.repo.id).await.unwrap(), 0);
}
