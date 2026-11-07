use super::*;
use crate::kv::sessions::epoch::stopped as kv;
use crate::{Database, KvStore};
use redb::{ReadableDatabase, ReadableTableMetadata};

#[tokio::test]
async fn complete_upgrade_scans_orphans_rebases_distinct_epochs_and_preserves_credentials() {
    let f = Fixture::new().await;
    f.orphan_epoch(500);
    let prepared = f.owner().await.prepare().await.unwrap();
    let summary = prepared.summary();
    assert_eq!((summary.observed_epoch_floor, summary.rebased_accounts, summary.final_epoch_floor), (500, 2, 502));
    assert_eq!(f.hashes_and_epochs().await, [("unchanged PHC".into(), 501), ("unchanged PHC".into(), 502)]);
    assert_eq!(format!("{prepared:?}"), "PreparedIdentityPair { .. }");
    assert!(StoppedIdentityUpgrade::open(&f.sql, &f.kv, limits()).await.is_err(), "acceptance retains the actual exclusive KV owner");
    drop(prepared);
    assert!(Database::open(&f.sql).await.is_err());
    assert!(KvStore::open(&f.kv).is_err());
    let accepted = f.owner().await.accept_prepared().await.unwrap();
    assert_eq!(accepted.summary(), summary);
}
#[tokio::test]
async fn interrupted_after_sql_preparing_resumes_the_same_checkpoint_without_legacy_cleanup() {
    let f = Fixture::new().await;
    f.orphan_epoch(500);
    let mut owner = f.owner().await;
    let checkpoint = owner.begin_preparing().await.unwrap();
    assert_eq!(f.state().await, "preparing");
    let tx = owner.db.begin_read().unwrap();
    assert_eq!(tx.open_table(LEGACY).unwrap().len().unwrap(), 1);
    drop(tx);
    drop(owner);
    let prepared = f.owner().await.prepare().await.unwrap();
    assert_eq!(prepared.summary().observed_epoch_floor, checkpoint.floor);
    assert!(prepared.checkpoint == checkpoint);
}
#[tokio::test]
async fn interrupted_after_kv_prepare_resumes_without_regenerating_or_double_allocating() {
    let f = Fixture::new().await;
    let mut owner = f.owner().await;
    let c = owner.begin_preparing().await.unwrap();
    kv::prepare(&owner.db, c, limits()).unwrap();
    drop(owner);
    let prepared = f.owner().await.prepare().await.unwrap();
    let expected = prepared.summary();
    assert!(prepared.checkpoint == c);
    drop(prepared);
    let prepared = f.owner().await.prepare().await.unwrap();
    assert_eq!(prepared.summary(), expected);
    assert!(prepared.checkpoint == c);
}
#[tokio::test]
async fn rebase_failure_rolls_back_all_accounts_counter_and_prepared_mode_then_resumes() {
    let f = Fixture::new().await;
    sqlx::query("CREATE TRIGGER fixture_fail_rebase BEFORE UPDATE OF authentication_epoch ON user_logins WHEN NEW.account_id=2 BEGIN SELECT RAISE(ABORT,'fixture'); END").execute(&mut f.connection().await).await.unwrap();
    assert!(f.owner().await.prepare().await.is_err());
    assert_eq!(f.state().await, "preparing");
    assert_eq!(f.hashes_and_epochs().await, [("unchanged PHC".into(), 0), ("unchanged PHC".into(), 0)]);
    let counter: i64 =
        sqlx::query_scalar("SELECT high_water FROM credential_epoch_allocator").fetch_one(&mut f.connection().await).await.unwrap();
    assert_eq!(counter, 0);
    sqlx::query("DROP TRIGGER fixture_fail_rebase").execute(&mut f.connection().await).await.unwrap();
    let prepared = f.owner().await.prepare().await.unwrap();
    assert_eq!(prepared.summary().final_epoch_floor, 2);
}
#[tokio::test]
async fn prepared_sql_with_rolled_back_kv_is_refused_without_recreating_a_checkpoint() {
    let f = Fixture::new().await;
    let backup = f.directory.path().join("before.redb");
    std::fs::copy(&f.kv, &backup).unwrap();
    drop(f.owner().await.prepare().await.unwrap());
    std::fs::copy(&backup, &f.kv).unwrap();
    assert!(matches!(f.owner().await.prepare().await.unwrap_err().error(), IdentityUpgradeError::Mismatch));
    assert_eq!(f.state().await, "prepared");
    let db = redb::Database::open(&f.kv).unwrap();
    assert!(kv::mode(&db).unwrap().is_none());
}

#[tokio::test]
async fn accepted_cutoff_retains_old_pending_facts_without_forging_a_cleanup_ack() {
    use rbit_core::{AccountId, Timestamp};
    use sqlx::Connection;
    let f = Fixture::new().await;
    let mut connection = f.connection().await;
    let mut tx = connection.begin().await.unwrap();
    let intent = crate::sqlite::credential_revocations::stage(&mut tx, AccountId(1), Timestamp(1)).await.unwrap();
    tx.commit().await.unwrap();
    let prepared = f.owner().await.prepare().await.unwrap();
    assert_eq!(prepared.summary().retired_intents_through, intent.id);
    let row: (Option<i64>, Option<String>, Option<Vec<u8>>) =
        sqlx::query_as("SELECT completed_at,outcome,cleanup_pair FROM credential_revocations WHERE id=?")
            .bind(intent.id)
            .fetch_one(&mut connection)
            .await
            .unwrap();
    assert_eq!(row, (None, None, None));
    assert_eq!(f.hashes_and_epochs().await, [("unchanged PHC".into(), 2), ("unchanged PHC".into(), 3)]);
}
#[tokio::test]
async fn surviving_binding_of_a_deleted_sql_account_still_raises_the_cutover_floor() {
    use rbit_core::{AccountId, Timestamp};
    use rbit_identity_models::credential::CredentialEpoch;
    let f = Fixture::new().await;
    {
        let store = KvStore::open(&f.kv).unwrap();
        let now = Timestamp::now();
        store
            .sessions()
            .epoch_sidecar()
            .put_bound(
                [7; 32],
                crate::SessionRecord::at(AccountId(2), Timestamp(now.0 + 3600), None, "deleted", now),
                AccountId(2),
                CredentialEpoch::new(500).unwrap(),
            )
            .await
            .unwrap();
    }
    sqlx::query("DELETE FROM accounts WHERE id=2").execute(&mut f.connection().await).await.unwrap();
    let prepared = f.owner().await.prepare().await.unwrap();
    assert_eq!(prepared.summary().observed_epoch_floor, 500);
    assert_eq!(prepared.summary().rebased_accounts, 1);
    assert_eq!(prepared.summary().final_epoch_floor, 501);
}

#[tokio::test]
async fn independently_prepared_pairs_cannot_be_mixed_even_when_summary_counts_match() {
    let a = Fixture::new().await;
    let b = Fixture::new().await;
    let prepared_a = a.owner().await.prepare().await.unwrap();
    let prepared_b = b.owner().await.prepare().await.unwrap();
    assert_eq!(prepared_a.summary(), prepared_b.summary());
    let checkpoint_b = prepared_b.checkpoint;
    drop(prepared_a);
    drop(prepared_b);
    std::fs::copy(&b.kv, &a.kv).unwrap();
    assert!(matches!(a.owner().await.prepare().await.unwrap_err().error(), IdentityUpgradeError::Mismatch));
    assert_eq!(a.state().await, "prepared");
    let db = redb::Database::open(&a.kv).unwrap();
    assert!(kv::mode(&db).unwrap() == Some(checkpoint_b));
}
