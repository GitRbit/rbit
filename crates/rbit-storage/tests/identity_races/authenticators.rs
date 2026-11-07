//! TOTP steps, recovery codes and enrolment transitions under contention: one proof is
//! honoured once, and state changes stay bound to the secret that was verified.

use rbit_core::AccountId;
use rbit_storage::Database;

use super::{ROUNDS, create_user, open_database, race};

const CONTENDERS: usize = 8;
const SECRET: [u8; 20] = [1; 20];
const REPLACEMENT: [u8; 20] = [2; 20];
const CODES: [[u8; 32]; 2] = [[3; 32], [4; 32]];

/// An account whose authenticator was confirmed at step 1.
async fn enrolled(database: &Database, name: &str) -> AccountId {
    let account = create_user(database, name, "hash").await.id();
    let store = database.two_factor();
    assert!(store.begin_enrolment(account, &SECRET).await.unwrap());
    assert_eq!(store.confirm(account, &SECRET, 1, &CODES).await.unwrap(), Some(true));
    account
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn one_totp_step_is_honoured_once_under_contention() {
    let dir = tempfile::tempdir().unwrap();
    let database = open_database(&dir).await;
    let account = enrolled(&database, "alice").await;
    for round in 0..u64::from(ROUNDS) {
        let step = 10 + round;
        let outcomes = race(CONTENDERS, |_| {
            let store = database.two_factor();
            async move { store.claim_step(account, &SECRET, step).await.unwrap() }
        })
        .await;
        assert_eq!(outcomes.iter().filter(|won| **won).count(), 1, "step {step} was replayed: {outcomes:?}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_newest_step_always_wins_and_older_steps_cannot_follow() {
    let dir = tempfile::tempdir().unwrap();
    let database = open_database(&dir).await;
    let account = enrolled(&database, "alice").await;
    let store = database.two_factor();
    let mut floor = 1;
    for _ in 0..ROUNDS {
        let base = floor + 1;
        let outcomes = race(CONTENDERS, |index| {
            let store = database.two_factor();
            async move { store.claim_step(account, &SECRET, base + u64::try_from(index).unwrap()).await.unwrap() }
        })
        .await;
        let newest = base + u64::try_from(CONTENDERS).unwrap() - 1;
        assert!(outcomes[CONTENDERS - 1], "the newest step is never shadowed by an older one");
        assert_eq!(store.find(account).await.unwrap().unwrap().last_used_step, Some(newest));
        for step in base..=newest {
            assert!(!store.claim_step(account, &SECRET, step).await.unwrap(), "step {step} replayed after a newer one");
        }
        floor = newest;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_recovery_code_is_spent_once_under_contention() {
    let dir = tempfile::tempdir().unwrap();
    let database = open_database(&dir).await;
    let store = database.two_factor();
    for round in 0..ROUNDS {
        let account = enrolled(&database, &format!("codes{round}")).await;
        let outcomes = race(CONTENDERS, |_| {
            let store = database.two_factor();
            async move { store.spend_recovery_code(account, &CODES[0]).await.unwrap() }
        })
        .await;
        assert_eq!(outcomes.iter().filter(|won| **won).count(), 1, "round {round}: a spent code won twice: {outcomes:?}");
        assert_eq!(store.remaining_recovery_codes(account).await.unwrap(), 1);
    }
}

/// Cancellation racing confirmation must never remove a factor that was just enabled.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn confirmation_racing_cancellation_never_removes_an_enabled_factor() {
    let dir = tempfile::tempdir().unwrap();
    let database = open_database(&dir).await;
    let store = database.two_factor();
    for round in 0..ROUNDS {
        let account = create_user(&database, &format!("cancel{round}"), "hash").await.id();
        assert!(store.begin_enrolment(account, &SECRET).await.unwrap());
        let outcomes = race(2, |index| {
            let store = database.two_factor();
            async move {
                if index == 0 {
                    store.confirm(account, &SECRET, 1, &CODES).await.unwrap().is_some()
                } else {
                    store.cancel_enrolment(account).await.unwrap();
                    false
                }
            }
        })
        .await;
        if outcomes[0] {
            assert!(store.is_enabled(account).await.unwrap(), "round {round}: cancellation removed a confirmed factor");
            assert_eq!(store.remaining_recovery_codes(account).await.unwrap(), 2);
        } else {
            assert!(store.find(account).await.unwrap().is_none(), "round {round}: a cancelled enrolment was confirmed");
            assert_eq!(store.remaining_recovery_codes(account).await.unwrap(), 0);
        }
    }
}

/// Replacing a pending secret races confirmation of the old one: exactly one applies.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn confirmation_racing_replacement_stays_bound_to_the_verified_secret() {
    let dir = tempfile::tempdir().unwrap();
    let database = open_database(&dir).await;
    let store = database.two_factor();
    for round in 0..ROUNDS {
        let account = create_user(&database, &format!("replace{round}"), "hash").await.id();
        assert!(store.begin_enrolment(account, &SECRET).await.unwrap());
        let outcomes = race(2, |index| {
            let store = database.two_factor();
            async move {
                if index == 0 {
                    store.confirm(account, &SECRET, 1, &CODES).await.unwrap().is_some()
                } else {
                    store.begin_enrolment(account, &REPLACEMENT).await.unwrap()
                }
            }
        })
        .await;
        assert_ne!(outcomes[0], outcomes[1], "round {round}: exactly one of confirmation and replacement applies");
        let record = store.find(account).await.unwrap().unwrap();
        if outcomes[0] {
            assert!(record.is_enabled());
            assert_eq!(record.secret, SECRET);
            assert!(store.claim_step(account, &SECRET, 2).await.unwrap());
            assert!(!store.claim_step(account, &REPLACEMENT, 3).await.unwrap());
        } else {
            assert!(!record.is_enabled(), "round {round}: the replaced secret was confirmed");
            assert_eq!(record.secret, REPLACEMENT);
            assert!(!store.claim_step(account, &SECRET, 2).await.unwrap());
        }
    }
}
