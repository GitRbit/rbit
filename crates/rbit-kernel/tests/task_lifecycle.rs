//! Ownership and health remain truthful when waiters disappear or shutdown races start.

use rbit_kernel::{TaskLifecycle, TaskState};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[tokio::test]
async fn clones_start_only_one_worker_and_stopped_owners_cannot_restart() {
    let owner = TaskLifecycle::new();
    let started = Arc::new(AtomicUsize::new(0));
    let mut contenders = Vec::new();
    for _ in 0..16 {
        let contender = owner.clone();
        let stop = contender.stop_signal();
        let started = Arc::clone(&started);
        contenders.push(tokio::spawn(async move {
            contender.start(async move {
                started.fetch_add(1, Ordering::SeqCst);
                stop.cancelled().await;
            })
        }));
    }
    let mut accepted = 0;
    for contender in contenders {
        accepted += usize::from(contender.await.unwrap());
    }
    assert_eq!(accepted, 1);
    owner.shutdown().await;
    assert_eq!(started.load(Ordering::SeqCst), 1);
    assert_eq!(owner.state(), TaskState::Stopped);
    assert!(!owner.start(async {}));
}

#[tokio::test]
async fn cancelling_a_drain_wait_does_not_drop_work_or_claim_it_stopped() {
    let owner = TaskLifecycle::new();
    let stop = owner.stop_signal();
    let (entered, started) = tokio::sync::oneshot::channel();
    let (observed, stopped) = tokio::sync::oneshot::channel();
    let (release, resume) = tokio::sync::oneshot::channel();
    assert!(owner.start(async move {
        entered.send(()).unwrap();
        stop.cancelled().await;
        observed.send(()).unwrap();
        resume.await.unwrap();
    }));
    started.await.unwrap();
    let waiting = owner.clone();
    let waiter = tokio::spawn(async move {
        waiting.shutdown().await;
    });
    stopped.await.unwrap();
    waiter.abort();
    assert!(waiter.await.unwrap_err().is_cancelled());
    assert_eq!(owner.state(), TaskState::Stopping);
    // Deadline expiry is exactly another dropped wait, not task completion.
    assert!(tokio::time::timeout(std::time::Duration::ZERO, owner.shutdown()).await.is_err());
    assert_eq!(owner.state(), TaskState::Stopping);
    let second = owner.clone();
    let concurrent = tokio::spawn(async move {
        second.shutdown().await;
    });
    release.send(()).unwrap();
    owner.shutdown().await;
    concurrent.await.unwrap();
    assert_eq!(owner.state(), TaskState::Stopped);
}

#[tokio::test]
async fn panic_is_failed_health_after_a_successful_drain() {
    let owner = TaskLifecycle::new();
    assert!(owner.start(async {
        panic!("simulated subscriber failure");
    }));
    owner.shutdown().await;
    assert_eq!(owner.state(), TaskState::Failed);
    assert!(!owner.start(async {}));
}

#[tokio::test]
async fn stop_before_start_never_polls_the_rejected_future() {
    let owner = TaskLifecycle::new();
    owner.shutdown().await;
    assert!(!owner.start(async {
        panic!("a stopped owner must not poll this");
    }));
    assert_eq!(owner.state(), TaskState::Stopped);
    owner.stop_signal().cancelled().await;
}

#[tokio::test]
async fn dropping_last_owner_signals_cooperative_work_without_aborting_its_effects() {
    let owner = TaskLifecycle::new();
    let stop = owner.stop_signal();
    let (finished, completion) = tokio::sync::oneshot::channel();
    assert!(owner.start(async move {
        stop.cancelled().await;
        finished.send(()).unwrap();
    }));
    drop(owner);
    completion.await.unwrap();
}

#[test]
fn start_outside_runtime_panics_without_consuming_the_owner() {
    let owner = TaskLifecycle::new();
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| owner.start(async {}))).is_err());
    assert_eq!(owner.state(), TaskState::Idle);
}
