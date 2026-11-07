use super::*;

fn store() -> (tempfile::TempDir, SessionStore) {
    let dir = tempfile::tempdir().unwrap();
    let db = redb::Database::create(dir.path().join("kv.redb")).unwrap();
    let store = SessionStore::new(Arc::new(db));
    store.ensure_table().unwrap();
    (dir, store)
}

fn record(account: i64, expires_in: i64) -> SessionRecord {
    SessionRecord::new(AccountId(account), Timestamp(Timestamp::now().0 + expires_in), Some("192.0.2.1".into()), "Mozilla/5.0")
}

#[test]
fn record_round_trips_and_truncates_user_agents() {
    let record = SessionRecord::new(AccountId(42), Timestamp(1_900_000_000), None, &"x".repeat(1000));
    assert_eq!(record.user_agent.len(), 256);
    assert_eq!(decode_session(&encode_session(&record).unwrap()), Some(record));
    assert_eq!(decode_session(&[0xff; 3]), None);
}

#[tokio::test]
async fn indexes_sessions_by_account() {
    let (_dir, store) = store();
    store.put([1; 32], record(1, 3600)).await.unwrap();
    store.put([2; 32], record(1, 3600)).await.unwrap();
    store.put([3; 32], record(2, 3600)).await.unwrap();
    store.put([4; 32], record(1, -10)).await.unwrap();

    let listed: Vec<_> = store.list_for_account(AccountId(1)).await.unwrap().into_iter().map(|s| s.digest).collect();
    assert_eq!(listed.len(), 2, "expired sessions are not listed");
    assert!(listed.contains(&[1; 32]) && listed.contains(&[2; 32]));

    store.delete([1; 32]).await.unwrap();
    assert_eq!(store.list_for_account(AccountId(1)).await.unwrap().len(), 1);
    assert!(store.get([1; 32]).await.unwrap().is_none());
}

#[tokio::test]
async fn deletes_all_but_one() {
    let (_dir, store) = store();
    for byte in 1..=3 {
        store.put([byte; 32], record(7, 3600)).await.unwrap();
    }
    store.put([9; 32], record(8, 3600)).await.unwrap();

    assert_eq!(store.delete_for_account(AccountId(7), Some([2; 32])).await.unwrap(), 2);
    let left: Vec<_> = store.list_for_account(AccountId(7)).await.unwrap().into_iter().map(|s| s.digest).collect();
    assert_eq!(left, [[2; 32]]);
    assert!(store.get([9; 32]).await.unwrap().is_some(), "other accounts are untouched");

    assert_eq!(store.delete_for_account(AccountId(7), None).await.unwrap(), 1);
    assert_eq!(store.list_for_account(AccountId(7)).await.unwrap().len(), 0);
}

#[tokio::test]
async fn renewal_never_restores_revoked_or_expired_sessions() {
    let (_dir, store) = store();
    let original = record(7, 3600);
    store.put([1; 32], original.clone()).await.unwrap();
    let stale = store.get([1; 32]).await.unwrap().unwrap();
    store.delete_for_account(AccountId(7), None).await.unwrap();
    assert!(!store.renew([1; 32], stale.expires_at, stale.last_seen_at).await.unwrap());
    assert!(store.get([1; 32]).await.unwrap().is_none());
    assert_eq!(store.list_for_account(AccountId(7)).await.unwrap().len(), 0);

    store.put([2; 32], record(7, -1)).await.unwrap();
    assert!(!store.renew([2; 32], original.expires_at, original.last_seen_at).await.unwrap());
    assert!(store.get([2; 32]).await.unwrap().is_none());
}

#[tokio::test]
async fn renewal_does_not_regress_timestamps() {
    let (_dir, store) = store();
    let original = record(7, 3600);
    store.put([1; 32], original.clone()).await.unwrap();
    let later = Timestamp(original.expires_at.0 + 3600);
    assert!(store.renew([1; 32], later, original.last_seen_at).await.unwrap());
    assert!(store.renew([1; 32], original.expires_at, Timestamp(original.last_seen_at.0 - 10)).await.unwrap());
    let renewed = store.get([1; 32]).await.unwrap().unwrap();
    assert_eq!(renewed.expires_at, later);
    assert_eq!(renewed.last_seen_at, original.last_seen_at);
    assert_eq!(renewed.account_id, original.account_id);
}

#[tokio::test]
async fn replacing_a_session_updates_its_account_index() {
    let (_dir, store) = store();
    store.put([1; 32], record(7, 3600)).await.unwrap();
    store.put([1; 32], record(8, 3600)).await.unwrap();
    assert_eq!(store.list_for_account(AccountId(7)).await.unwrap().len(), 0);
    assert_eq!(store.delete_for_account(AccountId(7), None).await.unwrap(), 0);
    assert_eq!(store.list_for_account(AccountId(8)).await.unwrap().len(), 1);
}

#[tokio::test]
async fn purges_expired_sessions_and_their_index_entries() {
    let (_dir, store) = store();
    store.put([1; 32], record(1, -1)).await.unwrap();
    store.put([2; 32], record(1, 3600)).await.unwrap();
    assert_eq!(store.purge_expired().await.unwrap(), 1);
    assert_eq!(store.delete_for_account(AccountId(1), None).await.unwrap(), 1, "the purged session left no index entry");
}

#[tokio::test]
async fn scanning_live_sessions_never_waits_for_the_writer() {
    let (_dir, store) = store();
    store.put([1; 32], record(1, 3600)).await.unwrap();
    let writer = store.db.begin_write().unwrap();
    let result = tokio::time::timeout(std::time::Duration::from_secs(5), store.purge_expired()).await;
    drop(writer);
    assert_eq!(result.expect("a read-only expiry scan must not acquire the write lock").unwrap(), 0);
}

#[tokio::test]
async fn cleanup_rechecks_replaced_and_revoked_candidates() {
    let (_dir, store) = store();
    let now = Timestamp::now();
    store.put([1; 32], record(1, -1)).await.unwrap();
    store.put([2; 32], record(1, -1)).await.unwrap();
    store.put([3; 32], record(1, -1)).await.unwrap();
    // These keys were collected by an earlier read snapshot.
    let candidates = [[1; 32], [2; 32], [3; 32]];
    store.put([1; 32], record(2, 3600)).await.unwrap();
    store.delete([2; 32]).await.unwrap();
    assert_eq!(remove_expired_batch(&store.db, &candidates, now).unwrap(), 1);
    assert_eq!(store.get([1; 32]).await.unwrap().unwrap().account_id, AccountId(2));
    assert_eq!(store.delete_for_account(AccountId(1), None).await.unwrap(), 0);
    assert_eq!(store.delete_for_account(AccountId(2), None).await.unwrap(), 1);
}

#[tokio::test]
async fn purge_spans_multiple_batches_without_missing_keys_or_indexes() {
    let (_dir, store) = store();
    let count = PURGE_BATCH_SIZE * 2 + 1;
    let tx = store.db.begin_write().unwrap();
    {
        let mut sessions = tx.open_table(SESSIONS).unwrap();
        let mut index = tx.open_table(ACCOUNT_SESSIONS).unwrap();
        let expired = encode_session(&record(1, -1)).unwrap();
        for id in 0..count {
            let mut digest = [0; 32];
            digest[..8].copy_from_slice(&u64::try_from(id).unwrap().to_be_bytes());
            sessions.insert(&digest, expired.as_slice()).unwrap();
            index.insert((1, &digest), ()).unwrap();
        }
    }
    tx.commit().unwrap();
    store.put([255; 32], record(1, 3600)).await.unwrap();
    assert_eq!(store.purge_expired().await.unwrap(), count);
    assert_eq!(store.purge_expired().await.unwrap(), 0);
    assert_eq!(store.delete_for_account(AccountId(1), None).await.unwrap(), 1);
}

#[test]
fn storage_paths_reexport_the_exact_feature_record_types() {
    use std::any::TypeId;
    assert_eq!(TypeId::of::<SessionRecord>(), TypeId::of::<rbit_identity_models::sessions::SessionRecord>());
    assert_eq!(TypeId::of::<StoredSession>(), TypeId::of::<rbit_identity_models::sessions::StoredSession>());
    assert_eq!(TypeId::of::<crate::SessionRecord>(), TypeId::of::<rbit_identity_models::sessions::SessionRecord>());
    assert_eq!(TypeId::of::<crate::SessionUser>(), TypeId::of::<rbit_identity_models::sessions::SessionUser>());
}
