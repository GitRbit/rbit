//! Per-storage fixed worker gate; compiled only for the crate's private tests.
//! No public callback, global gate or production ownership coordinator.
use std::{
    future::{Future, poll_fn},
    pin::Pin,
    sync::{Arc, Mutex, mpsc},
    task::Poll,
    time::Duration,
};
use tokio::{sync::oneshot, time::timeout};

use crate::{GitError, RepositoryStorage, relocate::relocate};
use rbit_core::{AccountId, RepoId};

const WAIT: Duration = Duration::from_secs(5);
const REPO: RepoId = RepoId(7);
const FROM: AccountId = AccountId(1);
const TO: AccountId = AccountId(2);

#[derive(Debug, Clone, Default)]
pub(crate) struct WorkerGates(Arc<Mutex<Option<WorkerGate>>>);
impl WorkerGates {
    pub(crate) fn take(&self) -> Option<WorkerGate> {
        self.0.lock().unwrap().take()
    }
    fn install(&self) -> Controller {
        let (entered, entry) = oneshot::channel();
        let (release, wait) = mpsc::sync_channel(1);
        let mut state = self.0.lock().unwrap();
        assert!(state.is_none(), "one gate per storage operation");
        *state = Some(WorkerGate { entered, wait });
        Controller { entry, release: Some(release) }
    }
}
#[derive(Debug)]
pub(crate) struct WorkerGate {
    entered: oneshot::Sender<()>,
    wait: mpsc::Receiver<Decision>,
}
impl WorkerGate {
    pub(crate) fn enter(self) {
        self.entered.send(()).expect("test controller disappeared before worker entry");
        match self.wait.recv_timeout(WAIT) {
            Ok(Decision::Proceed) => {}
            Ok(Decision::Cancel) => panic!("test controller canceled; filesystem operation refused"),
            Err(error) => panic!("worker gate failed closed before filesystem operation: {error}"),
        }
    }
}
#[derive(Debug)]
enum Decision {
    Proceed,
    Cancel,
}
struct Controller {
    entry: oneshot::Receiver<()>,
    release: Option<mpsc::SyncSender<Decision>>,
}
impl Controller {
    async fn entered(&mut self) {
        timeout(WAIT, &mut self.entry).await.unwrap().expect("actual worker never acknowledged entry");
    }
    fn release(mut self) {
        self.release.take().unwrap().try_send(Decision::Proceed).expect("worker no longer waiting");
    }
}
impl Drop for Controller {
    fn drop(&mut self) {
        if let Some(release) = self.release.take() {
            // Unwind/disconnect must never authorize a destructive operation.
            // The bounded worker wait ends with a test failure and releases its lease.
            let _ = release.try_send(Decision::Cancel);
        }
    }
}
#[derive(Debug, Clone, Copy)]
enum Operation {
    Delete,
    Relocate,
}
async fn pending<F: Future + ?Sized>(mut future: Pin<&mut F>) {
    poll_fn(|cx| {
        assert!(future.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
}

struct Fixture {
    _directory: tempfile::TempDir,
    storage: RepositoryStorage,
    operation: Operation,
    failure: bool,
}
impl Fixture {
    async fn new(operation: Operation, failure: bool) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let storage = RepositoryStorage::new(directory.path());
        let source = storage.path_for(FROM, REPO);
        if matches!(operation, Operation::Delete) && failure {
            std::fs::create_dir_all(source.parent().unwrap()).unwrap();
            std::fs::write(&source, "ordinary file").unwrap();
        } else {
            storage.create(FROM, REPO, "main").await.unwrap();
            std::fs::write(source.join("marker"), "original").unwrap();
        }
        if matches!(operation, Operation::Relocate) && failure {
            let target = storage.create(TO, REPO, "main").await.unwrap();
            std::fs::write(target.join("marker"), "existing").unwrap();
        }
        Self { _directory: directory, storage, operation, failure }
    }
    fn assert_filesystem(&self) {
        let source = self.storage.path_for(FROM, REPO);
        let target = self.storage.path_for(TO, REPO);
        match (self.operation, self.failure) {
            (Operation::Delete, false) => assert!(!source.exists()),
            (Operation::Delete, true) => assert_eq!(std::fs::read_to_string(source).unwrap(), "ordinary file"),
            (Operation::Relocate, false) => {
                assert!(!source.exists());
                assert_eq!(std::fs::read_to_string(target.join("marker")).unwrap(), "original");
                assert_eq!(std::fs::read_to_string(target.join("HEAD")).unwrap(), "ref: refs/heads/main\n");
            }
            (Operation::Relocate, true) => {
                assert_eq!(std::fs::read_to_string(source.join("marker")).unwrap(), "original");
                assert_eq!(std::fs::read_to_string(target.join("marker")).unwrap(), "existing");
            }
        }
    }
}
async fn run(operation: Operation, failure: bool) {
    let f = Fixture::new(operation, failure).await;
    let mut controller = f.storage.worker_gates.install();
    let storage = f.storage.clone();
    let task = tokio::spawn(async move {
        match operation {
            Operation::Delete => storage.delete(FROM, REPO).await,
            Operation::Relocate => relocate(&storage, REPO, FROM, TO).await.map(|_| ()),
        }
    });
    controller.entered().await;
    let task = if failure {
        // Retain the error waiter to prove the exact real operation error, not a gate panic.
        Some(task)
    } else {
        task.abort();
        assert!(timeout(WAIT, task).await.unwrap().unwrap_err().is_cancelled());
        None
    };
    // Poll actual lock acquisitions once after entry (and, on success, waiter cancellation).
    // Neither reader nor second editor may enter while the filesystem worker is gated.
    let mut reader = Box::pin(f.storage.lease(REPO));
    let mut editor = Box::pin(f.storage.exclusive(REPO));
    pending(reader.as_mut()).await;
    pending(editor.as_mut()).await;
    drop(reader);
    drop(editor); // Do not let probes acquire and deadlock one another after release.
    controller.release();
    if let Some(task) = task {
        let error = timeout(WAIT, task).await.unwrap().unwrap().unwrap_err();
        match operation {
            Operation::Delete => assert!(matches!(error, GitError::Io(error) if error.kind() == std::io::ErrorKind::NotADirectory)),
            Operation::Relocate => assert!(matches!(error, GitError::AlreadyExists)),
        }
    }
    // The shared lease synchronizes actual worker completion/lease destruction, rather
    // than guessing completion from a timer or from the canceled outer JoinHandle.
    let reader = timeout(WAIT, f.storage.lease(REPO)).await.unwrap();
    f.assert_filesystem();
    drop(reader);
    drop(timeout(WAIT, f.storage.exclusive(REPO)).await.unwrap());
}

#[tokio::test]
async fn canceled_delete_waiter_does_not_release_running_worker_lease() {
    run(Operation::Delete, false).await;
}
#[tokio::test]
async fn canceled_relocate_waiter_does_not_release_running_worker_lease() {
    run(Operation::Relocate, false).await;
}
#[tokio::test]
async fn real_delete_file_error_releases_worker_lease_and_preserves_file() {
    run(Operation::Delete, true).await;
}
#[tokio::test]
async fn real_relocate_target_error_releases_worker_lease_and_preserves_both() {
    run(Operation::Relocate, true).await;
}
