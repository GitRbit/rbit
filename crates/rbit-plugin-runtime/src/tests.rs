//! Admission and disposal races, cancellation and incomplete cleanup.

use std::{future::pending, sync::Arc, time::Duration};

use rbit_plugin_api::Generation;
use tokio::sync::oneshot;

use crate::{Admission, AdmissionError, DrainError, PluginScope};

fn generation(value: u64) -> Generation {
    Generation::new(value).unwrap()
}

#[test]
fn stale_generations_never_enter() {
    let admission = Admission::new(generation(2));
    assert!(matches!(
        admission.try_acquire(generation(1)),
        Err(AdmissionError::GenerationMismatch { requested, current }) if requested == generation(1) && current == generation(2)
    ));
    assert_eq!(admission.in_flight(), 0);
    let lease = admission.try_acquire(generation(2)).unwrap();
    assert_eq!(lease.generation(), generation(2));
    assert_eq!(admission.in_flight(), 1);
    drop(lease);
    assert_eq!(admission.in_flight(), 0);
}

#[test]
fn closing_preserves_accepted_work_and_permanently_refuses_new_work() {
    let admission = Admission::new(generation(1));
    let first = admission.try_acquire(generation(1)).unwrap();
    let second = admission.try_acquire(generation(1)).unwrap();
    admission.close();
    admission.close();
    assert!(admission.is_closed());
    assert!(matches!(admission.try_acquire(generation(1)), Err(AdmissionError::Closed)));
    assert_eq!(admission.in_flight(), 2);
    drop(first);
    assert_eq!(admission.in_flight(), 1);
    drop(second);
    assert_eq!(admission.in_flight(), 0);
}

#[test]
fn acquire_and_close_have_a_single_ordering() {
    for _ in 0..64 {
        let admission = Admission::new(generation(1));
        let barrier = Arc::new(std::sync::Barrier::new(3));
        let acquiring_gate = admission.clone();
        let acquiring_barrier = Arc::clone(&barrier);
        let acquiring = std::thread::spawn(move || {
            acquiring_barrier.wait();
            acquiring_gate.try_acquire(generation(1))
        });
        let closing_gate = admission.clone();
        let closing_barrier = Arc::clone(&barrier);
        let closing = std::thread::spawn(move || {
            closing_barrier.wait();
            closing_gate.close();
        });
        barrier.wait();
        closing.join().unwrap();
        let accepted = acquiring.join().unwrap();
        assert!(admission.is_closed());
        assert_eq!(admission.in_flight(), usize::from(accepted.is_ok()));
        assert!(matches!(admission.try_acquire(generation(1)), Err(AdmissionError::Closed)));
        drop(accepted);
        assert_eq!(admission.in_flight(), 0);
    }
}

#[tokio::test]
async fn open_admission_cannot_be_reported_as_drained() {
    let admission = Admission::new(generation(1));
    assert_eq!(admission.wait_for_drain(Duration::from_secs(1)).await, Err(DrainError::Open));
}

#[tokio::test]
async fn timed_out_drain_stays_closed_and_can_be_retried() {
    let admission = Admission::new(generation(1));
    let lease = admission.try_acquire(generation(1)).unwrap();
    admission.close();
    assert_eq!(admission.wait_for_drain(Duration::ZERO).await, Err(DrainError::Timeout { in_flight: 1 }));
    assert!(admission.is_closed());
    drop(lease);
    admission.wait_for_drain(Duration::ZERO).await.unwrap();
}

#[tokio::test]
async fn final_lease_wakes_all_drain_waiters() {
    let admission = Admission::new(generation(1));
    let lease = admission.try_acquire(generation(1)).unwrap();
    admission.close();
    let mut waiters = Vec::new();
    for _ in 0..8 {
        let admission = admission.clone();
        waiters.push(tokio::spawn(async move { admission.wait_for_drain(Duration::from_secs(1)).await }));
    }
    tokio::task::yield_now().await;
    drop(lease);
    for waiter in waiters {
        waiter.await.unwrap().unwrap();
    }
}

#[tokio::test]
async fn disposal_waits_for_cooperative_cleanup_and_is_idempotent() {
    let scope = PluginScope::new(generation(1));
    let (started, ready) = oneshot::channel();
    let (finish, cleanup) = oneshot::channel();
    let handle = scope
        .spawn(|cancel| async move {
            started.send(()).unwrap();
            cancel.cancelled().await;
            cleanup.await.unwrap();
            42
        })
        .unwrap();
    ready.await.unwrap();
    assert_eq!(scope.dispose(Duration::ZERO).await, Err(DrainError::Timeout { in_flight: 1 }));
    assert!(scope.is_closed());
    assert!(matches!(scope.spawn(|_| async {}), Err(AdmissionError::Closed)));
    finish.send(()).unwrap();
    scope.dispose(Duration::from_secs(1)).await.unwrap();
    assert_eq!(handle.await.unwrap(), 42);
    scope.dispose(Duration::ZERO).await.unwrap();
    assert_eq!(scope.in_flight(), 0);
}

#[tokio::test]
async fn dropping_join_handle_does_not_forget_the_task() {
    let scope = PluginScope::new(generation(1));
    let (finish, cleanup) = oneshot::channel();
    let handle = scope.spawn(|_| async move { cleanup.await.unwrap() }).unwrap();
    drop(handle);
    assert_eq!(scope.dispose(Duration::ZERO).await, Err(DrainError::Timeout { in_flight: 1 }));
    finish.send(()).unwrap();
    scope.dispose(Duration::from_secs(1)).await.unwrap();
}

#[tokio::test]
async fn aborted_task_releases_its_lease() {
    let scope = PluginScope::new(generation(1));
    let handle = scope.spawn(|_| pending::<()>()).unwrap();
    handle.abort();
    assert!(handle.await.unwrap_err().is_cancelled());
    scope.dispose(Duration::ZERO).await.unwrap();
    assert_eq!(scope.in_flight(), 0);
}

#[tokio::test]
async fn panicked_task_releases_its_lease() {
    let scope = PluginScope::new(generation(1));
    let handle = scope.spawn(|_| async { panic!("failing plugin task") }).unwrap();
    assert!(handle.await.unwrap_err().is_panic());
    scope.dispose(Duration::ZERO).await.unwrap();
    assert_eq!(scope.in_flight(), 0);
}

#[tokio::test]
async fn last_scope_drop_closes_existing_handles_and_signals_cancellation() {
    let scope = PluginScope::new(generation(1));
    let handle = scope.spawn(|cancel| async move { cancel.cancelled().await }).unwrap();
    let clone = scope.clone();
    drop(scope);
    assert!(!clone.is_closed());
    drop(clone);
    tokio::time::timeout(Duration::from_secs(1), handle).await.unwrap().unwrap();
}

#[tokio::test]
async fn disposal_waits_for_external_response_body_lease() {
    let scope = PluginScope::new(generation(1));
    let response_body_lease = scope.try_acquire(generation(1)).unwrap();
    assert_eq!(scope.dispose(Duration::ZERO).await, Err(DrainError::Timeout { in_flight: 1 }));
    drop(response_body_lease);
    scope.dispose(Duration::ZERO).await.unwrap();
}

#[tokio::test]
async fn abandoning_disposal_does_not_reopen_the_generation() {
    let scope = PluginScope::new(generation(1));
    let (cancelled, cancellation_seen) = oneshot::channel();
    let (finish, cleanup) = oneshot::channel();
    let task = scope
        .spawn(|cancel| async move {
            cancel.cancelled().await;
            cancelled.send(()).unwrap();
            cleanup.await.unwrap();
        })
        .unwrap();
    let disposing_scope = scope.clone();
    let disposing = tokio::spawn(async move { disposing_scope.dispose(Duration::from_secs(60)).await });
    cancellation_seen.await.unwrap();
    disposing.abort();
    assert!(disposing.await.unwrap_err().is_cancelled());
    assert!(scope.is_closed());
    assert_eq!(scope.in_flight(), 1);
    assert!(matches!(scope.try_acquire(generation(1)), Err(AdmissionError::Closed)));
    finish.send(()).unwrap();
    scope.dispose(Duration::from_secs(1)).await.unwrap();
    task.await.unwrap();
}
