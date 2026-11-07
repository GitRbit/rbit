use super::*;
use crate::kv::sessions::epoch::runtime;
use rbit_identity_models::sessions::{SessionRevocationAcknowledgment as Ack, SessionRevocationState};

#[tokio::test]
async fn real_global_transaction_cleanup_and_ack_share_the_accepted_protocol() {
    let f = SqlFixture::new().await;
    let revocations = f.database.credential_revocations();
    let sessions = f.accepted.session_store().epoch_sidecar();
    let now = Timestamp::now();
    let (intent, work) =
        crate::count_work(f.database.profiles().set_password_hash_with_intent(AccountId(1), "replacement PHC", epoch(1), now)).await;
    let intent = intent.unwrap().unwrap();
    assert_eq!((work.acquisitions, work.statements), (1, 4));
    let pending = revocations.find(&intent).await.unwrap().unwrap();
    assert_eq!(revocations.state(&pending).await.unwrap(), SessionRevocationState::CurrentPending);
    assert!(f.database.accounts().find_session_user_at_epoch(AccountId(1), false, pending.target()).await.unwrap().is_none());
    let request = pending.request().unwrap();
    let mut raw = f.fixture.connection().await;
    let mut tx = raw.begin().await.unwrap();
    assert!(
        crate::sqlite::credential_revocations::complete(
            &mut tx,
            &intent,
            crate::sqlite::credential_revocations::RevocationOutcome::EndedAll,
            now
        )
        .await
        .is_err()
    );
    tx.rollback().await.unwrap();
    assert_eq!(revocations.state(&pending).await.unwrap(), SessionRevocationState::CurrentPending);
    let pair = revocations.verify_pair(&sessions).await.unwrap();
    let applied = sessions.apply_pending_revocation(request, pair).await.unwrap();
    // No session existed, but acknowledged cleanup must retain its target forever.
    assert_eq!(runtime::read(&f.accepted.owner.db).unwrap(), Some(intent.target_epoch));
    let legacy = crate::SessionStore::new(Arc::clone(&f.accepted.owner.db)).epoch_sidecar();
    let (result, work) = crate::count_work(revocations.acknowledge(&request, &legacy, &applied, now)).await;
    assert!(result.is_err());
    assert_eq!(work, crate::Work::default());
    assert_eq!(revocations.acknowledge(&request, &sessions, &applied, now).await.unwrap(), Ack::Applied);
    assert!(f.database.accounts().find_session_user_at_epoch(AccountId(1), false, pending.target()).await.unwrap().is_some());
    assert_eq!(revocations.state(&pending).await.unwrap(), SessionRevocationState::Resolved);
}

#[tokio::test]
async fn acknowledged_empty_cleanup_then_older_sql_restore_refuses_restart() {
    let mut f = SqlFixture::new().await;
    let snapshot = f.fixture.directory.path().join("before-ack.db");
    sqlx::query("VACUUM INTO ?").bind(snapshot.to_str().unwrap()).execute(&mut f.accepted.owner.connection).await.unwrap();
    let revocations = f.database.credential_revocations();
    let sessions = f.accepted.session_store().epoch_sidecar();
    let now = Timestamp::now();
    let intent =
        f.database.profiles().set_password_hash_with_intent(AccountId(1), "replacement PHC", epoch(1), now).await.unwrap().unwrap();
    let request = revocations.find(&intent).await.unwrap().unwrap().request().unwrap();
    let pair = revocations.verify_pair(&sessions).await.unwrap();
    let applied = sessions.apply_pending_revocation(request, pair).await.unwrap();
    assert_eq!(revocations.acknowledge(&request, &sessions, &applied, now).await.unwrap(), Ack::Applied);
    let floor = runtime::read(&f.accepted.owner.db).unwrap().unwrap();
    drop(revocations);
    drop(sessions);
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)").execute(&mut f.accepted.owner.connection).await.unwrap();
    let fixture = f.close().await;
    let wal = fixture.sql.with_file_name("identity.db-wal");
    match std::fs::metadata(wal) {
        Ok(metadata) => assert_eq!(metadata.len(), 0),
        Err(error) => assert_eq!(error.kind(), std::io::ErrorKind::NotFound),
    }
    std::fs::copy(snapshot, &fixture.sql).unwrap();
    let owner = fixture.owner().await;
    assert_eq!(runtime::read(&owner.db).unwrap(), Some(floor));
    assert!(owner.accept_active_inner().await.is_err());
}
