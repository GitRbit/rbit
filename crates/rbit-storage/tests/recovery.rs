//! Email token and verification persistence against a real, migrated `SQLite` database.

use rbit_core::{AccountId, Email, Timestamp, User};
use rbit_storage::{
    Database, NewUser,
    sqlite::recovery::{NewEmailToken, TokenPurpose},
};

async fn open_database(dir: &tempfile::TempDir) -> Database {
    Database::open(&dir.path().join("rbit.db")).await.unwrap()
}

async fn create_user(database: &Database, name: &str) -> User {
    database
        .accounts()
        .create_user(NewUser {
            name: name.parse().unwrap(),
            email: format!("{name}@example.com").parse().unwrap(),
            display_name: None,
            password_hash: "not-a-real-hash".to_owned(),
            is_admin: false,
        })
        .await
        .unwrap()
}

fn token(user: &User, purpose: TokenPurpose, digest_byte: u8, expires_at: i64) -> NewEmailToken {
    NewEmailToken {
        digest: [digest_byte; 32],
        account_id: user.id(),
        purpose,
        email: user.email.clone(),
        expires_at: Timestamp(expires_at),
    }
}

fn now() -> Timestamp {
    Timestamp::now()
}

fn later() -> i64 {
    now().seconds() + 3600
}

#[tokio::test]
async fn tokens_are_single_use() {
    let dir = tempfile::tempdir().unwrap();
    let database = open_database(&dir).await;
    let store = database.recovery();
    let alice = create_user(&database, "alice").await;

    store.insert(&token(&alice, TokenPurpose::Reset, 1, later())).await.unwrap();

    let found = store.find_valid(&[1; 32], TokenPurpose::Reset, now()).await.unwrap().unwrap();
    assert_eq!(found.account_id, alice.id());
    assert_eq!(found.email, alice.email);
    // Looking does not use it up.
    assert!(store.find_valid(&[1; 32], TokenPurpose::Reset, now()).await.unwrap().is_some());

    assert!(store.consume(&[1; 32], TokenPurpose::Reset, now()).await.unwrap().is_some());
    assert!(store.consume(&[1; 32], TokenPurpose::Reset, now()).await.unwrap().is_none());
    assert!(store.find_valid(&[1; 32], TokenPurpose::Reset, now()).await.unwrap().is_none());
}

#[tokio::test]
async fn tokens_are_bound_to_purpose_expiry_and_address() {
    let dir = tempfile::tempdir().unwrap();
    let database = open_database(&dir).await;
    let store = database.recovery();
    let alice = create_user(&database, "alice").await;

    store.insert(&token(&alice, TokenPurpose::Verify, 2, later())).await.unwrap();
    assert!(store.find_valid(&[2; 32], TokenPurpose::Reset, now()).await.unwrap().is_none(), "wrong purpose");
    assert!(store.find_valid(&[9; 32], TokenPurpose::Verify, now()).await.unwrap().is_none(), "unknown digest");
    assert!(store.consume(&[2; 32], TokenPurpose::Verify, Timestamp(later() + 1)).await.unwrap().is_none(), "expired");

    // Changing the address invalidates links sent to the old one.
    let new_email: Email = "alice@new.example".parse().unwrap();
    database.profiles().set_email(alice.id(), &new_email).await.unwrap();
    assert!(store.find_valid(&[2; 32], TokenPurpose::Verify, now()).await.unwrap().is_none());

    // Letter case of the address does not matter.
    let shouted = NewEmailToken { email: "ALICE@NEW.EXAMPLE".parse().unwrap(), ..token(&alice, TokenPurpose::Verify, 3, later()) };
    store.insert(&shouted).await.unwrap();
    assert!(store.consume(&[3; 32], TokenPurpose::Verify, now()).await.unwrap().is_some());
}

#[tokio::test]
async fn revoking_uses_up_outstanding_tokens() {
    let dir = tempfile::tempdir().unwrap();
    let database = open_database(&dir).await;
    let store = database.recovery();
    let alice = create_user(&database, "alice").await;

    store.insert(&token(&alice, TokenPurpose::Reset, 4, later())).await.unwrap();
    store.insert(&token(&alice, TokenPurpose::Reset, 5, later())).await.unwrap();
    store.insert(&token(&alice, TokenPurpose::Verify, 6, later())).await.unwrap();

    assert_eq!(store.revoke_all(alice.id(), TokenPurpose::Reset).await.unwrap(), 2);
    assert!(store.find_valid(&[4; 32], TokenPurpose::Reset, now()).await.unwrap().is_none());
    assert!(store.find_valid(&[6; 32], TokenPurpose::Verify, now()).await.unwrap().is_some(), "other purposes untouched");
}

#[tokio::test]
async fn expired_tokens_are_swept_on_insert() {
    let dir = tempfile::tempdir().unwrap();
    let database = open_database(&dir).await;
    let store = database.recovery();
    let alice = create_user(&database, "alice").await;

    store.insert(&token(&alice, TokenPurpose::Reset, 7, now().seconds() - 1)).await.unwrap();
    store.insert(&token(&alice, TokenPurpose::Reset, 8, later())).await.unwrap();
    // The expired token is gone, so its digest can be stored again.
    store.insert(&token(&alice, TokenPurpose::Reset, 7, later())).await.unwrap();
}

#[tokio::test]
async fn verification_follows_the_address() {
    let dir = tempfile::tempdir().unwrap();
    let database = open_database(&dir).await;
    let store = database.recovery();
    let alice = create_user(&database, "alice").await;
    let bob = create_user(&database, "bob").await;

    let account = store.find_account_by_email(&"ALICE@example.com".parse().unwrap()).await.unwrap().unwrap();
    assert_eq!(account.id, alice.id());
    assert_eq!(account.email_verified_at, None);
    assert!(!account.suspended);
    assert!(store.find_account_by_email(&"nobody@example.com".parse().unwrap()).await.unwrap().is_none());

    assert!(!store.mark_verified(alice.id(), &"other@example.com".parse().unwrap(), now()).await.unwrap(), "stale address");
    assert!(store.mark_verified(alice.id(), &alice.email, Timestamp(100)).await.unwrap());
    assert_eq!(store.find_account(alice.id()).await.unwrap().unwrap().email_verified_at, Some(Timestamp(100)));

    let verified = store.verified_among(&[alice.id(), bob.id(), AccountId(999)]).await.unwrap();
    assert!(verified.contains(&alice.id()) && !verified.contains(&bob.id()));
    assert!(store.verified_among(&[]).await.unwrap().is_empty());

    // The same address in another case keeps verification; a new address loses it.
    database.profiles().set_email(alice.id(), &"Alice@Example.com".parse().unwrap()).await.unwrap();
    assert!(store.find_account(alice.id()).await.unwrap().unwrap().email_verified_at.is_some());
    database.profiles().set_email(alice.id(), &"alice@new.example".parse().unwrap()).await.unwrap();
    assert!(store.find_account(alice.id()).await.unwrap().unwrap().email_verified_at.is_none());
}

#[tokio::test]
async fn distinct_reset_links_cannot_both_replace_the_password() {
    let dir = tempfile::tempdir().unwrap();
    let database = open_database(&dir).await;
    let store = database.recovery();
    let alice = create_user(&database, "alice").await;
    store.insert(&token(&alice, TokenPurpose::Reset, 1, later())).await.unwrap();
    store.insert(&token(&alice, TokenPurpose::Reset, 2, later())).await.unwrap();

    let (first, second) = tokio::join!(
        store.reset_password(&[1; 32], "first-password-hash", now()),
        store.reset_password(&[2; 32], "second-password-hash", now()),
    );
    let (first, second) = (first.unwrap(), second.unwrap());
    assert_ne!(first.is_some(), second.is_some());
    let expected = if first.is_some() { "first-password-hash" } else { "second-password-hash" };
    assert_eq!(database.profiles().password_hash(alice.id()).await.unwrap().as_deref(), Some(expected));
    assert!(store.find_account(alice.id()).await.unwrap().unwrap().email_verified_at.is_some());
    assert!(store.find_valid(&[1; 32], TokenPurpose::Reset, now()).await.unwrap().is_none());
    assert!(store.find_valid(&[2; 32], TokenPurpose::Reset, now()).await.unwrap().is_none());
}

#[tokio::test]
async fn changed_credentials_permanently_invalidate_existing_email_links() {
    let dir = tempfile::tempdir().unwrap();
    let database = open_database(&dir).await;
    let store = database.recovery();
    let alice = create_user(&database, "alice").await;
    store.insert(&token(&alice, TokenPurpose::Reset, 1, later())).await.unwrap();
    database.profiles().set_password_hash(alice.id(), "new-password-hash").await.unwrap();
    assert!(store.reset_password(&[1; 32], "stale-password-hash", now()).await.unwrap().is_none());
    assert_eq!(database.profiles().password_hash(alice.id()).await.unwrap().as_deref(), Some("new-password-hash"));

    store.insert(&token(&alice, TokenPurpose::Reset, 2, later())).await.unwrap();
    store.insert(&token(&alice, TokenPurpose::Verify, 3, later())).await.unwrap();
    database.profiles().set_email(alice.id(), &"new@example.com".parse().unwrap()).await.unwrap();
    database.profiles().set_email(alice.id(), &alice.email).await.unwrap();
    assert!(store.find_valid(&[2; 32], TokenPurpose::Reset, now()).await.unwrap().is_none());
    assert!(store.find_valid(&[3; 32], TokenPurpose::Verify, now()).await.unwrap().is_none());
}

#[tokio::test]
async fn reset_links_are_rechecked_against_suspension_at_redemption() {
    let dir = tempfile::tempdir().unwrap();
    let database = open_database(&dir).await;
    let store = database.recovery();
    let alice = create_user(&database, "alice").await;
    store.insert(&token(&alice, TokenPurpose::Reset, 1, later())).await.unwrap();
    assert!(store.find_valid(&[1; 32], TokenPurpose::Reset, now()).await.unwrap().is_some());
    database.admin().set_suspended(alice.id(), true).await.unwrap();
    assert!(store.reset_password(&[1; 32], "new-password-hash", now()).await.unwrap().is_none());
    assert_eq!(database.profiles().password_hash(alice.id()).await.unwrap().as_deref(), Some("not-a-real-hash"));
}

#[tokio::test]
async fn password_reset_invalidates_already_verified_profile_requests() {
    let dir = tempfile::tempdir().unwrap();
    let database = open_database(&dir).await;
    let store = database.recovery();
    let profiles = database.profiles();
    let alice = create_user(&database, "alice").await;
    let verified = profiles.password_hash(alice.id()).await.unwrap().unwrap();
    store.insert(&token(&alice, TokenPurpose::Reset, 1, later())).await.unwrap();
    assert!(store.reset_password(&[1; 32], "recovered-password", now()).await.unwrap().is_some());
    store.insert(&token(&alice, TokenPurpose::Reset, 2, later())).await.unwrap();

    assert!(!profiles.set_password_hash_if_current(alice.id(), "stale-password", &verified).await.unwrap());
    assert!(!profiles.set_email_if_password(alice.id(), &"attacker@example.com".parse().unwrap(), &verified).await.unwrap());
    assert_eq!(profiles.password_hash(alice.id()).await.unwrap().as_deref(), Some("recovered-password"));
    assert_eq!(store.find_account(alice.id()).await.unwrap().unwrap().email, alice.email);
    assert!(store.find_valid(&[2; 32], TokenPurpose::Reset, now()).await.unwrap().is_some(), "stale proof must not revoke new links");

    let (first, second) = tokio::join!(
        profiles.set_password_hash_if_current(alice.id(), "first-password", "recovered-password"),
        profiles.set_password_hash_if_current(alice.id(), "second-password", "recovered-password"),
    );
    assert_ne!(first.unwrap(), second.unwrap(), "one verified password permits only one replacement");
    assert!(store.find_valid(&[2; 32], TokenPurpose::Reset, now()).await.unwrap().is_none());
    let current = profiles.password_hash(alice.id()).await.unwrap().unwrap();
    let changed_email = "new@example.com".parse().unwrap();
    assert!(profiles.set_email_if_password(alice.id(), &changed_email, &current).await.unwrap());
    assert_eq!(store.find_account(alice.id()).await.unwrap().unwrap().email, changed_email);
}
