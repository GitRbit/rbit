//! Cancelling a submitted password operation cannot free its queued worker's capacity.

use std::{
    future::{Future, poll_fn},
    num::NonZeroUsize,
    sync::mpsc,
    task::Poll,
    time::Duration,
};

use rbit_identity::PasswordWork;
use rbit_kernel::{AdmissionError, BlockingBudget};
use tokio::sync::oneshot;

#[test]
fn abandoned_queued_password_work_keeps_capacity_until_actual_completion() {
    let runtime = tokio::runtime::Builder::new_current_thread().max_blocking_threads(1).enable_all().build().unwrap();
    runtime.block_on(async {
        let budget = BlockingBudget::new(NonZeroUsize::new(1).unwrap());
        let work = PasswordWork::new(budget.clone());
        let (started, ready) = oneshot::channel();
        let (release, proceed) = mpsc::channel();
        let occupied_pool = tokio::task::spawn_blocking(move || {
            started.send(()).unwrap();
            proceed.recv_timeout(Duration::from_secs(10)).unwrap();
        });
        ready.await.unwrap();

        let mut request = Box::pin(work.begin().unwrap().hash("detached password worker".into()));
        poll_fn(|context| {
            assert!(request.as_mut().poll(context).is_pending());
            Poll::Ready(())
        })
        .await;
        drop(request);
        assert_eq!(work.begin().unwrap_err(), AdmissionError::Busy);

        release.send(()).unwrap();
        occupied_pool.await.unwrap();
        tokio::time::timeout(Duration::from_secs(10), async {
            while budget.available() == 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        drop(work.begin().unwrap());
    });
}
