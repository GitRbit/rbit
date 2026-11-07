use super::*;

#[tokio::test]
async fn no_session_end_all_persists_witness_and_an_actual_older_sql_snapshot_is_refused() {
    let mut f = RuntimeFixture::new().await;
    let snapshot = f.fixture.directory.path().join("older-active.db");
    sqlx::query("VACUUM INTO ?").bind(snapshot.to_str().unwrap()).execute(&mut f.accepted.owner.connection).await.unwrap();
    let store = f.epoch();
    assert_eq!(store.list_bound(AccountId(1)).await.unwrap().len(), 0);
    f.set_epoch(100).await;
    let pair = store.pair_identity().await.unwrap().unwrap();
    let request = request(100, Decision::EndAll);
    let applied = store.apply_pending_revocation(request, pair).await.unwrap();
    assert!(applied.matches(&store, &request));
    assert_eq!(f.floor(), 100);
    assert_eq!(store.list_bound(AccountId(1)).await.unwrap().len(), 0);
    // This is real session-store completion before SQL ACK; production ACK wiring
    // remains a separate gate. Stop/drain both actual owners before replacing SQL.
    drop(store);
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)").execute(&mut f.accepted.owner.connection).await.unwrap();
    let fixture = f.close().await;
    let wal = fixture.sql.with_file_name("identity.db-wal");
    match std::fs::metadata(wal) {
        Ok(metadata) => assert_eq!(metadata.len(), 0),
        Err(error) => assert_eq!(error.kind(), std::io::ErrorKind::NotFound),
    }
    std::fs::copy(&snapshot, &fixture.sql).unwrap();
    let owner = fixture.owner().await;
    assert_eq!(runtime::read(&owner.db).unwrap(), Some(100));
    assert!(owner.accept_active_inner().await.is_err(), "persistent witness survives even when every binding is absent");
}

#[tokio::test]
async fn revoked_or_expired_records_leave_the_runtime_witness_intact() {
    let mut f = RuntimeFixture::new().await;
    f.set_epoch(100).await;
    let store = f.epoch();
    let current = put(&store, 1, 100).await;
    put(&store, 2, 100).await;
    assert_eq!(store.revoke_others(&current).await.unwrap(), Some(1));
    assert_eq!(f.floor(), 100);
    store.put_bound([3; 32], record(-3600), AccountId(1), epoch(100)).await.unwrap().unwrap();
    assert_eq!(store.purge_expired_bound().await.unwrap(), 1);
    assert_eq!(f.floor(), 100);
    assert!(store.logout(&current).await.unwrap());
    assert_eq!(store.list_bound(AccountId(1)).await.unwrap().len(), 0);
    assert_eq!(f.floor(), 100);
}

#[tokio::test]
async fn cancelled_waiter_does_not_split_a_dispatched_record_and_floor_commit() {
    use futures_util::poll;
    let mut f = RuntimeFixture::new().await;
    f.set_epoch(100).await;
    let store = f.epoch();
    let held = f.accepted.owner.db.begin_write().unwrap();
    let mut request = Box::pin(store.put_bound([9; 32], record(3600), AccountId(1), epoch(100)));
    assert!(poll!(request.as_mut()).is_pending(), "first poll queues the real blocking writer behind the held transaction");
    drop(request);
    held.commit().unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while store.get_snapshot([9; 32]).await.unwrap().is_none() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("dispatched worker must finish after its waiter is dropped");
    assert_eq!(f.floor(), 100);
    drop(store);
    let fixture = f.close().await;
    let accepted = fixture.owner().await.accept_active_inner().await.unwrap();
    assert_eq!(runtime::read(&accepted.owner.db).unwrap(), Some(100));
    assert_eq!(accepted.session_store().epoch_sidecar().get_snapshot([9; 32]).await.unwrap().unwrap().epoch(), epoch(100));
}
