use super::*;

#[tokio::test]
async fn pristine_zero_history_seeds_zero_and_each_committed_value_is_unique() {
    let pool = legacy().await;
    upgrade(&pool).await;
    assert_eq!(high_water(&pool).await, 0);
    let counter = crate::query_count::QueryCounter::new();
    let mut tx = pool.begin().await.unwrap();
    let first = counter.measure(allocate_in(&mut tx)).await.unwrap();
    assert_eq!(counter.queries().len(), 1, "one statement on valid state, no account lookup");
    tx.commit().await.unwrap();
    assert_eq!(first.get(), 1);
    assert_eq!(allocate(&pool).await.get(), 2);
}

#[tokio::test]
async fn provisional_allocations_rollback_and_commit_failure_cannot_publish_authority() {
    let pool = legacy().await;
    upgrade(&pool).await;
    let mut tx = pool.begin().await.unwrap();
    let provisional = allocate_in(&mut tx).await.unwrap();
    tx.rollback().await.unwrap();
    assert_eq!(high_water(&pool).await, 0);
    assert_eq!(allocate(&pool).await, provisional, "rolled-back values are DATA, never committed receipts");
    sqlx::raw_sql("CREATE TABLE bad_commit(account_id INTEGER REFERENCES accounts(id) DEFERRABLE INITIALLY DEFERRED) STRICT;")
        .execute(&pool)
        .await
        .unwrap();
    let mut tx = pool.begin().await.unwrap();
    let uncommitted = allocate_in(&mut tx).await.unwrap();
    sqlx::query("INSERT INTO bad_commit VALUES(999)").execute(&mut *tx).await.unwrap();
    assert!(tx.commit().await.is_err());
    assert_eq!(high_water(&pool).await, 1);
    assert_eq!(allocate(&pool).await, uncommitted);
}

#[tokio::test]
async fn max_integer_is_checked_never_real_or_wrapped_and_floor_never_lowers() {
    let pool = legacy().await;
    upgrade(&pool).await;
    let mut tx = pool.begin().await.unwrap();
    assert_eq!(raise_floor_in(&mut tx, CredentialEpoch::new(12).unwrap()).await.unwrap().get(), 12);
    assert_eq!(raise_floor_in(&mut tx, CredentialEpoch::new(3).unwrap()).await.unwrap().get(), 12);
    assert!(matches!(raise_floor_in(&mut tx, CredentialEpoch::new(i64::MAX).unwrap()).await, Err(EpochAllocationError::Exhausted)));
    tx.commit().await.unwrap();
    assert_eq!(high_water(&pool).await, 12);
    sqlx::query("UPDATE credential_epoch_allocator SET high_water=?").bind(i64::MAX - 1).execute(&pool).await.unwrap();
    assert_eq!(allocate(&pool).await.get(), i64::MAX);
    let mut tx = pool.begin().await.unwrap();
    assert!(matches!(allocate_in(&mut tx).await, Err(EpochAllocationError::Exhausted)));
    tx.rollback().await.unwrap();
    assert_eq!(high_water(&pool).await, i64::MAX);
}

#[tokio::test]
async fn missing_duplicate_wrong_singleton_and_malformed_types_never_auto_repair() {
    for rows in ["", "(1,0),(1,0)", "(2,0)", "(1,-1)", "(1,'unknown')"] {
        let pool = legacy().await;
        upgrade(&pool).await;
        sqlx::raw_sql("DROP TABLE credential_epoch_allocator;CREATE TABLE credential_epoch_allocator(singleton,high_water);")
            .execute(&pool)
            .await
            .unwrap();
        match rows {
            "" => {}
            "(1,0),(1,0)" => {
                sqlx::query("INSERT INTO credential_epoch_allocator VALUES(1,0),(1,0)").execute(&pool).await.unwrap();
            }
            "(2,0)" => {
                sqlx::query("INSERT INTO credential_epoch_allocator VALUES(2,0)").execute(&pool).await.unwrap();
            }
            "(1,-1)" => {
                sqlx::query("INSERT INTO credential_epoch_allocator VALUES(1,-1)").execute(&pool).await.unwrap();
            }
            "(1,'unknown')" => {
                sqlx::query("INSERT INTO credential_epoch_allocator VALUES(1,'unknown')").execute(&pool).await.unwrap();
            }
            _ => unreachable!(),
        }
        let before: Vec<(String, String)> =
            sqlx::query_as("SELECT quote(singleton),quote(high_water) FROM credential_epoch_allocator ORDER BY rowid")
                .fetch_all(&pool)
                .await
                .unwrap();
        let mut tx = pool.begin().await.unwrap();
        assert!(matches!(allocate_in(&mut tx).await, Err(EpochAllocationError::InvalidState)));
        assert!(matches!(raise_floor_in(&mut tx, CredentialEpoch::new(5).unwrap()).await, Err(EpochAllocationError::InvalidState)));
        tx.rollback().await.unwrap();
        let after: Vec<(String, String)> =
            sqlx::query_as("SELECT quote(singleton),quote(high_water) FROM credential_epoch_allocator ORDER BY rowid")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(after, before);
    }
}

#[tokio::test]
async fn committed_counter_survives_reopen_and_same_id_recreation_rejects_retained_redb_epoch() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("epochs.db");
    let options = rbit_storage::sqlite::writer_compatibility::register(SqliteConnectOptions::new())
        .filename(&path)
        .create_if_missing(true)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new().max_connections(1).connect_with(options.clone()).await.unwrap();
    sqlx::migrate!("./migrations").run_to(60, &pool).await.unwrap();
    seed_users(&pool).await;
    let mut tx = pool.begin().await.unwrap();
    let old = intents::stage(&mut tx, AccountId(1), Timestamp(20)).await.unwrap();
    tx.commit().await.unwrap();
    let kv = rbit_storage::KvStore::open(&dir.path().join("sessions.redb")).unwrap();
    let sessions = kv.sessions().epoch_sidecar();
    let now = Timestamp::now();
    let record = rbit_storage::SessionRecord::at(AccountId(1), Timestamp(now.0 + 3600), None, "fixture", now);
    sessions.put_if_absent([1; 32], record, AccountId(1), CredentialEpoch::new(old.target_epoch).unwrap()).await.unwrap();
    sqlx::query("DELETE FROM accounts WHERE id=1").execute(&pool).await.unwrap();
    upgrade(&pool).await;
    assert_eq!(high_water(&pool).await, old.target_epoch);
    let mut tx = pool.begin().await.unwrap();
    let fresh = allocate_in(&mut tx).await.unwrap();
    sqlx::query("INSERT INTO accounts(id,kind,name,created_at,updated_at) VALUES(1,'user','recreated',30,30)")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("INSERT INTO user_logins(account_id,email,password_hash,is_admin,authentication_epoch) VALUES(1,'recreated@example.com','new hash',0,?)")
        .bind(fresh.get()).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    let retained = sessions.get_live([1; 32]).await.unwrap().unwrap();
    assert!(retained.epoch() < fresh);
    let accepts: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM user_logins WHERE account_id=1 AND authentication_epoch=?)")
        .bind(retained.epoch().get())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(!accepts);
    pool.close().await;
    let reopened = SqlitePoolOptions::new().max_connections(1).connect_with(options).await.unwrap();
    upgrade(&reopened).await;
    assert!(allocate(&reopened).await > fresh);
    reopened.close().await;
}

#[tokio::test]
async fn independent_connection_pools_serialize_committed_allocations_without_reuse() {
    let directory = tempfile::tempdir().unwrap();
    let options = rbit_storage::sqlite::writer_compatibility::register(SqliteConnectOptions::new())
        .filename(directory.path().join("concurrent.db"))
        .create_if_missing(true)
        .foreign_keys(true);
    let first = SqlitePoolOptions::new().max_connections(1).connect_with(options.clone()).await.unwrap();
    sqlx::migrate!("./migrations").run(&first).await.unwrap();
    let second = SqlitePoolOptions::new().max_connections(1).connect_with(options).await.unwrap();
    let (a, b) = tokio::join!(allocate(&first), allocate(&second));
    assert_ne!(a, b);
    assert_eq!(a.get().min(b.get()), 1);
    assert_eq!(a.get().max(b.get()), 2);
    assert_eq!(high_water(&first).await, 2);
    assert_eq!(high_water(&second).await, 2);
    first.close().await;
    second.close().await;
}
