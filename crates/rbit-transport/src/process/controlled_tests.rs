//! Actual Git processes and destructor barriers verify interruption ownership.

use std::{
    io,
    pin::Pin,
    process::Stdio,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    task::{Context, Poll},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, ReadBuf},
    sync::{Semaphore, oneshot},
};
use tokio_util::sync::CancellationToken;

use super::*;

fn repository() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    assert!(std::process::Command::new("git").args(["init", "--quiet", "--bare"]).arg(dir.path()).status().unwrap().success());
    dir
}

struct GatedInput {
    started: Option<oneshot::Sender<()>>,
    destroying: Option<oneshot::Sender<()>>,
    release: mpsc::Receiver<()>,
    destroyed: Arc<AtomicBool>,
}

impl AsyncRead for GatedInput {
    fn poll_read(mut self: Pin<&mut Self>, _: &mut Context<'_>, _: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        if let Some(started) = self.started.take() {
            let _ = started.send(());
        }
        Poll::Pending
    }
}

impl Drop for GatedInput {
    fn drop(&mut self) {
        if let Some(destroying) = self.destroying.take() {
            let _ = destroying.send(());
        }
        self.release.recv_timeout(Duration::from_secs(10)).unwrap();
        self.destroyed.store(true, Ordering::SeqCst);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancellation_retains_admission_until_real_git_exit_and_feeder_destruction() {
    let repo = repository();
    let cancellation = CancellationToken::new();
    let control = ProcessControl::new(cancellation.clone(), tokio::time::Instant::now() + Duration::from_secs(30));
    let permits = Arc::new(Semaphore::new(1));
    let permit = Arc::clone(&permits).acquire_owned().await.unwrap();
    let (started, reading) = oneshot::channel();
    let (destroying, dropped) = oneshot::channel();
    let (release, resume) = mpsc::channel();
    let destroyed = Arc::new(AtomicBool::new(false));
    let input = GatedInput { started: Some(started), destroying: Some(destroying), release: resume, destroyed: Arc::clone(&destroyed) };
    let observed = Arc::clone(&permits);
    let exchange = tokio::spawn(async move {
        let _permit = permit;
        let result = ProcessTransport::new("git")
            .serve_interactive_controlled(
                Service::UploadPack,
                repo.path(),
                None,
                ProcessIo::new(input, tokio::io::sink(), tokio::io::sink()),
                &control,
            )
            .await;
        assert_eq!(observed.available_permits(), 0, "the owner releases admission only after cleanup returns");
        result
    });
    tokio::time::timeout(Duration::from_secs(10), reading).await.unwrap().unwrap();
    cancellation.cancel();
    tokio::time::timeout(Duration::from_secs(10), dropped).await.unwrap().unwrap();
    assert!(!exchange.is_finished());
    assert_eq!(permits.available_permits(), 0);
    assert!(!destroyed.load(Ordering::SeqCst));
    release.send(()).unwrap();
    assert!(matches!(exchange.await.unwrap(), Err(TransportError::Cancelled)));
    assert!(destroyed.load(Ordering::SeqCst));
    assert_eq!(permits.available_permits(), 1);
}

#[tokio::test]
async fn termination_wait_reaps_a_real_child_before_reporting_success() {
    let repo = repository();
    let mut child = ProcessTransport::new("git")
        .service_command(Service::UploadPack, None, ReceiveOptions::default())
        .arg(repo.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    assert!(child.id().is_some());
    let status = exchange::terminate(&mut child).await.unwrap();
    assert!(!status.success());
    assert!(child.id().is_none());
    assert_eq!(child.try_wait().unwrap(), Some(status));
}

#[tokio::test]
async fn deadline_reaps_an_already_started_child_and_destroys_its_reader() {
    let repo = repository();
    let transport = ProcessTransport::new("git");
    let child = transport
        .command(Service::ReceivePack, repo.path(), None, ReceiveOptions::default())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let control = ProcessControl::new(CancellationToken::new(), tokio::time::Instant::now());
    let (destroying, dropped) = oneshot::channel();
    let (release, resume) = mpsc::channel();
    release.send(()).unwrap();
    let destroyed = Arc::new(AtomicBool::new(false));
    let input = GatedInput { started: None, destroying: Some(destroying), release: resume, destroyed: Arc::clone(&destroyed) };
    let result =
        exchange::run(Service::ReceivePack, child, ProcessIo::new(input, tokio::io::sink(), tokio::io::sink()), Some(&control)).await;
    assert!(matches!(result, Err(TransportError::TimedOut)));
    dropped.await.unwrap();
    assert!(destroyed.load(Ordering::SeqCst));
}

#[tokio::test]
async fn interrupted_entries_do_not_spawn_any_git_stage() {
    let transport = ProcessTransport::new("this-git-binary-must-not-be-executed");
    let token = CancellationToken::new();
    token.cancel();
    let control = ProcessControl::new(token, tokio::time::Instant::now() + Duration::from_secs(10));
    assert!(matches!(
        transport.advertisement_controlled(Service::ReceivePack, Path::new("unused"), None, &control).await,
        Err(TransportError::Cancelled)
    ));
    let control = ProcessControl::new(CancellationToken::new(), tokio::time::Instant::now());
    assert!(matches!(
        transport
            .receive_pack_controlled(
                Path::new("unused"),
                None,
                ReceiveOptions::default(),
                ProcessIo::new(tokio::io::empty(), tokio::io::sink(), tokio::io::sink()),
                &control
            )
            .await,
        Err(TransportError::TimedOut)
    ));
    assert!(matches!(
        transport
            .serve_interactive_controlled(
                Service::UploadPack,
                Path::new("unused"),
                None,
                ProcessIo::new(tokio::io::empty(), tokio::io::sink(), tokio::io::sink()),
                &control
            )
            .await,
        Err(TransportError::TimedOut)
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn http_completion_waits_for_aborted_request_destruction() {
    let repo = repository();
    let (started, reading) = oneshot::channel();
    let (destroying, dropped) = oneshot::channel();
    let (release, resume) = mpsc::channel();
    let destroyed = Arc::new(AtomicBool::new(false));
    let input = GatedInput { started: Some(started), destroying: Some(destroying), release: resume, destroyed: Arc::clone(&destroyed) };
    let session = ProcessTransport::new("git").serve(Service::UploadPack, repo.path(), None, ReceiveOptions::default(), input).unwrap();
    tokio::time::timeout(Duration::from_secs(10), reading).await.unwrap().unwrap();
    drop(session.output);
    tokio::time::timeout(Duration::from_secs(10), dropped).await.unwrap().unwrap();
    let mut completion = session.completion;
    assert!(matches!(completion.try_recv(), Err(oneshot::error::TryRecvError::Empty)));
    assert!(!destroyed.load(Ordering::SeqCst));
    release.send(()).unwrap();
    let status = tokio::time::timeout(Duration::from_secs(10), completion).await.unwrap().unwrap();
    assert!(!status.success());
    assert!(destroyed.load(Ordering::SeqCst));
}

#[tokio::test]
async fn controlled_success_preserves_advertisements_protocol_and_exit_status() {
    let repo = repository();
    let transport = ProcessTransport::new("git");
    let control = ProcessControl::new(CancellationToken::new(), tokio::time::Instant::now() + Duration::from_secs(30));
    let advertisement = transport.advertisement_controlled(Service::ReceivePack, repo.path(), None, &control).await.unwrap();
    assert!(String::from_utf8_lossy(&advertisement).contains("report-status"));
    assert!(!advertisement.starts_with(b"001f# service="));
    let (mut output, mut errors) = (Vec::new(), Vec::new());
    let status = transport
        .serve_interactive_controlled(
            Service::UploadPack,
            repo.path(),
            Some("version=2"),
            ProcessIo::new(&b"0000"[..], &mut output, &mut errors),
            &control,
        )
        .await
        .unwrap();
    assert!(status.success());
    assert!(output.starts_with(b"000eversion 2\n"));
    output.clear();
    let status = transport
        .receive_pack_controlled(
            repo.path(),
            None,
            ReceiveOptions::default(),
            ProcessIo::new(&b"0000"[..], &mut output, &mut errors),
            &control,
        )
        .await
        .unwrap();
    assert!(status.success());
    assert_eq!(output, [0_u8; 0]);
}
