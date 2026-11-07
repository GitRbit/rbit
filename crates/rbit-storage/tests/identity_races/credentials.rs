//! Recovery links and password proofs under contention: a consumed link never wins a
//! second time, and a link and a stale password proof can never both take effect.

use rbit_core::{Email, Timestamp, User};
use rbit_storage::{
    Database,
    sqlite::recovery::{NewEmailToken, TokenPurpose},
};

use super::{ROUNDS, create_user, open_database, race};

const CONTENDERS: usize = 8;
const ORIGINAL_HASH: &str = "original-password-hash";

fn link(user: &User, purpose: TokenPurpose, digest: [u8; 32]) -> NewEmailToken {
    NewEmailToken {
        digest,
        account_id: user.id(),
        purpose,
        email: user.email.clone(),
        expires_at: Timestamp(Timestamp::now().seconds() + 3600),
    }
}

async fn password_hash(database: &Database, user: &User) -> String {
    database.profiles().password_hash(user.id()).await.unwrap().unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn one_reset_link_redeemed_concurrently_replaces_the_password_once() {
    let dir = tempfile::tempdir().unwrap();
    let database = open_database(&dir).await;
    let alice = create_user(&database, "alice", ORIGINAL_HASH).await;
    let recovery = database.recovery();
    for round in 0..ROUNDS {
        let digest = [round; 32];
        recovery.insert(&link(&alice, TokenPurpose::Reset, digest)).await.unwrap();
        let outcomes = race(CONTENDERS, |index| {
            let recovery = recovery.clone();
            async move { recovery.reset_password(&digest, &format!("hash-{round}-{index}"), Timestamp::now()).await.unwrap().is_some() }
        })
        .await;
        let winners: Vec<_> = outcomes.iter().enumerate().filter(|(_, won)| **won).map(|(index, _)| index).collect();
        assert_eq!(winners.len(), 1, "round {round}: a consumed reset link won again: {outcomes:?}");
        assert_eq!(password_hash(&database, &alice).await, format!("hash-{round}-{}", winners[0]));
        assert!(recovery.find_valid(&digest, TokenPurpose::Reset, Timestamp::now()).await.unwrap().is_none());
        assert!(recovery.reset_password(&digest, "late-hash", Timestamp::now()).await.unwrap().is_none());
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn one_verification_link_is_consumed_once_under_contention() {
    let dir = tempfile::tempdir().unwrap();
    let database = open_database(&dir).await;
    let alice = create_user(&database, "alice", ORIGINAL_HASH).await;
    let recovery = database.recovery();
    for round in 0..ROUNDS {
        let digest = [round; 32];
        recovery.insert(&link(&alice, TokenPurpose::Verify, digest)).await.unwrap();
        let outcomes = race(CONTENDERS, |_| {
            let recovery = recovery.clone();
            async move { recovery.consume(&digest, TokenPurpose::Verify, Timestamp::now()).await.unwrap().is_some() }
        })
        .await;
        assert_eq!(outcomes.iter().filter(|won| **won).count(), 1, "round {round}: {outcomes:?}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_changes_from_one_verified_password_replace_it_once() {
    let dir = tempfile::tempdir().unwrap();
    let database = open_database(&dir).await;
    let alice = create_user(&database, "alice", ORIGINAL_HASH).await;
    let account = alice.id();
    let mut expected = ORIGINAL_HASH.to_owned();
    for round in 0..ROUNDS {
        let verified = expected.clone();
        let outcomes = race(CONTENDERS, |index| {
            let profiles = database.profiles();
            let verified = verified.clone();
            async move { profiles.set_password_hash_if_current(account, &format!("hash-{round}-{index}"), &verified).await.unwrap() }
        })
        .await;
        let winners: Vec<_> = outcomes.iter().enumerate().filter(|(_, won)| **won).map(|(index, _)| index).collect();
        assert_eq!(winners.len(), 1, "round {round}: one verified password permits one replacement: {outcomes:?}");
        expected = format!("hash-{round}-{}", winners[0]);
        assert_eq!(password_hash(&database, &alice).await, expected);
    }
}

/// A self-service change verified against the old password races a reset link.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_password_change_and_a_reset_link_cannot_both_replace_the_password() {
    let dir = tempfile::tempdir().unwrap();
    let database = open_database(&dir).await;
    let recovery = database.recovery();
    for round in 0..ROUNDS {
        let user = create_user(&database, &format!("change{round}"), ORIGINAL_HASH).await;
        recovery.insert(&link(&user, TokenPurpose::Reset, [round; 32])).await.unwrap();
        let account = user.id();
        let outcomes = race(2, |index| {
            let (profiles, recovery) = (database.profiles(), recovery.clone());
            async move {
                if index == 0 {
                    profiles.set_password_hash_if_current(account, "changed-hash", ORIGINAL_HASH).await.unwrap()
                } else {
                    recovery.reset_password(&[round; 32], "reset-hash", Timestamp::now()).await.unwrap().is_some()
                }
            }
        })
        .await;
        assert_ne!(outcomes[0], outcomes[1], "round {round}: exactly one credential write wins");
        let expected = if outcomes[0] { "changed-hash" } else { "reset-hash" };
        assert_eq!(password_hash(&database, &user).await, expected);
        assert!(recovery.find_valid(&[round; 32], TokenPurpose::Reset, Timestamp::now()).await.unwrap().is_none());
    }
}

/// A stale password proof racing a reset link cannot redirect the account's mailbox.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_email_change_and_a_reset_link_cannot_both_take_effect() {
    let dir = tempfile::tempdir().unwrap();
    let database = open_database(&dir).await;
    let recovery = database.recovery();
    for round in 0..ROUNDS {
        let user = create_user(&database, &format!("mail{round}"), ORIGINAL_HASH).await;
        recovery.insert(&link(&user, TokenPurpose::Reset, [round; 32])).await.unwrap();
        let account = user.id();
        let changed: Email = format!("moved{round}@example.com").parse().unwrap();
        let outcomes = race(2, |index| {
            let (profiles, recovery, changed) = (database.profiles(), recovery.clone(), changed.clone());
            async move {
                if index == 0 {
                    profiles.set_email_if_password(account, &changed, ORIGINAL_HASH).await.unwrap()
                } else {
                    recovery.reset_password(&[round; 32], "reset-hash", Timestamp::now()).await.unwrap().is_some()
                }
            }
        })
        .await;
        assert_ne!(outcomes[0], outcomes[1], "round {round}: exactly one of the email change and the reset wins");
        let stored = recovery.find_account(user.id()).await.unwrap().unwrap();
        if outcomes[0] {
            assert_eq!(stored.email, changed);
            assert_eq!(password_hash(&database, &user).await, ORIGINAL_HASH);
        } else {
            assert_eq!(stored.email, user.email);
            assert_eq!(password_hash(&database, &user).await, "reset-hash");
        }
    }
}
