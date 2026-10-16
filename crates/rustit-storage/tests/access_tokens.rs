//! Access token persistence against a real, migrated SQLite database.

use rustit_core::{AccessTokenId, Timestamp, TokenScope, User};
use rustit_storage::{Database, NewAccessToken, NewUser};

async fn open_database(dir: &tempfile::TempDir) -> Database {
    Database::open(&dir.path().join("rustit.db")).await.unwrap()
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

fn new_token(owner: &User, name: &str, digest_byte: u8) -> NewAccessToken {
    NewAccessToken {
        account_id: owner.id(),
        name: name.to_owned(),
        digest: [digest_byte; 32],
        hint: "abcd".to_owned(),
        scope: TokenScope::Write,
        expires_at: Some(Timestamp(4_000_000_000)),
    }
}

#[tokio::test]
async fn creates_lists_and_finds_tokens_by_digest() {
    let dir = tempfile::tempdir().unwrap();
    let database = open_database(&dir).await;
    let store = database.access_tokens();
    let alice = create_user(&database, "alice").await;

    let first = store.create(new_token(&alice, "laptop", 1)).await.unwrap();
    let second = store.create(new_token(&alice, "ci", 2)).await.unwrap();
    assert_eq!(first.name, "laptop");
    assert_eq!(first.scope, TokenScope::Write);
    assert_eq!(first.expires_at, Some(Timestamp(4_000_000_000)));
    assert!(first.last_used_at.is_none());

    let listed: Vec<_> = store.list_for_account(alice.id()).await.unwrap().into_iter().map(|t| t.id).collect();
    assert_eq!(listed, vec![second.id, first.id], "newest first");
    assert_eq!(store.count_for_account(alice.id()).await.unwrap(), 2);

    let (found, owner) = store.find_by_digest(&[1; 32]).await.unwrap().unwrap();
    assert_eq!(found.id, first.id);
    assert_eq!(owner.id(), alice.id());
    assert_eq!(owner.name().as_str(), "alice");
    assert!(store.find_by_digest(&[9; 32]).await.unwrap().is_none());
}

#[tokio::test]
async fn duplicate_digest_is_a_conflict() {
    let dir = tempfile::tempdir().unwrap();
    let database = open_database(&dir).await;
    let alice = create_user(&database, "alice").await;

    database.access_tokens().create(new_token(&alice, "one", 1)).await.unwrap();
    let duplicate = database.access_tokens().create(new_token(&alice, "two", 1)).await;
    assert!(matches!(duplicate, Err(rustit_storage::StorageError::Conflict(_))));
}

#[tokio::test]
async fn only_the_owner_can_delete() {
    let dir = tempfile::tempdir().unwrap();
    let database = open_database(&dir).await;
    let store = database.access_tokens();
    let alice = create_user(&database, "alice").await;
    let mallory = create_user(&database, "mallory").await;
    let token = store.create(new_token(&alice, "laptop", 1)).await.unwrap();

    assert!(!store.delete(mallory.id(), token.id).await.unwrap());
    assert!(store.find_by_digest(&[1; 32]).await.unwrap().is_some());

    assert!(store.delete(alice.id(), token.id).await.unwrap());
    assert!(!store.delete(alice.id(), token.id).await.unwrap());
    assert!(store.find_by_digest(&[1; 32]).await.unwrap().is_none());
    assert!(!store.delete(alice.id(), AccessTokenId(9999)).await.unwrap());
}

#[tokio::test]
async fn touch_is_throttled_to_once_an_hour() {
    let dir = tempfile::tempdir().unwrap();
    let database = open_database(&dir).await;
    let store = database.access_tokens();
    let alice = create_user(&database, "alice").await;
    let token = store.create(new_token(&alice, "laptop", 1)).await.unwrap();
    let last_used = || async { store.find_by_digest(&[1; 32]).await.unwrap().unwrap().0.last_used_at };

    store.touch(token.id, Timestamp(1_000_000)).await.unwrap();
    assert_eq!(last_used().await, Some(Timestamp(1_000_000)));

    store.touch(token.id, Timestamp(1_000_000 + 3_599)).await.unwrap();
    assert_eq!(last_used().await, Some(Timestamp(1_000_000)), "within the hour: unchanged");

    store.touch(token.id, Timestamp(1_000_000 + 3_600)).await.unwrap();
    assert_eq!(last_used().await, Some(Timestamp(1_000_000 + 3_600)));
}
