//! Development fault fixtures for a proposed proof/lease contract, never production activation.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

#[derive(Debug, Clone, Copy)]
pub(crate) enum Fault {
    Wait,
    Query,
    Assign,
    Resume,
}

/// Only complete native observations may construct this receipt in the proof driver.
#[derive(Debug)]
pub(crate) struct VerifiedCompletion {
    _private: (),
}

impl VerifiedCompletion {
    pub fn observe(root_reaped: bool, members_empty: bool, pumps_destroyed: bool) -> Option<Self> {
        (root_reaped && members_empty && pumps_destroyed).then_some(Self { _private: () })
    }
}

/// An unverified owner must close admission before releasing its ordinary permit.
#[derive(Debug)]
pub(crate) struct Admission {
    permit: Option<OwnedSemaphorePermit>,
    gate: Arc<Semaphore>,
    quarantined: Arc<AtomicBool>,
    verified: bool,
}

impl Admission {
    pub async fn new(gate: Arc<Semaphore>, quarantined: Arc<AtomicBool>) -> Self {
        let permit = Arc::clone(&gate).acquire_owned().await.expect("proof admission starts open");
        Self { permit: Some(permit), gate, quarantined, verified: false }
    }

    pub fn complete(mut self, _proof: VerifiedCompletion) {
        self.verified = true;
        drop(self.permit.take());
    }

    pub fn quarantine(self, _fault: Fault) {
        drop(self);
    }
}

impl Drop for Admission {
    fn drop(&mut self) {
        if !self.verified {
            self.gate.close();
            self.quarantined.store(true, Ordering::SeqCst);
        }
        // Field destruction follows gate closure, including during unwind.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn wait_query_assign_resume_faults_close_admission_instead_of_claiming_stopped() {
        for fault in [Fault::Wait, Fault::Query, Fault::Assign, Fault::Resume] {
            let gate = Arc::new(Semaphore::new(1));
            let quarantined = Arc::new(AtomicBool::new(false));
            let lease = Admission::new(Arc::clone(&gate), Arc::clone(&quarantined)).await;
            lease.quarantine(fault);
            assert!(gate.is_closed());
            assert!(gate.try_acquire().is_err());
            assert!(quarantined.load(Ordering::SeqCst));
        }
    }

    #[tokio::test]
    async fn panic_defaults_to_quarantine_and_partial_observations_never_verify() {
        let gate = Arc::new(Semaphore::new(1));
        let quarantined = Arc::new(AtomicBool::new(false));
        let lease = Admission::new(Arc::clone(&gate), quarantined).await;
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
                let _lease = lease;
                panic!("proof fault");
            }))
            .is_err()
        );
        assert!(gate.is_closed());
        for evidence in [(false, true, true), (true, false, true), (true, true, false)] {
            assert!(VerifiedCompletion::observe(evidence.0, evidence.1, evidence.2).is_none());
        }
    }

    #[tokio::test]
    async fn verified_completion_releases_usable_admission() {
        let gate = Arc::new(Semaphore::new(1));
        let lease = Admission::new(Arc::clone(&gate), Arc::new(AtomicBool::new(false))).await;
        lease.complete(VerifiedCompletion::observe(true, true, true).unwrap());
        assert!(!gate.is_closed());
        assert_eq!(gate.available_permits(), 1);
    }
}
