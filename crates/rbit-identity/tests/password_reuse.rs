//! Password-reuse checks and replacement hashing share one admitted worker.

use std::num::NonZeroUsize;

use rbit_identity::PasswordWork;
use rbit_kernel::{AdmissionError, BlockingBudget};

#[tokio::test]
async fn reused_candidate_is_rejected_and_different_candidate_hashes() {
    let budget = BlockingBudget::new(NonZeroUsize::new(1).unwrap());
    let work = PasswordWork::new(budget.clone());
    let previous = work.begin().unwrap().hash("initial password".into()).await.unwrap();
    let reused = work.begin().unwrap().hash_if_different("initial password".into(), previous.clone()).await.unwrap();
    assert!(reused.is_none());
    let replacement = work.begin().unwrap().hash_if_different("different password".into(), previous).await.unwrap().unwrap();
    assert!(work.begin().unwrap().verify("different password".into(), replacement.clone()).await.unwrap());
    assert!(!work.begin().unwrap().verify("initial password".into(), replacement).await.unwrap());
    assert_eq!(budget.available(), 1);
}

#[tokio::test]
async fn malformed_previous_hash_keeps_false_verification_behavior() {
    let budget = BlockingBudget::new(NonZeroUsize::new(1).unwrap());
    let work = PasswordWork::new(budget.clone());
    for previous in ["", "malformed", "$argon2id$v=19$invalid"] {
        let replacement = work.begin().unwrap().hash_if_different("candidate password".into(), previous.into()).await.unwrap().unwrap();
        assert!(work.begin().unwrap().verify("candidate password".into(), replacement).await.unwrap());
        assert_eq!(budget.available(), 1);
    }
}

#[tokio::test]
async fn closing_admission_between_reservation_and_execution_does_not_readmit() {
    let budget = BlockingBudget::new(NonZeroUsize::new(1).unwrap());
    let work = PasswordWork::new(budget.clone());
    let previous = work.begin().unwrap().hash("previous password".into()).await.unwrap();
    let attempt = work.begin().unwrap();
    work.close();
    assert_eq!(work.begin().unwrap_err(), AdmissionError::Closed);
    let replacement = attempt.hash_if_different("new candidate".into(), previous).await.unwrap().unwrap();
    let verifier = PasswordWork::new(BlockingBudget::new(NonZeroUsize::new(1).unwrap()));
    assert!(verifier.begin().unwrap().verify("new candidate".into(), replacement).await.unwrap());
    assert_eq!(budget.available(), 1);
}
