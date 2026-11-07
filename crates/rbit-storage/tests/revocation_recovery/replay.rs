use super::fixture::*;
use rbit_core::Timestamp;
use rbit_identity_models::{
    credential::CredentialEpoch,
    sessions::{SessionRevocationAcknowledgment as Ack, SessionRevocationOutcome as Outcome},
};
use rbit_storage::{Database, KvStore, SessionRecord, Work, count_work};

#[tokio::test]
async fn keep_decision_is_in_same_commit_and_replays_promoted_binding_after_reopen_before_ack() {
    let fixture = Fixture::new().await;
    let intent = fixture.keep_change().await;
    let request = fixture.request(&intent).await;
    let stored: Vec<u8> = sqlx::query_scalar("SELECT keep_spec FROM credential_revocations WHERE id=?")
        .bind(intent.id)
        .fetch_one(&fixture.raw)
        .await
        .unwrap();
    assert_eq!(stored, fixture.current.promotion_spec().unwrap().storage_bytes());
    assert_eq!(stored.len(), 64);
    let applied = fixture.sessions.apply_pending_revocation(request, fixture.pair).await.unwrap();
    assert_eq!(applied.outcome(), Outcome::KeptCurrent);
    assert_eq!(fixture.outcome(intent.id).await, None);
    let Fixture { directory, database, kv, sessions, raw, account, pair, .. } = fixture;
    raw.close().await;
    database.close().await;
    drop(database);
    drop(sessions);
    drop(kv);
    // Stopped reopen is a committed failure cut, not an asserted OS/power-loss test.
    let database = Database::open(&directory.path().join("recovery.db")).await.unwrap();
    let kv = KvStore::open(&directory.path().join("sessions.redb")).unwrap();
    let sessions = kv.sessions().epoch_sidecar();
    let revocations = database.credential_revocations();
    let pending = revocations.find(&intent).await.unwrap().unwrap();
    assert_eq!(pending.request(), Some(request));
    let (wrong, work) = count_work(revocations.acknowledge(&request, &sessions, &applied, Timestamp::now())).await;
    assert!(wrong.is_err(), "old-instance proof is not new-instance authority");
    assert_eq!(work, Work::default());
    let replay = sessions.apply_pending_revocation(request, pair).await.unwrap();
    assert_eq!(replay.outcome(), Outcome::KeptCurrent);
    assert_eq!(revocations.acknowledge(&request, &sessions, &replay, Timestamp::now()).await.unwrap(), Ack::Applied);
    assert_eq!(sessions.get_live([1; 32]).await.unwrap().unwrap().epoch().get(), intent.target_epoch);
    assert!(sessions.get_snapshot([2; 32]).await.unwrap().is_none());
    assert!(
        database
            .accounts()
            .find_session_user_at_epoch(account, false, CredentialEpoch::new(intent.target_epoch).unwrap())
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn failed_sql_ack_keeps_pending_and_retry_preserves_the_same_promoted_incarnation() {
    let fixture = Fixture::new().await;
    let intent = fixture.keep_change().await;
    let request = fixture.request(&intent).await;
    fixture.execute("CREATE TRIGGER fixture_ack_failure BEFORE UPDATE OF outcome ON credential_revocations BEGIN SELECT RAISE(ABORT,'fixture ack'); END").await;
    let applied = fixture.sessions.apply_pending_revocation(request, fixture.pair).await.unwrap();
    let revocations = fixture.database.credential_revocations();
    assert!(revocations.acknowledge(&request, &fixture.sessions, &applied, Timestamp::now()).await.is_err());
    assert_eq!(fixture.outcome(intent.id).await, None);
    fixture.execute("DROP TRIGGER fixture_ack_failure").await;
    let replay = fixture.sessions.apply_pending_revocation(request, fixture.pair).await.unwrap();
    assert_eq!(replay.outcome(), Outcome::KeptCurrent);
    assert_eq!(revocations.acknowledge(&request, &fixture.sessions, &replay, Timestamp::now()).await.unwrap(), Ack::Applied);
    assert_eq!(revocations.acknowledge(&request, &fixture.sessions, &replay, Timestamp::now()).await.unwrap(), Ack::AlreadyResolved);
}

#[tokio::test]
async fn logout_captured_before_promotion_ends_same_incarnation_but_never_a_replacement() {
    let fixture = Fixture::new().await;
    let intent = fixture.keep_change().await;
    let request = fixture.request(&intent).await;
    fixture.sessions.apply_pending_revocation(request, fixture.pair).await.unwrap();
    assert!(!fixture.sessions.delete(&fixture.current).await.unwrap(), "stale resolver cleanup stays epoch-exact");
    assert!(!fixture.sessions.renew(&fixture.current, Timestamp(i64::MAX), Timestamp::now()).await.unwrap());
    assert!(fixture.sessions.logout(&fixture.current).await.unwrap(), "actual cookie logout crosses the same-incarnation promotion");
    let now = Timestamp::now();
    let record = SessionRecord::at(fixture.account, Timestamp(now.0 + 3600), None, "replacement", now);
    fixture
        .sessions
        .put_bound([1; 32], record, fixture.account, CredentialEpoch::new(intent.target_epoch).unwrap())
        .await
        .unwrap()
        .unwrap();
    assert!(!fixture.sessions.logout(&fixture.current).await.unwrap(), "old logout cannot delete new incarnation");
    assert!(fixture.sessions.get_live([1; 32]).await.unwrap().is_some());
}

#[tokio::test]
async fn logout_or_same_epoch_replacement_before_replay_is_never_kept_or_resurrected() {
    for replacement in [false, true] {
        let fixture = Fixture::new().await;
        let intent = fixture.keep_change().await;
        let request = fixture.request(&intent).await;
        assert!(fixture.sessions.delete(&fixture.current).await.unwrap());
        if replacement {
            fixture
                .sessions
                .put_bound([1; 32], fixture.current.record().clone(), fixture.account, fixture.original)
                .await
                .unwrap()
                .unwrap();
        }
        let applied = fixture.sessions.apply_pending_revocation(request, fixture.pair).await.unwrap();
        assert_eq!(applied.outcome(), Outcome::EndedAll);
        assert!(fixture.sessions.get_snapshot([1; 32]).await.unwrap().is_none());
        assert!(fixture.sessions.get_snapshot([2; 32]).await.unwrap().is_none());
    }
}

#[tokio::test]
async fn older_worker_never_acknowledges_newer_epoch_or_deletes_its_sessions() {
    let fixture = Fixture::new().await;
    let older = fixture.keep_change().await;
    let old_request = fixture.request(&older).await;
    let newer = fixture.end_all_change(CredentialEpoch::new(older.target_epoch).unwrap()).await;
    let now = Timestamp::now();
    let record = SessionRecord::at(fixture.account, Timestamp(now.0 + 3600), None, "newer", now);
    fixture.sessions.put_bound([9; 32], record, fixture.account, CredentialEpoch::new(newer.target_epoch).unwrap()).await.unwrap();
    let old_effect = fixture.sessions.apply_pending_revocation(old_request, fixture.pair).await.unwrap();
    assert_eq!(
        fixture
            .database
            .credential_revocations()
            .acknowledge(&old_request, &fixture.sessions, &old_effect, Timestamp::now())
            .await
            .unwrap(),
        Ack::Changed
    );
    assert_eq!(fixture.outcome(older.id).await, None);
    assert_eq!(fixture.outcome(newer.id).await, None);
    assert!(fixture.sessions.get_live([9; 32]).await.unwrap().is_some());
}
