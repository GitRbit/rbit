//! Safe drop instrumentation verifies the same protected handoff used by public inputs.

use std::{
    num::NonZeroUsize,
    sync::{Arc, Mutex, mpsc},
    time::Duration,
};

use rbit_kernel::{AdmissionError, BlockingBudget};
use tokio::sync::oneshot;
use zeroize::{Zeroize, Zeroizing};

use crate::{PasswordError, PasswordWork};

#[derive(Debug)]
struct ObservedSecret {
    bytes: [u8; 8],
    events: Arc<Mutex<Vec<(&'static str, bool)>>>,
}

impl Zeroize for ObservedSecret {
    fn zeroize(&mut self) {
        self.bytes.zeroize();
        self.events.lock().unwrap().push(("zeroized", true));
    }
}

impl Drop for ObservedSecret {
    fn drop(&mut self) {
        self.events.lock().unwrap().push(("dropped", self.bytes.iter().all(|byte| *byte == 0)));
    }
}

fn work() -> (PasswordWork, BlockingBudget) {
    let budget = BlockingBudget::new(NonZeroUsize::new(1).unwrap());
    (PasswordWork::new(budget.clone()), budget)
}

#[test]
fn unpolled_protected_future_owns_and_zeroizes_before_drop_without_a_runtime() {
    let (work, budget) = work();
    let events = Arc::new(Mutex::new(Vec::new()));
    let secret = Zeroizing::new(ObservedSecret { bytes: [0xAB; 8], events: events.clone() });
    let future = work.begin().unwrap().execute_protected(secret, |_secret| Ok(()));
    assert!(events.lock().unwrap().is_empty());
    drop(future);
    assert_eq!(*events.lock().unwrap(), [("zeroized", true), ("dropped", true)]);
    assert_eq!(budget.available(), 1);
}

#[tokio::test]
async fn cancelled_waiter_leaves_zeroizing_inputs_owned_until_the_worker_finishes() {
    let (work, budget) = work();
    let events = Arc::new(Mutex::new(Vec::new()));
    let secret = Zeroizing::new(ObservedSecret { bytes: [0xAB; 8], events: events.clone() });
    let (started, ready) = oneshot::channel();
    let (release, proceed) = mpsc::channel();
    let request = tokio::spawn(work.begin().unwrap().execute_protected(secret, move |secret| {
        started.send(()).unwrap();
        proceed.recv_timeout(Duration::from_secs(10)).unwrap();
        drop(secret);
        Ok(())
    }));
    ready.await.unwrap();
    request.abort();
    assert!(request.await.unwrap_err().is_cancelled());
    assert_eq!(work.begin().unwrap_err(), AdmissionError::Busy);
    assert!(events.lock().unwrap().is_empty());
    release.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        while budget.available() == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(*events.lock().unwrap(), [("zeroized", true), ("dropped", true)]);
}

#[tokio::test]
async fn worker_unwind_zeroizes_inputs_and_preserves_join_failure() {
    let (work, budget) = work();
    let events = Arc::new(Mutex::new(Vec::new()));
    let secret = Zeroizing::new(ObservedSecret { bytes: [0xAB; 8], events: events.clone() });
    let error = work
        .begin()
        .unwrap()
        .execute_protected(secret, |_secret| -> Result<(), PasswordError> {
            panic!("simulated protected worker failure");
        })
        .await
        .unwrap_err();
    assert!(matches!(error, PasswordError::Join(join) if join.is_panic()));
    assert_eq!(*events.lock().unwrap(), [("zeroized", true), ("dropped", true)]);
    assert_eq!(budget.available(), 1);
}
