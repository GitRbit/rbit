//! Sealed credentials against a real, migrated `SQLite` database: where the key lives, what
//! happens when it is missing or wrong, and that sealed values fit their columns.

use rbit_core::{Timestamp, Visibility, push_mirror::MAX_SECRET_BYTES};
use rbit_storage::{
    Database, NewRepository, NewUser, SealingKey, StorageError, sealing::KEY_FILE_NAME, sqlite::push_mirrors::NewPushMirror,
};

#[tokio::test]
async fn the_key_file_is_created_next_to_the_database_or_where_configured() {
    let dir = tempfile::tempdir().unwrap();
    let database = Database::open(&dir.path().join("rbit.db")).await.unwrap();
    database.close().await;
    assert!(dir.path().join(KEY_FILE_NAME).is_file());

    let other = tempfile::tempdir().unwrap();
    let key_file = other.path().join("keys").join("rbit.key");
    let database = Database::open_with_key_file(&other.path().join("rbit.db"), &key_file).await.unwrap();
    database.server_secrets().get_or_insert("links", b"value").await.unwrap();
    database.close().await;
    assert!(key_file.is_file());
    assert!(!other.path().join(KEY_FILE_NAME).exists(), "no second key in the data directory");

    let reopened = Database::open_with_key_file(&other.path().join("rbit.db"), &key_file).await.unwrap();
    assert_eq!(reopened.server_secrets().get_or_insert("links", b"other").await.unwrap(), b"value");
    reopened.close().await;
}

#[tokio::test]
async fn a_lost_key_is_never_silently_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rbit.db");
    let database = Database::open(&path).await.unwrap();
    database.server_secrets().get_or_insert("links", b"value").await.unwrap();
    database.close().await;

    let key_file = dir.path().join(KEY_FILE_NAME);
    std::fs::remove_file(&key_file).unwrap();
    assert!(matches!(Database::open(&path).await, Err(StorageError::MissingKey(_))));
    assert!(!key_file.exists(), "no new key is generated over sealed data");

    SealingKey::create(&key_file).unwrap();
    assert!(matches!(Database::open(&path).await, Err(StorageError::WrongKey(_))));
}

#[tokio::test]
async fn the_longest_push_mirror_secret_fits_sealed() {
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
    let repo = database
        .repositories()
        .create(NewRepository {
            owner_id: user.id(),
            name: "rocket".parse().unwrap(),
            description: None,
            visibility: Visibility::Public,
            default_branch: "main".to_owned(),
        })
        .await
        .unwrap();
    let secret = "x".repeat(MAX_SECRET_BYTES);
    let mirror = database
        .push_mirrors()
        .create(&NewPushMirror {
            repo: repo.id,
            url: "https://example.com/rocket",
            username: "bot",
            secret: Some(&secret),
            sync_on_push: true,
            created_by: user.id(),
        })
        .await
        .unwrap();
    let target = database.push_mirrors().target(mirror.id).await.unwrap().unwrap();
    assert_eq!(target.secret.as_deref(), Some(secret.as_str()));
    assert!(target.mirror.next_sync_at <= Timestamp::now());
    database.close().await;
}
