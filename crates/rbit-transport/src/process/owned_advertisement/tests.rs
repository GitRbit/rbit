//! Real child ownership survives cancellation of the actual advertisement waiter.

use super::*;
use std::sync::Arc;
use std::{sync::mpsc, time::Duration};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

struct Admission {
    permit: Option<OwnedSemaphorePermit>,
    destroying: Option<oneshot::Sender<()>>,
    release: mpsc::Receiver<()>,
    destroyed: Option<oneshot::Sender<()>>,
}

impl Drop for Admission {
    fn drop(&mut self) {
        self.destroying.take().unwrap().send(()).unwrap();
        self.release.recv_timeout(Duration::from_secs(10)).unwrap();
        drop(self.permit.take());
        let _ = self.destroyed.take().unwrap().send(());
    }
}

fn waiting_git() -> (tempfile::TempDir, Child, tokio::process::ChildStdin) {
    let repo = tempfile::tempdir().unwrap();
    assert!(std::process::Command::new("git").args(["init", "--quiet", "--bare"]).arg(repo.path()).status().unwrap().success());
    let mut child = ProcessTransport::new("git")
        .service_command(Service::UploadPack, None, ReceiveOptions::default())
        .arg(repo.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // Keep the input open independently so the real Git child cannot finish by EOF.
    let stdin = child.stdin.take().unwrap();
    (repo, child, stdin)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dropping_the_advertisement_waiter_cancels_but_does_not_drop_its_admission() {
    let (_repo, child, _stdin) = waiting_git();
    let permits = Arc::new(Semaphore::new(1));
    let (destroying, reaped) = oneshot::channel();
    let (destroyed, released) = oneshot::channel();
    let (release, resume) = mpsc::channel();
    let admission = Admission {
        permit: Some(Arc::clone(&permits).acquire_owned().await.unwrap()),
        destroying: Some(destroying),
        release: resume,
        destroyed: Some(destroyed),
    };
    let control = ProcessControl::new(CancellationToken::new(), tokio::time::Instant::now() + Duration::from_secs(30));
    let mut waiter = Box::pin(owned(Service::UploadPack, child, control, admission));
    tokio::select! {
        biased;
        result = &mut waiter => panic!("Git must stay waiting for input: {result:?}"),
        () = tokio::task::yield_now() => {},
    }
    drop(waiter);
    tokio::time::timeout(Duration::from_secs(10), reaped).await.unwrap().unwrap();
    assert_eq!(permits.available_permits(), 0, "the owner retains admission through real child cleanup and guard destruction");
    release.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(10), released).await.unwrap().unwrap();
    assert_eq!(permits.available_permits(), 1);
}

#[tokio::test]
async fn deadline_awaits_cleanup_before_owned_advertisement_returns() {
    let (_repo, child, _stdin) = waiting_git();
    let permits = Arc::new(Semaphore::new(1));
    let permit = Arc::clone(&permits).acquire_owned().await.unwrap();
    let control = ProcessControl::new(CancellationToken::new(), tokio::time::Instant::now());
    let result = owned(Service::UploadPack, child, control, permit).await;
    assert!(matches!(result, Err(TransportError::TimedOut)));
    assert_eq!(permits.available_permits(), 1);
}

#[tokio::test]
async fn owned_advertisement_preserves_http_headers_inside_v0_and_v2_bodies() {
    let (repo, mut child, stdin) = waiting_git();
    drop(stdin);
    child.wait().await.unwrap();
    let transport = ProcessTransport::new("git");
    for protocol in [None, Some("version=2"), Some("version=2\nx")] {
        let expected = transport.advertise_refs(Service::UploadPack, repo.path(), protocol).await.unwrap();
        let control = ProcessControl::new(CancellationToken::new(), tokio::time::Instant::now() + Duration::from_secs(30));
        let actual = transport.advertise_refs_owned(Service::UploadPack, repo.path(), protocol, control, ()).await.unwrap();
        assert_eq!(actual, expected);
    }
}
