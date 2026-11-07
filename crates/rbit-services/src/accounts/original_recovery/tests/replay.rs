use super::*;
use rbit_core::Timestamp;
use rbit_identity::sessions::SessionRecoveryDisposition as Disposition;
use rbit_identity_models::credential::CredentialEpoch;
use rbit_storage::{KvStore, SessionRecord, Work, count_work, sqlite::credential_revocations as intents};

#[tokio::test]
async fn completed_keep_is_idempotent_and_never_replays_cleanup_against_fresh_sessions() {
    let fixture = Fixture::new().await;
    let intent = fixture.change().await;
    let (result, work) = count_work(fixture.recover(intent)).await;
    assert_eq!(result.unwrap(), Disposition::Completed);
    assert_eq!(work, Work { acquisitions: 4, statements: 4, cache_writes: 0 }, "pair/find/state/ack remain counted in owned worker");
    assert_eq!(fixture.outcome(intent.id).await.as_deref(), Some("kept_current"));
    let sessions = fixture.kv.sessions().epoch_sidecar();
    assert_eq!(sessions.get_live([1; 32]).await.unwrap().unwrap().epoch().get(), intent.target_epoch);
    assert!(sessions.get_snapshot([2; 32]).await.unwrap().is_none());
    let now = Timestamp::now();
    let record = SessionRecord::at(fixture.account, Timestamp(now.0 + 3600), None, "fresh after ack", now);
    sessions.put_bound([9; 32], record, fixture.account, CredentialEpoch::new(intent.target_epoch).unwrap()).await.unwrap();
    let (result, work) = count_work(fixture.recover(intent)).await;
    assert_eq!(result.unwrap(), Disposition::AlreadyResolved);
    assert_eq!(work, Work { acquisitions: 3, statements: 3, cache_writes: 0 });
    assert!(sessions.get_live([9; 32]).await.unwrap().is_some(), "no destructive replay after acknowledgment");
}

#[tokio::test]
async fn acknowledge_failure_retries_the_actual_promoted_binding_from_durable_decision() {
    let fixture = Fixture::new().await;
    let intent = fixture.change().await;
    fixture.execute("CREATE TRIGGER fixture_ack_failure BEFORE UPDATE OF outcome ON credential_revocations BEGIN SELECT RAISE(ABORT,'fixture'); END").await;
    assert!(fixture.recover(intent).await.is_err());
    assert_eq!(fixture.outcome(intent.id).await, None);
    assert_eq!(fixture.kv.sessions().epoch_sidecar().get_live([1; 32]).await.unwrap().unwrap().epoch().get(), intent.target_epoch);
    fixture.execute("DROP TRIGGER fixture_ack_failure").await;
    assert_eq!(fixture.recover(intent).await.unwrap(), Disposition::Completed);
    assert_eq!(fixture.outcome(intent.id).await.as_deref(), Some("kept_current"));
}

#[tokio::test]
async fn bounded_cursor_passes_deferred_older_ids_and_supersedes_only_after_newer_ack() {
    let fixture = Fixture::new().await;
    let older = fixture.change().await;
    let newer = fixture
        .database
        .profiles()
        .set_password_hash_with_intent(fixture.account, "admin", CredentialEpoch::new(older.target_epoch).unwrap(), Timestamp::now())
        .await
        .unwrap()
        .unwrap();
    let limit = NonZeroU8::new(1).unwrap();
    let first = fixture.services.accounts.recover_original_session_batch(0, limit).await.unwrap();
    assert_eq!((first.visited, first.deferred, first.completed), (1, 1, 0));
    assert_eq!(first.cursor, older.id);
    let second = fixture.services.accounts.recover_original_session_batch(first.cursor, limit).await.unwrap();
    assert_eq!((second.visited, second.completed), (1, 1));
    assert_eq!(second.cursor, newer.id);
    let retired = fixture.services.accounts.recover_original_session_batch(0, limit).await.unwrap();
    assert_eq!((retired.visited, retired.superseded), (1, 1));
    assert_eq!(fixture.outcome(older.id).await, None, "supersession does not forge old completion");
    let witness: Option<i64> = sqlx::query_scalar("SELECT superseded_by_epoch FROM credential_revocations WHERE id=?")
        .bind(older.id)
        .fetch_one(&fixture.raw)
        .await
        .unwrap();
    assert_eq!(witness, Some(newer.target_epoch));
    assert_eq!(fixture.outcome(newer.id).await.as_deref(), Some("ended_all"));
    assert_eq!(fixture.services.accounts.recover_original_session_batch(0, limit).await.unwrap().visited, 0);
}

#[tokio::test]
async fn explicit_legacy_current_decision_is_reported_without_implicit_end_all() {
    let fixture = Fixture::new().await;
    let mut tx = fixture.raw.begin().await.unwrap();
    let legacy = intents::stage(&mut tx, fixture.account, Timestamp::now()).await.unwrap();
    tx.commit().await.unwrap();
    assert_eq!(fixture.recover(legacy).await.unwrap(), Disposition::Unsupported);
    assert_eq!(fixture.outcome(legacy.id).await, None);
    assert!(fixture.kv.sessions().epoch_sidecar().get_live([1; 32]).await.unwrap().is_some());
    let report = fixture.services.accounts.recover_original_session_batch(0, NonZeroU8::new(10).unwrap()).await.unwrap();
    assert_eq!((report.unsupported, report.completed), (1, 0));
}

#[tokio::test]
async fn wrong_pair_is_refused_before_any_effect_or_sql_acknowledgment() {
    let fixture = Fixture::new().await;
    let intent = fixture.change().await;
    let directory = tempfile::tempdir().unwrap();
    let other = KvStore::open(&directory.path().join("wrong.redb")).unwrap();
    let sessions = other.sessions().epoch_sidecar();
    sessions.initialize_identity_if_empty().await.unwrap();
    let now = Timestamp::now();
    sessions
        .put_bound(
            [9; 32],
            SessionRecord::at(fixture.account, Timestamp(now.0 + 3600), None, "other", now),
            fixture.account,
            fixture.original,
        )
        .await
        .unwrap();
    let other_services = crate::Services::new(Arc::new(fixture.config.clone()), &fixture.database, &other).await.unwrap();
    assert!(other_services.accounts.recover_original_session_intent(intent).await.is_err());
    assert!(sessions.get_live([9; 32]).await.unwrap().is_some());
    assert_eq!(fixture.outcome(intent.id).await, None);
    assert_eq!(fixture.kv.sessions().epoch_sidecar().get_live([1; 32]).await.unwrap().unwrap().epoch(), fixture.original);
}

#[tokio::test]
async fn unpaired_completion_is_never_reported_as_successful_recovery() {
    let fixture = Fixture::new().await;
    let intent = fixture.change().await;
    let mut tx = fixture.raw.begin().await.unwrap();
    assert_eq!(
        intents::complete(&mut tx, &intent, intents::RevocationOutcome::KeptCurrent, Timestamp::now()).await.unwrap(),
        intents::Completion::Applied
    );
    tx.commit().await.unwrap();
    assert!(fixture.recover(intent).await.is_err());
    assert_eq!(fixture.kv.sessions().epoch_sidecar().get_live([1; 32]).await.unwrap().unwrap().epoch(), fixture.original);

    let mut tx = fixture.raw.begin().await.unwrap();
    let legacy = intents::stage(&mut tx, fixture.account, Timestamp::now()).await.unwrap();
    assert_eq!(
        intents::complete(&mut tx, &legacy, intents::RevocationOutcome::EndedAll, Timestamp::now()).await.unwrap(),
        intents::Completion::Applied
    );
    tx.commit().await.unwrap();
    assert_eq!(fixture.recover(legacy).await.unwrap(), Disposition::Unsupported);
}
