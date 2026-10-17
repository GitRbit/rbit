//! Authenticator state transitions must stay bound to the factor that was verified.

use rbit_storage::{Database, NewUser};

#[tokio::test]
async fn replaced_authenticator_secrets_cannot_be_confirmed_or_claimed() {
    let dir = tempfile::tempdir().unwrap();
    let database = Database::open(&dir.path().join("rbit.db")).await.unwrap();
    let user = database
        .accounts()
        .create_user(NewUser {
            name: "alice".parse().unwrap(),
            email: "alice@example.com".parse().unwrap(),
            display_name: None,
            password_hash: "not-a-real-hash".to_owned(),
            is_admin: false,
        })
        .await
        .unwrap();
    let store = database.two_factor();
    let old = [1; 20];
    let current = [2; 20];
    let codes = [[3; 32]];

    assert!(store.begin_enrolment(user.id(), &old).await.unwrap());
    assert!(store.begin_enrolment(user.id(), &current).await.unwrap());
    assert_eq!(store.confirm(user.id(), &old, 1, &codes).await.unwrap(), None);
    assert!(!store.is_enabled(user.id()).await.unwrap());
    assert_eq!(store.confirm(user.id(), &current, 1, &codes).await.unwrap(), Some(true));

    assert!(!store.claim_step(user.id(), &old, 2).await.unwrap());
    assert!(store.claim_step(user.id(), &current, 2).await.unwrap());
    assert!(!store.claim_step(user.id(), &current, 2).await.unwrap());

    // A cancellation that arrives after confirmation must preserve both the factor
    // and the recovery codes issued when it was enabled.
    store.cancel_enrolment(user.id()).await.unwrap();
    assert!(store.is_enabled(user.id()).await.unwrap());
    assert_eq!(store.remaining_recovery_codes(user.id()).await.unwrap(), 1);
    assert!(store.disable(user.id()).await.unwrap());
    assert!(store.begin_enrolment(user.id(), &old).await.unwrap());
    store.cancel_enrolment(user.id()).await.unwrap();
    assert!(store.find(user.id()).await.unwrap().is_none());
}
