//! The admission decision precedes one-use effects and survives worker handoff.

use std::{
    num::NonZeroUsize,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use rbit_kernel::{AdmissionError, BlockingBudget};
use tokio::sync::oneshot;

fn budget() -> BlockingBudget {
    BlockingBudget::new(NonZeroUsize::new(1).unwrap())
}

#[test]
fn reservation_saturates_and_drop_releases_without_a_runtime() {
    let budget = budget();
    let admitted = budget.try_reserve().unwrap();
    assert_eq!(budget.try_reserve().unwrap_err(), AdmissionError::Busy);
    assert_eq!(budget.available(), 0);
    drop(admitted);
    assert_eq!(budget.available(), 1);
    drop(budget.try_reserve().unwrap());
}

#[tokio::test]
async fn cancelling_before_attempt_effects_returns_unsubmitted_capacity() {
    let budget = budget();
    let consumed = Arc::new(AtomicBool::new(false));
    let (admitted, ready) = oneshot::channel();
    let (_resume, continue_request) = oneshot::channel::<()>();
    let task_budget = budget.clone();
    let task_consumed = Arc::clone(&consumed);
    let request = tokio::spawn(async move {
        let reservation = task_budget.try_reserve().unwrap();
        admitted.send(()).unwrap();
        continue_request.await.unwrap();
        // The application must place one-use effects after successful admission.
        task_consumed.store(true, Ordering::SeqCst);
        reservation.spawn(|| ()).await.unwrap();
    });
    ready.await.unwrap();
    assert_eq!(budget.available(), 0);
    request.abort();
    assert!(request.await.unwrap_err().is_cancelled());
    assert!(!consumed.load(Ordering::SeqCst));
    assert_eq!(budget.available(), 1);
}

#[tokio::test]
async fn already_admitted_reservation_can_finish_after_budget_close() {
    let budget = budget();
    let reservation = budget.try_reserve().unwrap();
    budget.close();
    assert_eq!(budget.try_reserve().unwrap_err(), AdmissionError::Closed);
    assert_eq!(reservation.spawn(|| 42).await.unwrap(), 42);
    assert_eq!(budget.available(), 1);
    assert_eq!(budget.try_reserve().unwrap_err(), AdmissionError::Closed);
}

#[test]
fn failed_spawn_outside_runtime_unwinds_and_releases_the_reservation() {
    let budget = budget();
    let reservation = budget.try_reserve().unwrap();
    let failed = catch_unwind(AssertUnwindSafe(|| reservation.spawn(|| ())));
    assert!(failed.is_err());
    assert_eq!(budget.available(), 1);
}
