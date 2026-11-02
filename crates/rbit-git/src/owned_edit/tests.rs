//! Real filesystem/worker tests; no SQL, HTTP waiter or process custody claim.
use super::{BoundRepositoryEdit, GitAdmissionError, GitEditOutput, OwnedGitDrainError, OwnedGitResult, WorkerLoss};
use crate::{GitError, RepositoryStorage};
use rbit_core::{AccountId, RepoId};
use std::{
    future::{Future, poll_fn},
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    task::Poll,
    time::Duration,
};
use tokio::time::{Instant, timeout};

const WAIT: Duration = Duration::from_secs(5);
const REPO: RepoId = RepoId(7);
const FROM: AccountId = AccountId(1);
const TO: AccountId = AccountId(2);

struct Context {
    drops: Arc<AtomicUsize>,
}
impl Drop for Context {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}
fn context() -> (Context, Arc<AtomicUsize>) {
    let drops = Arc::new(AtomicUsize::new(0));
    (Context { drops: drops.clone() }, drops)
}
fn edit<K: Send + 'static>(storage: &RepositoryStorage, context: K) -> BoundRepositoryEdit<K> {
    storage.reserve_edit().unwrap().attach(context).unwrap().bind(REPO)
}
fn returned<K: Send + 'static>(result: OwnedGitResult<K>) -> (BoundRepositoryEdit<K>, Result<GitEditOutput, GitError>) {
    match result {
        OwnedGitResult::Returned { edit, operation } => (edit, operation),
        OwnedGitResult::Unconfirmed { loss, .. } => panic!("unexpected unconfirmed operation: {loss:?}"),
    }
}
async fn pending<F: Future + ?Sized>(mut future: Pin<&mut F>) {
    poll_fn(|cx| {
        assert!(future.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
}
async fn worker_finished(custody: &super::state::Custody) {
    timeout(WAIT, async {
        loop {
            let changed = custody.registry.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if custody.worker_finished() {
                return;
            }
            changed.await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn four_fixed_operations_return_same_context_and_release_success_cycle() {
    let directory = tempfile::tempdir().unwrap();
    let storage = RepositoryStorage::new(directory.path());
    let weak = Arc::downgrade(&storage.owned_edits);
    let (value, drops) = context();
    let (command, result) = returned(edit(&storage, value).create(FROM, "main".into()).await);
    let source = storage.path_for(FROM, REPO);
    assert_eq!(result.unwrap(), GitEditOutput::Created(source.clone()));
    std::fs::write(source.join("marker"), "retained").unwrap();
    let (command, result) = returned(command.set_head(FROM, "next".into()).await);
    assert_eq!(result.unwrap(), GitEditOutput::HeadUpdated);
    assert_eq!(std::fs::read_to_string(source.join("HEAD")).unwrap(), "ref: refs/heads/next\n");
    let (command, result) = returned(command.relocate(FROM, TO).await);
    let target = storage.path_for(TO, REPO);
    assert_eq!(result.unwrap(), GitEditOutput::Relocated(target.clone()));
    assert!(!source.exists());
    assert_eq!(std::fs::read_to_string(target.join("marker")).unwrap(), "retained");
    let (command, result) = returned(command.delete(TO).await);
    assert_eq!(result.unwrap(), GitEditOutput::Deleted);
    assert!(!target.exists());
    let value = command.finish().unwrap();
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    assert_eq!(storage.owned_edits.snapshot(), (false, 0, false));
    drop(value);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    drop(storage);
    assert!(weak.upgrade().is_none());
}

#[tokio::test]
async fn ordinary_errors_return_context_and_reuse_more_than_two_credits() {
    let directory = tempfile::tempdir().unwrap();
    let storage = RepositoryStorage::new(directory.path());
    storage.create(FROM, REPO, "main").await.unwrap();
    storage.create(TO, REPO, "main").await.unwrap();
    for _ in 0..3 {
        let (command, result) = returned(edit(&storage, ()).create(FROM, "main".into()).await);
        assert!(matches!(result, Err(GitError::AlreadyExists)));
        command.finish().unwrap();
        let (command, result) = returned(edit(&storage, ()).relocate(FROM, TO).await);
        assert!(matches!(result, Err(GitError::AlreadyExists)));
        command.finish().unwrap();
    }
    let (command, result) = returned(edit(&storage, ()).set_head(AccountId(9), "main".into()).await);
    assert!(matches!(result, Err(GitError::NotFound("repository"))));
    command.finish().unwrap();
    std::fs::remove_dir_all(storage.path_for(TO, REPO)).unwrap();
    std::fs::write(storage.path_for(TO, REPO), "ordinary file").unwrap();
    let (command, result) = returned(edit(&storage, ()).delete(TO).await);
    assert!(matches!(result, Err(GitError::Io(error)) if error.kind() == std::io::ErrorKind::NotADirectory));
    command.finish().unwrap();
    assert_eq!(storage.owned_edits.snapshot(), (false, 0, false));
    assert!(storage.path_for(FROM, REPO).is_dir());
}

#[tokio::test]
async fn reservations_are_shared_bounded_and_stale_attach_returns_original_context() {
    let directory = tempfile::tempdir().unwrap();
    let storage = RepositoryStorage::new(directory.path());
    let first = storage.reserve_edit().unwrap();
    let second = storage.clone().reserve_edit().unwrap();
    assert_eq!(storage.reserve_edit().unwrap_err(), GitAdmissionError::Capacity);
    drop(second);
    let second = storage.reserve_edit().unwrap();
    storage.prepare_owned_shutdown();
    let value = Box::new("not Debug exposed");
    let pointer = std::ptr::from_ref(&*value);
    let refused = first.attach(value).unwrap_err();
    assert_eq!(refused.reason, GitAdmissionError::Stopped);
    assert_eq!(std::ptr::from_ref(&*refused.context), pointer);
    assert!(!format!("{refused:?}").contains("not Debug exposed"));
    drop(refused);
    drop(second);
    assert_eq!(storage.reserve_edit().unwrap_err(), GitAdmissionError::Stopped);
    storage.drain_owned(Instant::now() + WAIT).await.unwrap();
}

#[tokio::test]
async fn attached_flow_continues_after_stop_but_retained_reservation_prevents_drain() {
    let directory = tempfile::tempdir().unwrap();
    let storage = RepositoryStorage::new(directory.path());
    assert_eq!(storage.drain_owned(Instant::now() + WAIT).await, Err(OwnedGitDrainError::NotStopped));
    let command = edit(&storage, ());
    let spare = storage.reserve_edit().unwrap();
    storage.prepare_owned_shutdown();
    let mut drain = Box::pin(storage.drain_owned(Instant::now() + WAIT));
    pending(drain.as_mut()).await;
    let (command, result) = returned(command.create(FROM, "main".into()).await);
    result.unwrap();
    command.finish().unwrap();
    pending(drain.as_mut()).await;
    drop(spare);
    drain.await.unwrap();
    assert_eq!(storage.drain_owned(Instant::now()).await, Err(OwnedGitDrainError::Deadline));
}

#[tokio::test]
async fn actual_wrapper_loss_poisons_while_worker_keeps_lease_and_finishes_filesystem() {
    let directory = tempfile::tempdir().unwrap();
    let storage = RepositoryStorage::new(directory.path());
    let weak = Arc::downgrade(&storage.owned_edits);
    let (value, drops) = context();
    let command = edit(&storage, value);
    let custody = command.core.custody.clone();
    let mut gate = storage.worker_gates.install();
    let task = tokio::spawn(command.create(FROM, "main".into()));
    gate.entered().await;
    task.abort();
    assert!(timeout(WAIT, task).await.unwrap().unwrap_err().is_cancelled());
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    assert!(storage.owned_edits.is_unknown());
    let mut reader = Box::pin(storage.lease(REPO));
    let mut editor = Box::pin(storage.exclusive(REPO));
    pending(reader.as_mut()).await;
    pending(editor.as_mut()).await;
    drop(reader);
    drop(editor);
    gate.release();
    worker_finished(&custody).await;
    assert_eq!(std::fs::read_to_string(storage.path_for(FROM, REPO).join("HEAD")).unwrap(), "ref: refs/heads/main\n");
    // Finished fixed IO does not erase loss of the actual owner/internal result.
    let mut reader = Box::pin(storage.lease(REPO));
    pending(reader.as_mut()).await;
    drop(reader);
    storage.prepare_owned_shutdown();
    assert_eq!(storage.drain_owned(Instant::now() + WAIT).await, Err(OwnedGitDrainError::Unconfirmed));
    drop(custody);
    drop(storage);
    assert!(weak.upgrade().is_some(), "unknown anchor survives last storage handle");
}

#[tokio::test]
async fn injected_worker_panic_retains_custody_but_does_not_promise_context_survival() {
    let directory = tempfile::tempdir().unwrap();
    let storage = RepositoryStorage::new(directory.path());
    let (value, drops) = context();
    let command = edit(&storage, value);
    let mut gate = storage.worker_gates.install();
    let task = tokio::spawn(command.create(FROM, "main".into()));
    gate.entered().await;
    drop(gate); // Injected fail-closed gate panic before real filesystem work.
    let result = timeout(WAIT, task).await.unwrap().unwrap();
    assert!(matches!(result, OwnedGitResult::Unconfirmed { loss: WorkerLoss::WorkerPanicked, .. }));
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    assert!(!storage.path_for(FROM, REPO).exists());
    assert_eq!(storage.owned_edits.snapshot(), (false, 1, true));
    let mut reader = Box::pin(storage.lease(REPO));
    pending(reader.as_mut()).await;
}

#[tokio::test]
async fn lost_internal_payload_and_unpolled_operation_never_release_credit_cleanly() {
    let directory = tempfile::tempdir().unwrap();
    let storage = RepositoryStorage::new(directory.path());
    let (command, result) = returned(edit(&storage, ()).create(FROM, "main".into()).await);
    result.unwrap();
    drop(command); // Internal returned envelope loss, NOT an HTTP final reply waiter.
    let future = edit(&storage, ()).delete(TO);
    drop(future); // Future owns the edit even before first poll; no worker was started.
    assert_eq!(storage.owned_edits.snapshot(), (false, 2, true));
    assert_eq!(storage.reserve_edit().unwrap_err(), GitAdmissionError::Capacity);
    assert!(storage.path_for(FROM, REPO).exists());
}

#[tokio::test]
async fn explicit_quarantine_retains_recoverable_context_without_public_release() {
    let directory = tempfile::tempdir().unwrap();
    let storage = RepositoryStorage::new(directory.path());
    let (value, drops) = context();
    let retained = edit(&storage, value).quarantine();
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    drop(retained);
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    assert_eq!(storage.owned_edits.snapshot(), (false, 1, true));
}

#[test]
fn closed_captured_runtime_drops_payload_without_locked_spawn_or_false_success() {
    let (finished, receive) = mpsc::sync_channel(1);
    let thread = std::thread::spawn(move || {
        let directory = tempfile::tempdir().unwrap();
        let storage = RepositoryStorage::new(directory.path());
        let closed = tokio::runtime::Runtime::new().unwrap();
        let handle = closed.handle().clone();
        drop(closed);
        let (value, drops) = context();
        let command = {
            let _entered = handle.enter();
            edit(&storage, value)
        };
        let live = tokio::runtime::Runtime::new().unwrap();
        live.block_on(async {
            let result = timeout(WAIT, command.create(FROM, "main".into())).await.unwrap();
            assert!(matches!(result, OwnedGitResult::Unconfirmed { loss: WorkerLoss::CancelledOrRuntimeClosed, .. }));
            assert_eq!(drops.load(Ordering::SeqCst), 1);
            assert!(!storage.path_for(FROM, REPO).exists());
            storage.prepare_owned_shutdown();
            assert_eq!(storage.drain_owned(Instant::now() + WAIT).await, Err(OwnedGitDrainError::Unconfirmed));
        });
        finished.send(()).unwrap();
    });
    receive.recv_timeout(WAIT).expect("closed-runtime spawn deadlocked");
    thread.join().unwrap();
}

#[tokio::test]
async fn known_pre_effect_cancel_returns_context_without_unknown_or_success_cycle() {
    let directory = tempfile::tempdir().unwrap();
    let storage = RepositoryStorage::new(directory.path());
    let weak = Arc::downgrade(&storage.owned_edits);
    let mut command = storage.reserve_edit().unwrap().attach(String::from("private context")).unwrap();
    command.context_mut().push_str(" retained");
    assert!(!format!("{command:?}").contains("private context"));
    assert_eq!(command.cancel_before_effects().unwrap(), "private context retained");
    assert_eq!(storage.owned_edits.snapshot(), (false, 0, false));
    drop(storage);
    assert!(weak.upgrade().is_none());
}

#[test]
fn reservation_without_runtime_is_known_refusal_without_registering_credit() {
    let directory = tempfile::tempdir().unwrap();
    let storage = RepositoryStorage::new(directory.path());
    assert_eq!(storage.reserve_edit().unwrap_err(), GitAdmissionError::RuntimeUnavailable);
    assert_eq!(storage.owned_edits.snapshot(), (false, 0, false));
}

/// Panics only after the named inner Debug type AND one of its fields appeared.
/// A first-write panic would not exercise the original lock-held formatting path.
struct InnerFormatterPanic {
    target: &'static str,
    field: &'static str,
    value: &'static str,
    entered: bool,
    field_seen: bool,
    reached: bool,
}
impl std::fmt::Write for InnerFormatterPanic {
    fn write_str(&mut self, text: &str) -> std::fmt::Result {
        if text == self.target {
            self.entered = true;
        }
        if self.entered && text == self.field {
            self.field_seen = true;
        }
        if self.field_seen && text == self.value {
            self.reached = true;
            panic!("injected panic inside named state Debug field formatting");
        }
        Ok(())
    }
}
fn panic_in_inner_debug(value: &impl std::fmt::Debug, target: &'static str, field: &'static str, token: &'static str) {
    use std::fmt::Write as _;
    let mut writer = InnerFormatterPanic { target, field, value: token, entered: false, field_seen: false, reached: false };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| write!(&mut writer, "{value:?}").unwrap()));
    assert!(result.is_err());
    assert!(writer.entered && writer.field_seen && writer.reached, "must reach the inner field rather than an outer prefix");
}

#[tokio::test]
async fn panicking_public_custody_debug_does_not_poison_selected_reservation() {
    let directory = tempfile::tempdir().unwrap();
    let storage = RepositoryStorage::new(directory.path());
    let reservation = storage.reserve_edit().unwrap();
    panic_in_inner_debug(&reservation, "EditCustody", "worker", "Idle");
    // In v1, the inner worker field was formatted while the custody mutex was held.
    let command = reservation.attach("original context").unwrap();
    assert_eq!(command.cancel_before_effects().unwrap(), "original context");
    assert_eq!(storage.owned_edits.snapshot(), (false, 0, false));
}

#[tokio::test]
async fn panicking_public_storage_debug_does_not_poison_registry_admission() {
    let directory = tempfile::tempdir().unwrap();
    let storage = RepositoryStorage::new(directory.path());
    panic_in_inner_debug(&storage, "OwnedEditRegistry", "stopped", "false");
    // In v1, the inner stopped field was formatted while the registry mutex was held.
    storage.reserve_edit().unwrap().attach(()).unwrap().cancel_before_effects().unwrap();
    assert_eq!(storage.owned_edits.snapshot(), (false, 0, false));
}

#[tokio::test]
async fn send_only_context_moves_through_actual_worker_without_requiring_sync() {
    let directory = tempfile::tempdir().unwrap();
    let storage = RepositoryStorage::new(directory.path());
    // Cell is Send but not Sync: the public keepalive contract must not add Sync.
    let mut command = edit(&storage, std::cell::Cell::new(17_u8));
    command.context_mut().set(23);
    let task = tokio::spawn(command.create(FROM, "main".into()));
    let (mut command, operation) = returned(timeout(WAIT, task).await.unwrap().unwrap());
    assert_eq!(operation.unwrap(), GitEditOutput::Created(storage.path_for(FROM, REPO)));
    assert_eq!(command.context_mut().get(), 23);
    let value = command.finish().unwrap();
    assert_eq!(value.get(), 23);
    assert_eq!(std::fs::read_to_string(storage.path_for(FROM, REPO).join("HEAD")).unwrap(), "ref: refs/heads/main\n");
    storage.prepare_owned_shutdown();
    storage.drain_owned(Instant::now() + WAIT).await.unwrap();
}

// No SQL fixture: these cases prove Git context/credit behavior only. The named
// terminal's SQL-boundary assertion is a trusted caller prerequisite, not test data.
#[tokio::test]
async fn unbound_no_backend_terminal_returns_same_context_and_reuses_more_than_two_credits() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("never-created");
    let storage = RepositoryStorage::new(&root);
    let weak = Arc::downgrade(&storage.owned_edits);
    for _ in 0..4 {
        let (value, drops) = context();
        let value = Box::new(value);
        let pointer = std::ptr::from_ref(&*value);
        let command = storage.clone().reserve_edit().unwrap().attach(value).unwrap();
        assert_eq!(storage.owned_edits.snapshot(), (false, 1, false));
        let returned = command.finish_without_backend().unwrap();
        assert_eq!(std::ptr::from_ref(&*returned), pointer);
        assert_eq!(drops.load(Ordering::SeqCst), 0);
        assert_eq!(storage.owned_edits.snapshot(), (false, 0, false));
        assert!(!root.exists(), "terminal must not initialize or edit any repository");
        drop(returned);
        assert_eq!(drops.load(Ordering::SeqCst), 1);
    }
    drop(storage);
    assert!(weak.upgrade().is_none(), "successful terminal leaves no registry cycle");
}

#[tokio::test]
async fn unbound_no_backend_terminal_after_stop_returns_context_without_reopening_admission() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("never-created");
    let storage = RepositoryStorage::new(&root);
    let attached = storage.reserve_edit().unwrap().attach("same context").unwrap();
    let stale = storage.clone().reserve_edit().unwrap();
    storage.prepare_owned_shutdown();
    assert_eq!(attached.finish_without_backend().unwrap(), "same context");
    let refused = stale.attach("stale context").unwrap_err();
    assert_eq!(refused.reason, GitAdmissionError::Stopped);
    assert_eq!(refused.context, "stale context");
    assert_eq!(storage.reserve_edit().unwrap_err(), GitAdmissionError::Stopped);
    assert_eq!(storage.owned_edits.snapshot(), (true, 0, false));
    storage.drain_owned(Instant::now() + WAIT).await.unwrap();
    assert!(!root.exists());
}

#[tokio::test]
async fn unbound_no_backend_terminal_keeps_explicit_unknown_credit_and_recoverable_context() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("never-created");
    let storage = RepositoryStorage::new(&root);
    let weak = Arc::downgrade(&storage.owned_edits);
    let (value, drops) = context();
    let command = storage.reserve_edit().unwrap().attach(value).unwrap();
    command.core.custody.unknown(WorkerLoss::OwnerLost);
    let Err(retained) = command.finish_without_backend() else { panic!("unknown custody must never return K as acknowledged") };
    assert_eq!(storage.owned_edits.snapshot(), (false, 1, true));
    let spare = storage.reserve_edit().unwrap();
    assert_eq!(storage.reserve_edit().unwrap_err(), GitAdmissionError::Capacity);
    drop(spare);
    storage.prepare_owned_shutdown();
    assert_eq!(storage.drain_owned(Instant::now() + WAIT).await, Err(OwnedGitDrainError::Unconfirmed));
    assert_eq!(drops.load(Ordering::SeqCst), 0, "recoverable explicit quarantine retains K; no panic-survival claim");
    assert!(!root.exists());
    drop(retained);
    drop(storage);
    assert!(weak.upgrade().is_some(), "restart-only unknown anchor survives last public handle");
    assert_eq!(drops.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn unbound_no_backend_terminal_accepts_send_only_context_and_moves_returned_value() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("never-created");
    let storage = RepositoryStorage::new(&root);
    let mut command = storage.reserve_edit().unwrap().attach(std::cell::Cell::new(17_u8)).unwrap();
    command.context_mut().set(23);
    // Cell is Send but not Sync. This is an owned Rust task, not a Git IO worker.
    let task = tokio::spawn(async move { command.finish_without_backend().unwrap() });
    let returned = timeout(WAIT, task).await.unwrap().unwrap();
    assert_eq!(returned.get(), 23);
    assert_eq!(storage.owned_edits.snapshot(), (false, 0, false));
    assert!(!root.exists());
    storage.prepare_owned_shutdown();
    storage.drain_owned(Instant::now() + WAIT).await.unwrap();
}
