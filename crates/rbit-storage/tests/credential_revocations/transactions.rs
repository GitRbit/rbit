use std::num::NonZeroU8;

use rbit_core::{AccountId, Timestamp};
use rbit_storage::{
    StorageError,
    sqlite::credential_revocations::{self as intents, Completion, RevocationOutcome},
};

use super::{database, state, stored};

#[tokio::test]
async fn credential_proof_epoch_and_intent_commit_together() {
    let pool = database().await;
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("UPDATE user_logins SET password_hash='new hash',password_change_required=0 WHERE account_id=1")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("UPDATE email_tokens SET used_at=20 WHERE account_id=1").execute(&mut *tx).await.unwrap();
    let intent = intents::stage(&mut tx, AccountId(1), Timestamp(20)).await.unwrap();
    assert_eq!(intent.target_epoch, 1);
    tx.commit().await.unwrap();
    assert_eq!(state(&pool).await, ("new hash".into(), 1, Some(20), 1));
    assert_eq!(stored(&pool, intent).await, (None, None));
    let mut reader = pool.begin().await.unwrap();
    assert_eq!(intents::pending(&mut reader, 0, NonZeroU8::new(10).unwrap()).await.unwrap(), [intent]);
    reader.commit().await.unwrap();
}

#[tokio::test]
async fn dropped_or_explicitly_rolled_back_credentials_leave_no_fence_or_intent() {
    let pool = database().await;
    for explicit in [false, true] {
        let mut tx = pool.begin().await.unwrap();
        sqlx::query("UPDATE user_logins SET password_hash='uncommitted' WHERE account_id=1").execute(&mut *tx).await.unwrap();
        sqlx::query("UPDATE email_tokens SET used_at=20 WHERE account_id=1").execute(&mut *tx).await.unwrap();
        intents::stage(&mut tx, AccountId(1), Timestamp(20)).await.unwrap();
        if explicit {
            tx.rollback().await.unwrap();
        } else {
            drop(tx);
        }
        assert_eq!(state(&pool).await, ("old hash".into(), 0, None, 0));
    }
}

#[tokio::test]
async fn failed_intent_insert_rolls_back_its_epoch_savepoint() {
    let pool = database().await;
    sqlx::raw_sql("CREATE TRIGGER reject_intent BEFORE INSERT ON credential_revocations BEGIN SELECT RAISE(ABORT,'fixture'); END;")
        .execute(&pool)
        .await
        .unwrap();
    let mut tx = pool.begin().await.unwrap();
    assert!(intents::stage(&mut tx, AccountId(1), Timestamp(20)).await.is_err());
    // Even a caller handling the insert failure cannot accidentally commit only the epoch.
    let epoch: i64 =
        sqlx::query_scalar("SELECT authentication_epoch FROM user_logins WHERE account_id=1").fetch_one(&mut *tx).await.unwrap();
    assert_eq!(epoch, 0);
    tx.commit().await.unwrap();
    assert_eq!(state(&pool).await, ("old hash".into(), 0, None, 0));
}

#[tokio::test]
async fn later_sql_failure_rolls_back_credential_proof_and_staged_intent() {
    let pool = database().await;
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("UPDATE user_logins SET password_hash='new hash' WHERE account_id=1").execute(&mut *tx).await.unwrap();
    sqlx::query("UPDATE email_tokens SET used_at=20").execute(&mut *tx).await.unwrap();
    intents::stage(&mut tx, AccountId(1), Timestamp(20)).await.unwrap();
    assert!(sqlx::query("UPDATE user_logins SET email='bob@example.com' WHERE account_id=1").execute(&mut *tx).await.is_err());
    tx.rollback().await.unwrap();
    assert_eq!(state(&pool).await, ("old hash".into(), 0, None, 0));
}

#[tokio::test]
async fn fixed_epoch_completion_never_acknowledges_a_later_or_foreign_invalidation() {
    let pool = database().await;
    let mut tx = pool.begin().await.unwrap();
    let first = intents::stage(&mut tx, AccountId(1), Timestamp(20)).await.unwrap();
    let second = intents::stage(&mut tx, AccountId(1), Timestamp(21)).await.unwrap();
    let foreign = intents::stage(&mut tx, AccountId(2), Timestamp(22)).await.unwrap();
    for supplied in [
        first,
        intents::RevocationIntent { account_id: AccountId(2), ..second },
        intents::RevocationIntent { id: foreign.id, ..second },
        intents::RevocationIntent { target_epoch: 3, ..second },
    ] {
        assert_eq!(
            intents::complete(&mut tx, &supplied, RevocationOutcome::KeptCurrent, Timestamp(23)).await.unwrap(),
            Completion::NotCurrent
        );
    }
    assert_eq!(intents::pending(&mut tx, 0, NonZeroU8::new(1).unwrap()).await.unwrap(), [first]);
    assert_eq!(intents::pending(&mut tx, first.id, NonZeroU8::new(2).unwrap()).await.unwrap(), [second, foreign]);
    assert_eq!(intents::complete(&mut tx, &second, RevocationOutcome::KeptCurrent, Timestamp(24)).await.unwrap(), Completion::Applied);
    assert_eq!(
        intents::complete(&mut tx, &second, RevocationOutcome::KeptCurrent, Timestamp(25)).await.unwrap(),
        Completion::AlreadyApplied
    );
    assert_eq!(intents::complete(&mut tx, &second, RevocationOutcome::EndedAll, Timestamp(26)).await.unwrap(), Completion::NotCurrent);
    let later = intents::stage(&mut tx, AccountId(1), Timestamp(27)).await.unwrap();
    assert_eq!(
        intents::complete(&mut tx, &second, RevocationOutcome::KeptCurrent, Timestamp(28)).await.unwrap(),
        Completion::AlreadyApplied
    );
    tx.commit().await.unwrap();
    assert_eq!(stored(&pool, later).await, (None, None));
    assert_eq!(stored(&pool, first).await, (None, None));
    assert_eq!(stored(&pool, second).await, (Some("kept_current".into()), Some(24)));
    assert_eq!(stored(&pool, foreign).await, (None, None));
}

#[tokio::test]
async fn overflow_and_missing_accounts_refuse_staging_without_partial_intents() {
    let pool = database().await;
    sqlx::query("UPDATE user_logins SET authentication_epoch=? WHERE account_id=1").bind(i64::MAX - 1).execute(&pool).await.unwrap();
    let mut tx = pool.begin().await.unwrap();
    let last = intents::stage(&mut tx, AccountId(1), Timestamp(20)).await.unwrap();
    assert_eq!(last.target_epoch, i64::MAX);
    for account in [AccountId(1), AccountId(999)] {
        assert!(matches!(intents::stage(&mut tx, account, Timestamp(21)).await, Err(StorageError::Corrupt(_))));
    }
    tx.commit().await.unwrap();
    assert_eq!(state(&pool).await, ("old hash".into(), i64::MAX, None, 1));
}

#[tokio::test]
async fn completion_rollback_keeps_the_barrier_and_committed_ids_are_not_reused() {
    let pool = database().await;
    let mut tx = pool.begin().await.unwrap();
    let first = intents::stage(&mut tx, AccountId(1), Timestamp(20)).await.unwrap();
    tx.commit().await.unwrap();
    let mut tx = pool.begin().await.unwrap();
    intents::complete(&mut tx, &first, RevocationOutcome::EndedAll, Timestamp(21)).await.unwrap();
    tx.rollback().await.unwrap();
    assert_eq!(stored(&pool, first).await, (None, None));
    let mut tx = pool.begin().await.unwrap();
    intents::complete(&mut tx, &first, RevocationOutcome::EndedAll, Timestamp(22)).await.unwrap();
    // This simulates future completed-only retention; no cleanup API ships here.
    sqlx::query("DELETE FROM credential_revocations WHERE id=? AND completed_at IS NOT NULL")
        .bind(first.id)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let mut tx = pool.begin().await.unwrap();
    let next = intents::stage(&mut tx, AccountId(1), Timestamp(23)).await.unwrap();
    assert!(next.id > first.id);
    assert_eq!(intents::complete(&mut tx, &first, RevocationOutcome::EndedAll, Timestamp(24)).await.unwrap(), Completion::NotCurrent);
    tx.commit().await.unwrap();
    sqlx::query("DELETE FROM accounts WHERE id=1").execute(&pool).await.unwrap();
    let remaining: i64 = sqlx::query_scalar("SELECT count(*) FROM credential_revocations").fetch_one(&pool).await.unwrap();
    assert_eq!(remaining, 0);
}
