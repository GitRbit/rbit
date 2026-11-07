use super::fixture::*;
use rbit_core::Timestamp;
use rbit_identity_models::{
    credential::CredentialEpoch,
    sessions::{CompletedSessionRevocation, SessionRevocationAcknowledgment as Ack, SessionRevocationState},
};
use rbit_storage::{KvStore, SessionRecord, sqlite::credential_revocations as intents};

#[tokio::test]
async fn mismatched_pair_cannot_initialize_recover_or_ack_another_store() {
    let fixture = Fixture::new().await;
    let intent = fixture.keep_change().await;
    let request = fixture.request(&intent).await;
    let directory = tempfile::tempdir().unwrap();
    let other = KvStore::open(&directory.path().join("other.redb")).unwrap();
    let sessions = other.sessions().epoch_sidecar();
    let pair = sessions.initialize_identity_if_empty().await.unwrap();
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
    let store = fixture.database.credential_revocations();
    assert!(store.initialize_pair(&sessions).await.is_err());
    assert!(store.verify_pair(&sessions).await.is_err());
    assert!(sessions.apply_pending_revocation(request, fixture.pair).await.is_err());
    assert!(sessions.get_live([9; 32]).await.unwrap().is_some());
    // A real effect on another store is still not a proof for the expected instance.
    let wrong = sessions.apply_pending_revocation(request, pair).await.unwrap();
    assert!(store.acknowledge(&request, &fixture.sessions, &wrong, now).await.is_err());
    // Even naming that other instance does not defeat the persisted SQL pair match.
    assert_eq!(store.acknowledge(&request, &sessions, &wrong, now).await.unwrap(), Ack::Changed);
    assert_eq!(fixture.outcome(intent.id).await, None);
    assert!(fixture.sessions.get_live([1; 32]).await.unwrap().is_some());
}

#[tokio::test]
async fn legacy_completion_never_becomes_a_paired_cleanup_witness_or_gets_restamped() {
    let fixture = Fixture::new().await;
    let older = fixture.keep_change().await;
    let newer = fixture.end_all_change(CredentialEpoch::new(older.target_epoch).unwrap()).await;
    let mut tx = fixture.raw.begin().await.unwrap();
    // Deliberately exercises the STILL-LEGACY compatibility API. Active v2 must
    // refuse this path; here its unpaired completion cannot retire older work.
    assert_eq!(
        intents::complete(&mut tx, &newer, intents::RevocationOutcome::EndedAll, Timestamp::now()).await.unwrap(),
        intents::Completion::Applied
    );
    tx.commit().await.unwrap();
    let store = fixture.database.credential_revocations();
    let completed = store.find(&newer).await.unwrap().unwrap();
    assert_eq!(store.state(&completed).await.unwrap(), SessionRevocationState::Invalid);
    let old = store.find(&older).await.unwrap().unwrap();
    assert_eq!(store.state(&old).await.unwrap(), SessionRevocationState::Invalid);
    let witness =
        CompletedSessionRevocation::from_storage(newer.id, fixture.account, CredentialEpoch::new(newer.target_epoch).unwrap()).unwrap();
    assert_eq!(store.supersede(&old, &witness).await.unwrap(), Ack::Changed);
    let pair = fixture.pair.storage_bytes();
    assert!(
        sqlx::query("UPDATE credential_revocations SET cleanup_pair=? WHERE id=?")
            .bind(pair.as_slice())
            .bind(newer.id)
            .execute(&fixture.raw)
            .await
            .is_err()
    );
    let stamp: Option<Vec<u8>> = sqlx::query_scalar("SELECT cleanup_pair FROM credential_revocations WHERE id=?")
        .bind(newer.id)
        .fetch_one(&fixture.raw)
        .await
        .unwrap();
    assert!(stamp.is_none());
}

#[tokio::test]
async fn pair_metadata_is_explicit_immutable_and_rejects_a_malformed_extra_row() {
    let fixture = Fixture::new().await;
    let store = fixture.database.credential_revocations();
    assert_eq!(store.verify_pair(&fixture.sessions).await.unwrap(), fixture.pair);
    assert!(sqlx::query("UPDATE credential_session_pair SET identity=zeroblob(16)").execute(&fixture.raw).await.is_err());
    assert!(sqlx::query("DELETE FROM credential_session_pair").execute(&fixture.raw).await.is_err());
    fixture.execute("PRAGMA ignore_check_constraints=ON; INSERT INTO credential_session_pair(singleton,identity) VALUES(2,zeroblob(16)); PRAGMA ignore_check_constraints=OFF;").await;
    assert!(store.verify_pair(&fixture.sessions).await.is_err());
    assert!(store.initialize_pair(&fixture.sessions).await.is_err(), "no normalization of malformed state");
}
