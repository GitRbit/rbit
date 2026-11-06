use std::num::NonZeroU8;

use rbit_core::{AccountId, Timestamp};
use rbit_storage::{Database, sqlite::credential_revocations as intents};
use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};

use super::{database, seed};

#[tokio::test]
async fn upgrade_preserves_existing_accounts_and_passwords_with_zero_epochs() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("upgrade.db");
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::new().filename(&path).create_if_missing(true).foreign_keys(true))
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run_to(59, &pool).await.unwrap();
    seed(&pool).await;
    let before: Vec<(i64, String, String, i64)> =
        sqlx::query_as("SELECT account_id,email,password_hash,is_admin FROM user_logins ORDER BY account_id")
            .fetch_all(&pool)
            .await
            .unwrap();
    pool.close().await;
    let backup = dir.path().join("before-0060.db");
    std::fs::copy(&path, &backup).unwrap(); // Closed fixture only; never copy a live database.
    let database = Database::open(&path).await.unwrap();
    assert_eq!(database.accounts().find_login("alice").await.unwrap().unwrap().password_hash, "old hash");
    database.close().await;
    let pool = reopen(&path).await;
    let after: Vec<(i64, String, String, i64)> =
        sqlx::query_as("SELECT account_id,email,password_hash,is_admin FROM user_logins ORDER BY account_id")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(before, after);
    let epochs: Vec<i64> =
        sqlx::query_scalar("SELECT authentication_epoch FROM user_logins ORDER BY account_id").fetch_all(&pool).await.unwrap();
    assert_eq!(epochs, [0, 0]);
    let mut older = sqlx::migrate!("./migrations");
    older.migrations.to_mut().retain(|migration| migration.version <= 59);
    assert!(matches!(older.run(&pool).await, Err(sqlx::migrate::MigrateError::VersionMissing(60))));
    // Inactive fencing still changes the schema version. A stopped pre-upgrade copy
    // can pass the older migrator without rewriting migration history or user data.
    let restored = reopen(&backup).await;
    older.run(&restored).await.unwrap();
    let restored_rows: Vec<(i64, String, String, i64)> =
        sqlx::query_as("SELECT account_id,email,password_hash,is_admin FROM user_logins ORDER BY account_id")
            .fetch_all(&restored)
            .await
            .unwrap();
    assert_eq!(restored_rows, before);
    restored.close().await;
    let mut tx = pool.begin().await.unwrap();
    let intent = intents::stage(&mut tx, AccountId(1), Timestamp(20)).await.unwrap();
    tx.commit().await.unwrap();
    pool.close().await;
    let reopened = reopen(&path).await;
    let mut tx = reopened.begin().await.unwrap();
    assert_eq!(intents::pending(&mut tx, 0, NonZeroU8::new(10).unwrap()).await.unwrap(), [intent]);
    let epoch: i64 =
        sqlx::query_scalar("SELECT authentication_epoch FROM user_logins WHERE account_id=1").fetch_one(&mut *tx).await.unwrap();
    assert_eq!(epoch, 1);
    tx.commit().await.unwrap();
    assert!(sqlx::query("PRAGMA foreign_key_check").fetch_all(&reopened).await.unwrap().is_empty());
    let integrity: String = sqlx::query_scalar("PRAGMA integrity_check").fetch_one(&reopened).await.unwrap();
    assert_eq!(integrity, "ok");
    reopened.close().await;
}

async fn reopen(path: &std::path::Path) -> SqlitePool {
    SqlitePoolOptions::new().max_connections(1).connect_with(SqliteConnectOptions::new().filename(path).foreign_keys(true)).await.unwrap()
}

#[tokio::test]
async fn legacy_credential_writes_remain_inactive_until_protocol_activation() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("inactive.db");
    let database = Database::open(&path).await.unwrap();
    let raw = reopen(&path).await;
    seed(&raw).await;
    database.profiles().set_password_hash(AccountId(1), "legacy replacement").await.unwrap();
    let current: (String, i64, i64) = sqlx::query_as(
        "SELECT password_hash,authentication_epoch,(SELECT count(*) FROM credential_revocations) FROM user_logins WHERE account_id=1",
    )
    .fetch_one(&raw)
    .await
    .unwrap();
    assert_eq!(current, ("legacy replacement".into(), 0, 0));
    let changed = database.profiles().set_password_hash_if_current(AccountId(1), "rejected", "stale hash").await.unwrap();
    assert!(!changed);
    raw.close().await;
    database.close().await;
}

#[tokio::test]
async fn strict_schema_rejects_invalid_fences_outcomes_and_dangling_accounts() {
    let pool = database().await;
    for statement in [
        "UPDATE user_logins SET authentication_epoch=-1 WHERE account_id=1",
        "UPDATE user_logins SET authentication_epoch=1.5 WHERE account_id=1",
        "UPDATE user_logins SET authentication_epoch='bad' WHERE account_id=1",
        "INSERT INTO credential_revocations(account_id,target_epoch,created_at) VALUES(1,0,10)",
        "INSERT INTO credential_revocations(account_id,target_epoch,created_at) VALUES(999,1,10)",
        "INSERT INTO credential_revocations(account_id,target_epoch,created_at,outcome) VALUES(1,1,10,'kept_current')",
        "INSERT INTO credential_revocations(account_id,target_epoch,created_at,completed_at) VALUES(1,1,10,11)",
        "INSERT INTO credential_revocations(account_id,target_epoch,created_at,outcome,completed_at) VALUES(1,1,10,'unknown',11)",
    ] {
        // Every SQL value is a fixed fixture literal; no user-controlled SQL is evaluated.
        assert!(sqlx::raw_sql(statement).execute(&pool).await.is_err());
    }
    let mut tx = pool.begin().await.unwrap();
    intents::stage(&mut tx, AccountId(1), Timestamp(10)).await.unwrap();
    assert!(
        sqlx::query("INSERT INTO credential_revocations(account_id,target_epoch,created_at) VALUES(1,1,10)")
            .execute(&mut *tx)
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn failed_outer_commit_does_not_publish_password_or_intent() {
    let pool = database().await;
    sqlx::raw_sql("CREATE TABLE commit_failure(account_id INTEGER REFERENCES accounts(id) DEFERRABLE INITIALLY DEFERRED) STRICT;")
        .execute(&pool)
        .await
        .unwrap();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("UPDATE user_logins SET password_hash='not committed' WHERE account_id=1").execute(&mut *tx).await.unwrap();
    intents::stage(&mut tx, AccountId(1), Timestamp(20)).await.unwrap();
    sqlx::query("INSERT INTO commit_failure(account_id) VALUES(999)").execute(&mut *tx).await.unwrap();
    assert!(tx.commit().await.is_err());
    assert_eq!(super::state(&pool).await, ("old hash".into(), 0, None, 0));
}
