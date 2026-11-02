//! Pure admission/counter tests, not SQL, IO or worker settlement evidence.
use super::*;
use std::{
    future::{Future, poll_fn},
    pin::Pin,
    sync::{
        TryLockError,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    task::{Context, Poll, Wake, Waker},
    time::Duration,
};
use tokio::time::timeout;

const WAIT: Duration = Duration::from_secs(2);
async fn pending<F: Future>(mut future: Pin<&mut F>) {
    poll_fn(|cx| {
        assert!(future.as_mut().poll(cx).is_pending(), "expected actual pending admission/wait");
        Poll::Ready(())
    })
    .await;
}
async fn saturate(writer: &WriterRegistration) -> Vec<SuspendedMutation> {
    let mut operations = Vec::new();
    for _ in 0..MAX_MUTATIONS {
        operations.push(writer.begin().await.unwrap().suspend());
    }
    operations
}
async fn rejected(writer: &WriterRegistration) -> MutationCapacityWait {
    match writer.begin_observed().await {
        Err(MutationAdmissionFailure::Capacity { wait }) => wait,
        other => panic!("expected exact capacity rejection, got {other:?}"),
    }
}
async fn settle(operations: Vec<SuspendedMutation>) {
    for operation in operations {
        timeout(WAIT, operation.resume()).await.unwrap().complete();
    }
}

#[tokio::test]
async fn exact_rejection_observes_release_before_changed_future_construction() {
    let domain = Domain::new();
    let writer = domain.writer();
    let mut operations = saturate(&writer).await;
    let wait = rejected(&writer).await;
    assert_eq!(domain.snapshot().active, MAX_MUTATIONS);
    operations.pop().unwrap().resume().await.complete();
    assert_eq!(timeout(WAIT, wait.changed()).await.unwrap(), Ok(()));
    let admitted = writer.begin_observed().await.unwrap();
    assert_eq!(domain.snapshot().active, MAX_MUTATIONS);
    admitted.complete();
    settle(operations).await;
    assert_eq!(domain.snapshot(), Snapshot { active: 0, queued: 0, uncertain: false, writers_stopped: false });
}

#[tokio::test]
async fn enabled_wait_observes_real_release_without_registering_itself() {
    let domain = Domain::new();
    let writer = domain.writer();
    let mut operations = saturate(&writer).await;
    let mut changed = Box::pin(rejected(&writer).await.changed());
    pending(changed.as_mut()).await;
    assert_eq!(domain.snapshot().queued, 0);
    operations.pop().unwrap().resume().await.complete();
    assert_eq!(timeout(WAIT, changed).await.unwrap(), Ok(()));
    settle(operations).await;
}

#[tokio::test]
async fn unchanged_capacity_suspend_notifications_and_other_source_release_stay_pending() {
    let domain = Domain::new();
    let writer = domain.writer();
    let mut operations = saturate(&writer).await;
    let mut changed = Box::pin(rejected(&writer).await.changed());
    pending(changed.as_mut()).await;
    let resumed = operations.pop().unwrap().resume().await;
    operations.push(resumed.suspend());
    // Exercise the existing shared Notify without inventing a capacity change.
    domain.inner.changed.notify_waiters();
    pending(changed.as_mut()).await;
    let other = Domain::new();
    other.writer().begin().await.unwrap().complete();
    pending(changed.as_mut()).await;
    assert_eq!(domain.snapshot().active, MAX_MUTATIONS);
    operations.pop().unwrap().resume().await.complete();
    assert_eq!(timeout(WAIT, changed).await.unwrap(), Ok(()));
    settle(operations).await;
}

#[tokio::test]
async fn waiter_drop_changes_no_counts_and_stop_is_terminal_for_enabled_wait() {
    let domain = Domain::new();
    let writer = domain.writer();
    let operations = saturate(&writer).await;
    drop(rejected(&writer).await);
    let mut abandoned = Box::pin(rejected(&writer).await.changed());
    pending(abandoned.as_mut()).await;
    drop(abandoned);
    assert_eq!(domain.snapshot(), Snapshot { active: MAX_MUTATIONS, queued: 0, uncertain: false, writers_stopped: false });
    let mut changed = Box::pin(rejected(&writer).await.changed());
    pending(changed.as_mut()).await;
    let shutdown = domain.stop_writers();
    assert_eq!(timeout(WAIT, changed).await.unwrap(), Err(AdmissionWatchError::Stopped));
    assert!(matches!(writer.begin_observed().await, Err(MutationAdmissionFailure::Stopped)));
    assert!(matches!(writer.begin().await, Err(BeginError::Stopped)));
    settle(operations).await;
    drop(shutdown.drain(Instant::now() + WAIT).await.unwrap());
}

#[tokio::test]
async fn observed_queued_cancellation_releases_exactly_one_clean_ticket() {
    let domain = Domain::new();
    let writer = domain.writer();
    let held = writer.begin().await.unwrap();
    let revision = domain.inner.state().admission_revision;
    let mut queued = Box::pin(writer.begin_observed());
    pending(queued.as_mut()).await;
    assert_eq!(domain.snapshot(), Snapshot { active: 1, queued: 1, uncertain: false, writers_stopped: false });
    assert_eq!(domain.inner.state().admission_revision, revision);
    drop(queued);
    assert_eq!(domain.snapshot(), Snapshot { active: 1, queued: 0, uncertain: false, writers_stopped: false });
    assert_eq!(domain.inner.state().admission_revision, revision.map(|value| value + 1));
    held.complete();
    let acquired = writer.begin_observed().await.unwrap();
    drop(acquired);
    assert_eq!(domain.snapshot(), Snapshot { active: 0, queued: 0, uncertain: true, writers_stopped: false });
    writer.begin_observed().await.unwrap().complete();
    assert!(domain.snapshot().uncertain, "later success never clears actual acquired loss");
}

#[tokio::test]
async fn observed_queued_stop_refuses_before_active_and_drops_ticket_cleanly() {
    let domain = Domain::new();
    let writer = domain.writer();
    let held = writer.begin().await.unwrap();
    let mut queued = Box::pin(writer.begin_observed());
    pending(queued.as_mut()).await;
    let shutdown = domain.stop_writers();
    held.complete();
    assert!(matches!(timeout(WAIT, queued).await.unwrap(), Err(MutationAdmissionFailure::Stopped)));
    assert_eq!(domain.snapshot(), Snapshot { active: 0, queued: 0, uncertain: false, writers_stopped: true });
    drop(shutdown.drain(Instant::now() + WAIT).await.unwrap());
}

#[tokio::test]
async fn injected_revision_exhaustion_only_disables_new_observed_subsystem() {
    let domain = Domain::new();
    let writer = domain.writer();
    let mut operations = saturate(&writer).await;
    // Inject only the otherwise unreachable metadata edge, not capacity/IO evidence.
    domain.inner.state().admission_revision = Some(u64::MAX);
    let mut changed = Box::pin(rejected(&writer).await.changed());
    pending(changed.as_mut()).await;
    operations.pop().unwrap().resume().await.complete();
    assert_eq!(timeout(WAIT, changed).await.unwrap(), Err(AdmissionWatchError::ObserverUnavailable));
    assert!(matches!(writer.begin_observed().await, Err(MutationAdmissionFailure::ObserverUnavailable)));
    assert!(!domain.snapshot().writers_stopped);
    writer.begin().await.unwrap().complete();
    settle(operations).await;
    assert_eq!(domain.snapshot(), Snapshot { active: 0, queued: 0, uncertain: false, writers_stopped: false });
    drop(domain.drain(Instant::now() + WAIT).await.unwrap());
}

#[derive(Debug)]
struct LockProbe {
    inner: Arc<Inner>,
    wakes: AtomicUsize,
    state_held: AtomicBool,
}
impl LockProbe {
    fn observe(&self) {
        self.wakes.fetch_add(1, Ordering::SeqCst);
        match self.inner.state.try_lock() {
            Ok(_) | Err(TryLockError::Poisoned(_)) => {}
            Err(TryLockError::WouldBlock) => {
                self.state_held.store(true, Ordering::SeqCst);
            }
        }
    }
}
impl Wake for LockProbe {
    fn wake(self: Arc<Self>) {
        self.observe();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.observe();
    }
}

#[tokio::test]
async fn capacity_release_and_stop_wake_outside_state_mutex() {
    for stop in [false, true] {
        let domain = Domain::new();
        let writer = domain.writer();
        let mut operations = saturate(&writer).await;
        let mut changed = Box::pin(rejected(&writer).await.changed());
        let probe = Arc::new(LockProbe { inner: domain.inner.clone(), wakes: AtomicUsize::new(0), state_held: AtomicBool::new(false) });
        let waker = Waker::from(probe.clone());
        assert!(changed.as_mut().poll(&mut Context::from_waker(&waker)).is_pending());
        if stop {
            drop(domain.stop_writers());
        } else {
            operations.pop().unwrap().resume().await.complete();
        }
        assert!(probe.wakes.load(Ordering::SeqCst) > 0, "actual registered Notify waiter must be woken");
        assert!(!probe.state_held.load(Ordering::SeqCst), "state mutex must be released before invoking waker");
        assert_eq!(timeout(WAIT, changed).await.unwrap(), if stop { Err(AdmissionWatchError::Stopped) } else { Ok(()) });
        settle(operations).await;
    }
}

#[tokio::test]
async fn canceled_queued_ticket_wakes_capacity_observer_outside_state_mutex() {
    let domain = Domain::new();
    let writer = domain.writer();
    let mut operations = saturate(&writer).await;
    operations.pop().unwrap().resume().await.complete();
    let held = operations.pop().unwrap().resume().await;
    let mut queued = Box::pin(writer.begin_observed());
    pending(queued.as_mut()).await;
    assert_eq!(domain.snapshot().active + domain.snapshot().queued, MAX_MUTATIONS);
    let mut changed = Box::pin(rejected(&writer).await.changed());
    let probe = Arc::new(LockProbe { inner: domain.inner.clone(), wakes: AtomicUsize::new(0), state_held: AtomicBool::new(false) });
    let waker = Waker::from(probe.clone());
    assert!(changed.as_mut().poll(&mut Context::from_waker(&waker)).is_pending());
    drop(queued);
    assert!(probe.wakes.load(Ordering::SeqCst) > 0);
    assert!(!probe.state_held.load(Ordering::SeqCst));
    assert_eq!(timeout(WAIT, changed).await.unwrap(), Ok(()));
    assert_eq!(domain.snapshot().queued, 0);
    assert!(!domain.snapshot().uncertain);
    held.complete();
    settle(operations).await;
}

// An already registered ticket is a continuation even when ONLY observer metadata
// exhausts. This injection is not SQL/worker evidence or a real u64-scale workload.
#[tokio::test]
async fn registered_observed_ticket_survives_observer_exhaustion_without_fresh_check() {
    let domain = Domain::new();
    let writer = domain.writer();
    let held = writer.begin().await.unwrap();
    let mut queued = Box::pin(writer.begin_observed());
    pending(queued.as_mut()).await;
    assert_eq!(domain.snapshot(), Snapshot { active: 1, queued: 1, uncertain: false, writers_stopped: false });
    domain.inner.state().admission_revision = Some(u64::MAX);
    held.complete();
    let acquired = timeout(WAIT, queued).await.unwrap().unwrap();
    assert_eq!(domain.inner.state().admission_revision, None);
    assert_eq!(domain.snapshot(), Snapshot { active: 1, queued: 0, uncertain: false, writers_stopped: false });
    assert!(matches!(writer.begin_observed().await, Err(MutationAdmissionFailure::ObserverUnavailable)));
    acquired.complete();
    writer.begin().await.unwrap().complete();
    assert_eq!(domain.snapshot(), Snapshot { active: 0, queued: 0, uncertain: false, writers_stopped: false });
    assert_eq!(domain.inner.state().admission_revision, None);
}
