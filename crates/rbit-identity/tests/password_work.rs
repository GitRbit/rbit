//! Public bounded-work behavior and compatibility with existing stored credentials.

use std::num::NonZeroUsize;

use rbit_identity::{PasswordWork, Passwords};
use rbit_kernel::{AdmissionError, BlockingBudget};

fn work(capacity: usize) -> (PasswordWork, BlockingBudget) {
    let budget = BlockingBudget::new(NonZeroUsize::new(capacity).unwrap());
    (PasswordWork::new(budget.clone()), budget)
}

#[test]
fn clones_share_admission_and_shutdown_without_account_or_password_inputs() {
    let (work, budget) = work(1);
    let other_consumer = work.clone();
    let admitted = work.begin().unwrap();
    assert_eq!(other_consumer.begin().unwrap_err(), AdmissionError::Busy);
    drop(admitted);
    assert_eq!(budget.available(), 1);
    let admitted = other_consumer.begin().unwrap();
    work.close();
    assert_eq!(other_consumer.begin().unwrap_err(), AdmissionError::Closed);
    drop(admitted);
    assert_eq!(budget.available(), 1);
}

#[test]
fn dropping_unpolled_password_future_releases_reservation_without_a_runtime() {
    let (work, budget) = work(1);
    let not_submitted = work.begin().unwrap().hash("never submitted".to_owned());
    assert_eq!(budget.available(), 0);
    drop(not_submitted);
    assert_eq!(budget.available(), 1);
}

#[tokio::test]
async fn hashes_preserve_parameters_and_fresh_salts_and_verify_across_facades() {
    let (work, budget) = work(1);
    let first = work.begin().unwrap().hash("existing password".into()).await.unwrap();
    let second = work.begin().unwrap().hash("existing password".into()).await.unwrap();
    let prefix = "$argon2id$v=19$m=19456,t=2,p=1$";
    assert!(first.starts_with(prefix) && second.starts_with(prefix));
    assert_ne!(first.split('$').nth(4), second.split('$').nth(4), "new hashes use fresh random salt");
    assert!(Passwords::verify("existing password".into(), first.clone()).await.unwrap());
    assert!(work.begin().unwrap().verify("existing password".into(), first.clone()).await.unwrap());
    assert!(!work.begin().unwrap().verify("wrong password".into(), first).await.unwrap());
    assert_eq!(budget.available(), 1);
}

#[tokio::test]
async fn malformed_and_dummy_verification_preserve_rejection_and_release_capacity() {
    let (work, budget) = work(1);
    for stored in ["", "malformed", "$argon2id$v=19$invalid"] {
        assert!(!work.begin().unwrap().verify("supplied password".into(), stored.into()).await.unwrap());
    }
    work.begin().unwrap().verify_dummy("missing login password".into()).await.unwrap();
    assert_eq!(budget.available(), 1);
}

#[tokio::test]
async fn replacement_requires_valid_current_credentials_and_uses_one_capacity_slot() {
    let (work, budget) = work(1);
    let current = work.begin().unwrap().hash("old password".into()).await.unwrap();
    let invalid = work.begin().unwrap().verify_and_hash("incorrect".into(), current.clone(), "replacement".into()).await.unwrap();
    assert!(invalid.is_none());
    let replacement = work.begin().unwrap().verify_and_hash("old password".into(), current, "replacement".into()).await.unwrap().unwrap();
    assert!(work.begin().unwrap().verify("replacement".into(), replacement.clone()).await.unwrap());
    assert!(!work.begin().unwrap().verify("old password".into(), replacement).await.unwrap());
    assert_eq!(budget.available(), 1);
}
