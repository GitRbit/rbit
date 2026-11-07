//! Cancellation, saturation and shutdown contracts for real blocking workers.

use std::{num::NonZeroUsize, sync::mpsc, time::Duration};

use rbit_kernel::{AdmissionError, BlockingBudget};
use tokio::sync::oneshot;

fn budget(capacity: usize) -> BlockingBudget {
    BlockingBudget::new(NonZeroUsize::new(capacity).unwrap())
}

async fn wait_for_capacity(budget: &BlockingBudget) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while budget.available() != budget.capacity().get() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("worker must release capacity after finishing");
}

#[tokio::test]
async fn saturation_is_immediate_and_does_not_run_rejected_work() {
    let admission = budget(1);
    let (started_tx, started_rx) = oneshot::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let worker = admission
        .try_spawn(move || {
            started_tx.send(()).unwrap();
            release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            42
        })
        .unwrap();
    started_rx.await.unwrap();
    assert_eq!(admission.available(), 0);
    let rejection = admission.clone().try_spawn(|| panic!("rejected work must not run")).unwrap_err();
    assert_eq!(rejection, AdmissionError::Busy);
    release_tx.send(()).unwrap();
    assert_eq!(worker.await.unwrap(), 42);
    assert_eq!(admission.available(), 1);
}

#[tokio::test]
async fn cancelling_request_keeps_running_worker_counted() {
    let admission = budget(1);
    let (started_tx, started_rx) = oneshot::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let worker_budget = admission.clone();
    let request = tokio::spawn(async move {
        worker_budget
            .try_spawn(move || {
                started_tx.send(()).unwrap();
                release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            })
            .unwrap()
            .await
            .unwrap();
    });
    started_rx.await.unwrap();
    request.abort();
    assert!(request.await.unwrap_err().is_cancelled());
    assert_eq!(admission.try_spawn(|| ()).unwrap_err(), AdmissionError::Busy);
    release_tx.send(()).unwrap();
    wait_for_capacity(&admission).await;
    admission.try_spawn(|| ()).unwrap().await.unwrap();
}

#[tokio::test]
async fn response_deadline_does_not_release_unfinished_worker() {
    let admission = budget(1);
    let (started_tx, started_rx) = oneshot::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let worker = admission
        .try_spawn(move || {
            started_tx.send(()).unwrap();
            release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        })
        .unwrap();
    started_rx.await.unwrap();
    let deadline = tokio::time::Instant::now();
    assert!(tokio::time::timeout_at(deadline, worker).await.is_err());
    assert_eq!(admission.try_spawn(|| ()).unwrap_err(), AdmissionError::Busy);
    release_tx.send(()).unwrap();
    wait_for_capacity(&admission).await;
}

#[tokio::test]
async fn panicking_worker_releases_capacity_and_preserves_join_error() {
    let admission = budget(1);
    let worker = admission.try_spawn(|| panic!("simulated blocking failure")).unwrap();
    assert!(worker.await.unwrap_err().is_panic());
    assert_eq!(admission.available(), 1);
    assert_eq!(admission.try_spawn(|| 7).unwrap().await.unwrap(), 7);
}

#[tokio::test]
async fn closed_budget_rejects_new_work_but_allows_existing_work_to_finish() {
    let admission = budget(1);
    let (started_tx, started_rx) = oneshot::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let worker = admission
        .try_spawn(move || {
            started_tx.send(()).unwrap();
            release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        })
        .unwrap();
    started_rx.await.unwrap();
    admission.clone().close();
    assert!(admission.is_closed());
    assert_eq!(admission.try_spawn(|| ()).unwrap_err(), AdmissionError::Closed);
    release_tx.send(()).unwrap();
    worker.await.unwrap();
    assert_eq!(admission.available(), 1);
    assert_eq!(admission.try_spawn(|| ()).unwrap_err(), AdmissionError::Closed);
}

#[test]
fn aborting_queued_work_releases_capacity_without_running_it() {
    let runtime = tokio::runtime::Builder::new_current_thread().max_blocking_threads(1).build().unwrap();
    runtime.block_on(async {
        let admission = budget(1);
        let (started_tx, started_rx) = oneshot::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let occupied_pool = tokio::task::spawn_blocking(move || {
            started_tx.send(()).unwrap();
            release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        });
        started_rx.await.unwrap();
        let queued = admission.try_spawn(|| panic!("aborted queued work must not run")).unwrap();
        assert_eq!(admission.available(), 0);
        queued.abort();
        release_tx.send(()).unwrap();
        occupied_pool.await.unwrap();
        assert!(queued.await.unwrap_err().is_cancelled());
        assert_eq!(admission.available(), 1);
    });
}
