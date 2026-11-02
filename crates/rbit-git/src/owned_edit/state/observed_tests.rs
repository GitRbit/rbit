//! Source-only authored observer cases; private overflow metadata is not an IO fault.
use super::*;
use crate::{
    RepositoryStorage,
    owned_edit::{GitCapacityWait, GitFlowReservation},
};
use std::{
    future::{Future, poll_fn},
    panic::{AssertUnwindSafe, catch_unwind},
    pin::Pin,
    sync::{Weak, atomic::AtomicUsize},
    task::{Context, Poll, Wake, Waker},
    time::Duration,
};
use tokio::time::timeout;

const WAIT: Duration = Duration::from_secs(5);

fn full() -> (tempfile::TempDir, RepositoryStorage, GitFlowReservation, GitFlowReservation, GitCapacityWait) {
    let directory = tempfile::tempdir().unwrap();
    let storage = RepositoryStorage::new(directory.path());
    let first = storage.try_reserve_edit_observed().unwrap();
    let second = storage.reserve_edit().unwrap();
    let failure = storage.try_reserve_edit_observed().unwrap_err();
    assert_eq!(failure.reason, GitAdmissionError::Capacity);
    (directory, storage, first, second, failure.capacity.unwrap())
}
async fn pending<F: Future + ?Sized>(mut future: Pin<&mut F>) {
    poll_fn(|context| {
        assert!(future.as_mut().poll(context).is_pending());
        Poll::Ready(())
    })
    .await;
}

#[tokio::test]
async fn exact_refusal_observes_actual_release_before_changed_future_exists() {
    let (_directory, storage, first, second, wait) = full();
    let revision = storage.owned_edits.lock().capacity_revision;
    drop(first);
    assert_eq!(storage.owned_edits.lock().capacity_revision, revision.map(|value| value + 1));
    timeout(WAIT, wait.changed()).await.unwrap().unwrap();
    let replacement = storage.try_reserve_edit_observed().unwrap();
    assert_eq!(storage.owned_edits.snapshot(), (false, 2, false));
    drop((replacement, second));
    assert_eq!(storage.owned_edits.snapshot(), (false, 0, false));
}

#[tokio::test]
async fn enabled_wait_observes_clean_release_without_reserving_the_next_credit() {
    let (_directory, storage, first, second, wait) = full();
    let mut changed = Box::pin(wait.changed());
    pending(changed.as_mut()).await;
    drop(first);
    // A different caller is allowed to take the released credit before hint delivery.
    let replacement = storage.reserve_edit().unwrap();
    timeout(WAIT, changed).await.unwrap().unwrap();
    assert!(matches!(storage.reserve_edit(), Err(GitAdmissionError::Capacity)));
    drop((replacement, second));
}

#[tokio::test]
async fn unchanged_notifications_and_other_storage_releases_are_not_capacity_changes() {
    let (_directory, storage, first, second, wait) = full();
    let mut changed = Box::pin(wait.changed());
    pending(changed.as_mut()).await;
    let revision = storage.owned_edits.lock().capacity_revision;
    storage.owned_edits.changed.notify_waiters();
    pending(changed.as_mut()).await;
    assert_eq!(storage.owned_edits.lock().capacity_revision, revision);
    let other_directory = tempfile::tempdir().unwrap();
    let other = RepositoryStorage::new(other_directory.path());
    drop(other.reserve_edit().unwrap());
    pending(changed.as_mut()).await;
    drop(first);
    timeout(WAIT, changed).await.unwrap().unwrap();
    drop(second);
}

#[tokio::test]
async fn dropping_capacity_hint_changes_no_credit_or_revision() {
    let (_directory, storage, first, second, wait) = full();
    let revision = storage.owned_edits.lock().capacity_revision;
    drop(wait);
    assert_eq!(storage.owned_edits.lock().capacity_revision, revision);
    assert_eq!(storage.owned_edits.snapshot(), (false, 2, false));
    let failure = storage.try_reserve_edit_observed().unwrap_err();
    assert_eq!(failure.reason, GitAdmissionError::Capacity);
    assert!(failure.capacity.is_some());
    drop(failure);
    assert!(matches!(storage.reserve_edit(), Err(GitAdmissionError::Capacity)));
    drop((first, second));
}

#[tokio::test]
async fn first_stop_advances_once_and_is_terminal_for_hint_and_both_admissions() {
    let (_directory, storage, first, second, wait) = full();
    let revision = storage.owned_edits.lock().capacity_revision;
    let mut changed = Box::pin(wait.changed());
    pending(changed.as_mut()).await;
    storage.prepare_owned_shutdown();
    assert_eq!(storage.owned_edits.lock().capacity_revision, revision.map(|value| value + 1));
    let after_stop = storage.owned_edits.lock().capacity_revision;
    storage.prepare_owned_shutdown();
    assert_eq!(storage.owned_edits.lock().capacity_revision, after_stop);
    assert_eq!(timeout(WAIT, changed).await.unwrap(), Err(GitAdmissionError::Stopped));
    let failure = storage.try_reserve_edit_observed().unwrap_err();
    assert_eq!(failure.reason, GitAdmissionError::Stopped);
    assert!(failure.capacity.is_none());
    assert!(matches!(storage.reserve_edit(), Err(GitAdmissionError::Stopped)));
    drop((first, second));
}

#[tokio::test]
async fn checked_release_exhaustion_disables_only_observation_and_preserves_legacy_credit_use() {
    let (_directory, storage, first, second, initial_wait) = full();
    drop(initial_wait);
    storage.owned_edits.lock().capacity_revision = Some(u64::MAX);
    let wait = storage.try_reserve_edit_observed().unwrap_err().capacity.unwrap();
    drop(first); // Actual clean credit release overflows the injected private counter.
    assert_eq!(storage.owned_edits.lock().capacity_revision, None);
    assert_eq!(timeout(WAIT, wait.changed()).await.unwrap(), Err(GitAdmissionError::CoordinatorUnavailable));
    let failure = storage.try_reserve_edit_observed().unwrap_err();
    assert_eq!(failure.reason, GitAdmissionError::CoordinatorUnavailable);
    assert!(failure.capacity.is_none());
    let legacy = storage.reserve_edit().unwrap();
    let edit = legacy.attach(String::from("legacy after observer exhaustion")).unwrap();
    assert_eq!(edit.cancel_before_effects().unwrap(), "legacy after observer exhaustion");
    drop(second);
    storage.prepare_owned_shutdown();
    let failure = storage.try_reserve_edit_observed().unwrap_err();
    assert_eq!(failure.reason, GitAdmissionError::Stopped, "stop precedes observer exhaustion");
}

#[tokio::test]
async fn unknown_slot_and_unrelated_unknown_notifications_do_not_deny_spare_passive_credit() {
    let (_directory, storage, first, second, wait) = full();
    let mut changed = Box::pin(wait.changed());
    pending(changed.as_mut()).await;
    let retained = first.attach(String::from("retained unknown")).unwrap().quarantine();
    pending(changed.as_mut()).await;
    assert_eq!(storage.owned_edits.snapshot(), (false, 2, true));
    drop(second);
    timeout(WAIT, changed).await.unwrap().unwrap();
    let spare = storage.try_reserve_edit_observed().unwrap();
    assert_eq!(storage.owned_edits.snapshot(), (false, 2, true));
    drop((spare, retained)); // Explicit unknown slot remains anchored; no recovery API.
}

#[test]
fn runtime_unavailable_still_precedes_stopped_registry_for_observed_and_legacy() {
    let directory = tempfile::tempdir().unwrap();
    let storage = RepositoryStorage::new(directory.path());
    storage.prepare_owned_shutdown();
    let failure = storage.try_reserve_edit_observed().unwrap_err();
    assert_eq!(failure.reason, GitAdmissionError::RuntimeUnavailable);
    assert!(failure.capacity.is_none());
    assert!(matches!(storage.reserve_edit(), Err(GitAdmissionError::RuntimeUnavailable)));
}

#[tokio::test]
async fn poison_still_precedes_stopped_and_exhausted_observation() {
    let (_directory, storage, first, second, wait) = full();
    let poison = catch_unwind(AssertUnwindSafe(|| {
        let mut state = storage.owned_edits.state.lock().unwrap();
        state.capacity_revision = None;
        panic!("injected registry mutex poison");
    }));
    assert!(poison.is_err());
    let failure = storage.try_reserve_edit_observed().unwrap_err();
    assert_eq!(failure.reason, GitAdmissionError::CoordinatorUnavailable);
    assert!(failure.capacity.is_none());
    assert_eq!(wait.changed().await, Err(GitAdmissionError::CoordinatorUnavailable));
    assert!(matches!(storage.reserve_edit(), Err(GitAdmissionError::CoordinatorUnavailable)));
    drop((first, second));
}

struct LockProbe {
    registry: Weak<Registry>,
    custody: Weak<Custody>,
    wakes: AtomicUsize,
    blocked: AtomicUsize,
    cell_checks: AtomicUsize,
}
impl Wake for LockProbe {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.wakes.fetch_add(1, Ordering::SeqCst);
        if let Some(registry) = self.registry.upgrade()
            && matches!(registry.state.try_lock(), Err(std::sync::TryLockError::WouldBlock))
        {
            self.blocked.fetch_add(1, Ordering::SeqCst);
        }
        if let Some(custody) = self.custody.upgrade() {
            self.cell_checks.fetch_add(1, Ordering::SeqCst);
            if matches!(custody.state.try_lock(), Err(std::sync::TryLockError::WouldBlock)) {
                self.blocked.fetch_add(1, Ordering::SeqCst);
            }
        }
    }
}
fn probe(storage: &RepositoryStorage, custody: &Arc<Custody>) -> Arc<LockProbe> {
    Arc::new(LockProbe {
        registry: Arc::downgrade(&storage.owned_edits),
        custody: Arc::downgrade(custody),
        wakes: AtomicUsize::new(0),
        blocked: AtomicUsize::new(0),
        cell_checks: AtomicUsize::new(0),
    })
}

#[tokio::test]
async fn actual_credit_release_wakes_with_registry_and_released_cell_unlocked() {
    let (_directory, storage, first, second, wait) = full();
    let custody = first.custody.as_ref().unwrap().clone();
    let probe = probe(&storage, &custody);
    let waker = Waker::from(probe.clone());
    let mut context = Context::from_waker(&waker);
    let mut changed = Box::pin(wait.changed());
    assert!(changed.as_mut().poll(&mut context).is_pending());
    drop(first);
    assert!(probe.wakes.load(Ordering::SeqCst) > 0);
    assert!(probe.cell_checks.load(Ordering::SeqCst) > 0);
    assert_eq!(probe.blocked.load(Ordering::SeqCst), 0);
    timeout(WAIT, changed).await.unwrap().unwrap();
    drop((custody, second));
}

#[tokio::test]
async fn first_stop_wakes_with_registry_and_still_occupied_cell_unlocked() {
    let (_directory, storage, first, second, wait) = full();
    let custody = first.custody.as_ref().unwrap().clone();
    let probe = probe(&storage, &custody);
    let waker = Waker::from(probe.clone());
    let mut context = Context::from_waker(&waker);
    let mut changed = Box::pin(wait.changed());
    assert!(changed.as_mut().poll(&mut context).is_pending());
    storage.prepare_owned_shutdown();
    assert!(probe.wakes.load(Ordering::SeqCst) > 0);
    assert!(probe.cell_checks.load(Ordering::SeqCst) > 0);
    assert_eq!(probe.blocked.load(Ordering::SeqCst), 0);
    assert_eq!(timeout(WAIT, changed).await.unwrap(), Err(GitAdmissionError::Stopped));
    drop((custody, first, second));
}
