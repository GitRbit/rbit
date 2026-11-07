use super::*;
use crate::sealing::SealingKey;

#[tokio::test]
async fn accepted_pool_preserves_missing_and_wrong_key_refusals_without_new_key() {
    let f = Fixture::new().await;
    let mut owner = prepared(&f).await;
    owner.publish_active_inner().await.unwrap();
    let mut accepted = owner.accept_active_inner().await.unwrap();
    let original_key = f.sql.with_file_name(crate::sealing::KEY_FILE_NAME);
    let key = SealingKey::load(&original_key).unwrap().unwrap();
    let secret = key.seal(&[7; 20]).unwrap().into_bytes();
    sqlx::query("INSERT INTO two_factor(account_id,secret,created_at) VALUES(1,?,0)")
        .bind(&secret)
        .execute(&mut accepted.owner.connection)
        .await
        .unwrap();
    let missing = f.directory.path().join("missing.key");
    assert!(matches!(accepted.database(&missing).await, Err(StorageError::MissingKey(_))));
    assert!(!missing.exists());
    let wrong = f.directory.path().join("wrong.key");
    SealingKey::create(&wrong).unwrap();
    assert!(matches!(accepted.database(&wrong).await, Err(StorageError::WrongKey(_))));
    let stored: Vec<u8> =
        sqlx::query_scalar("SELECT secret FROM two_factor WHERE account_id=1").fetch_one(&mut accepted.owner.connection).await.unwrap();
    assert_eq!(stored, secret, "key refusal cannot reseal or replace the existing credential");
    assert!(crate::KvStore::open(&f.kv).is_err(), "accepted owner is retained across pool/key failure");
}

#[tokio::test]
async fn plaintext_legacy_secrets_are_sealed_only_after_accepted_pair() {
    let f = Fixture::new().await;
    let mut owner = prepared(&f).await;
    owner.publish_active_inner().await.unwrap();
    let mut accepted = owner.accept_active_inner().await.unwrap();
    sqlx::query("INSERT INTO two_factor(account_id,secret,created_at) VALUES(1,?,0)")
        .bind([7_u8; 20].as_slice())
        .execute(&mut accepted.owner.connection)
        .await
        .unwrap();
    let new_key = f.directory.path().join("new-accepted.key");
    let database = accepted.database(&new_key).await.unwrap();
    assert!(new_key.exists());
    let stored: Vec<u8> =
        sqlx::query_scalar("SELECT secret FROM two_factor WHERE account_id=1").fetch_one(&mut accepted.owner.connection).await.unwrap();
    assert!(stored.starts_with(crate::sealing::PREFIX.as_bytes()));
    assert_eq!(database.sealing_key().open(&stored).unwrap(), [7_u8; 20]);
    database.close().await;
}
