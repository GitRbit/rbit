use super::*;

#[tokio::test]
async fn valid_committed_local_history_is_bounded_by_sequence_even_after_cleanup_and_account_delete() {
    let pool = legacy().await;
    let mut epochs = [0_i64; 2];
    let mut largest = 0;
    for step in 1..=24_i64 {
        let account = if step % 3 == 0 { 2 } else { 1 };
        let mut tx = pool.begin().await.unwrap();
        let intent = intents::stage(&mut tx, AccountId(account), Timestamp(step)).await.unwrap();
        epochs[usize::try_from(account - 1).unwrap()] += 1;
        assert_eq!(intent.target_epoch, epochs[usize::try_from(account - 1).unwrap()]);
        assert!(intent.id >= intent.target_epoch);
        largest = intent.id;
        intents::complete(&mut tx, &intent, intents::RevocationOutcome::EndedAll, Timestamp(step)).await.unwrap();
        tx.commit().await.unwrap();
    }
    sqlx::query("DELETE FROM credential_revocations WHERE completed_at IS NOT NULL").execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM accounts WHERE id=1").execute(&pool).await.unwrap();
    let sequence: i64 =
        sqlx::query_scalar("SELECT seq FROM sqlite_sequence WHERE name='credential_revocations'").fetch_one(&pool).await.unwrap();
    assert_eq!(sequence, largest);
    let mut tx = pool.begin().await.unwrap();
    let provisional = intents::stage(&mut tx, AccountId(2), Timestamp(30)).await.unwrap();
    tx.rollback().await.unwrap();
    assert!(provisional.id > largest);
    upgrade(&pool).await;
    assert_eq!(high_water(&pool).await, largest);
    assert!(allocate(&pool).await.get() > largest);
}

#[tokio::test]
async fn unknown_sequence_and_unsupported_manual_epoch_histories_refuse_seed() {
    for kind in
        ["missing", "duplicate", "zero", "negative", "text", "epoch-ahead", "target-ahead", "negative-id", "negative-epoch", "exhausted"]
    {
        let pool = legacy().await;
        let mut tx = pool.begin().await.unwrap();
        intents::stage(&mut tx, AccountId(1), Timestamp(20)).await.unwrap();
        tx.commit().await.unwrap();
        match kind {
            "missing" => {
                sqlx::query("DELETE FROM sqlite_sequence WHERE name='credential_revocations'").execute(&pool).await.unwrap();
            }
            "duplicate" => {
                sqlx::query("INSERT INTO sqlite_sequence(name,seq) VALUES('credential_revocations',1)").execute(&pool).await.unwrap();
            }
            "zero" => {
                sqlx::query("UPDATE sqlite_sequence SET seq=0 WHERE name='credential_revocations'").execute(&pool).await.unwrap();
            }
            "negative" => {
                sqlx::query("UPDATE sqlite_sequence SET seq=-1 WHERE name='credential_revocations'").execute(&pool).await.unwrap();
            }
            "text" => {
                sqlx::query("UPDATE sqlite_sequence SET seq='unknown' WHERE name='credential_revocations'").execute(&pool).await.unwrap();
            }
            "epoch-ahead" => {
                sqlx::query("UPDATE user_logins SET authentication_epoch=2 WHERE account_id=1").execute(&pool).await.unwrap();
            }
            "target-ahead" => {
                sqlx::query("UPDATE credential_revocations SET target_epoch=2").execute(&pool).await.unwrap();
            }
            "negative-id" => {
                sqlx::query("UPDATE credential_revocations SET id=-1").execute(&pool).await.unwrap();
            }
            "negative-epoch" => {
                sqlx::query("PRAGMA ignore_check_constraints=ON").execute(&pool).await.unwrap();
                sqlx::query("UPDATE user_logins SET authentication_epoch=-1").execute(&pool).await.unwrap();
                sqlx::query("PRAGMA ignore_check_constraints=OFF").execute(&pool).await.unwrap();
            }
            "exhausted" => {
                sqlx::query("UPDATE sqlite_sequence SET seq=? WHERE name='credential_revocations'")
                    .bind(i64::MAX)
                    .execute(&pool)
                    .await
                    .unwrap();
            }
            _ => unreachable!(),
        }
        let error = sqlx::migrate!("./migrations").run(&pool).await.unwrap_err();
        let reason = error.to_string();
        assert!(
            reason.contains(if kind == "exhausted" { "credential_epoch_space_exhausted" } else { "credential_epoch_history_invalid" }),
            "{kind}: {reason}"
        );
        let created: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='credential_epoch_allocator')")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert!(!created, "failed migration must not leave a usable partial allocator");
    }
}

#[tokio::test]
async fn erased_sql_history_is_not_a_readiness_proof_and_observed_orphan_floor_only_raises() {
    let dir = tempfile::tempdir().unwrap();
    let pool = legacy().await;
    // Privileged manual erasure cannot be distinguished from pristine SQL history alone.
    let kv = rbit_storage::KvStore::open(&dir.path().join("orphan.redb")).unwrap();
    let sessions = kv.sessions().epoch_sidecar();
    let now = Timestamp::now();
    let record = rbit_storage::SessionRecord::at(AccountId(99), Timestamp(now.0 + 3600), None, "fixture", now);
    sessions.put_if_absent([9; 32], record, AccountId(99), CredentialEpoch::new(50).unwrap()).await.unwrap();
    upgrade(&pool).await;
    assert_eq!(high_water(&pool).await, 0);
    let observed = sessions.get_live([9; 32]).await.unwrap().unwrap();
    assert!(observed.epoch().get() > high_water(&pool).await, "SQL seed is insufficient; activation must remain quarantined");
    let mut tx = pool.begin().await.unwrap();
    assert_eq!(raise_floor_in(&mut tx, observed.epoch()).await.unwrap(), observed.epoch());
    tx.commit().await.unwrap();
    assert!(allocate(&pool).await > observed.epoch());
    // Raising one observed DATA fact still does not establish complete-scan/restore
    // acceptance or enable a session/legacy-writer mode; this atom has no such API.
}
