//! Login covering work keeps rejection and admission independent of account existence.

use std::num::NonZeroUsize;

use rbit_identity::PasswordWork;
use rbit_kernel::{AdmissionError, BlockingBudget};

#[tokio::test]
async fn valid_stored_credentials_accept_only_the_correct_password() {
    let budget = BlockingBudget::new(NonZeroUsize::new(1).unwrap());
    let work = PasswordWork::new(budget.clone());
    let stored = work.begin().unwrap().hash("real password".into()).await.unwrap();
    assert!(work.begin().unwrap().verify_login("real password".into(), Some(stored.clone())).await.unwrap());
    assert!(!work.begin().unwrap().verify_login("wrong password".into(), Some(stored)).await.unwrap());
    assert_eq!(budget.available(), 1);
}

#[tokio::test]
async fn absent_malformed_and_unsupported_credentials_reject_without_an_error() {
    let budget = BlockingBudget::new(NonZeroUsize::new(1).unwrap());
    let work = PasswordWork::new(budget.clone());
    let stored = work.begin().unwrap().hash("real password".into()).await.unwrap();
    for previous in [
        None,
        Some(String::new()),
        Some("malformed".into()),
        Some("$argon2id".into()),
        Some(stored.replace("argon2id", "unsupported")),
        Some(stored.replace("v=19", "v=999")),
    ] {
        assert!(!work.begin().unwrap().verify_login("real password".into(), previous).await.unwrap());
        assert_eq!(budget.available(), 1);
    }
}

#[tokio::test]
async fn missing_login_work_uses_its_reserved_slot_after_admission_closes() {
    let budget = BlockingBudget::new(NonZeroUsize::new(1).unwrap());
    let work = PasswordWork::new(budget.clone());
    let attempt = work.begin().unwrap();
    work.close();
    assert_eq!(work.begin().unwrap_err(), AdmissionError::Closed);
    assert!(!attempt.verify_login("unregistered password".into(), None).await.unwrap());
    assert_eq!(budget.available(), 1);
}
