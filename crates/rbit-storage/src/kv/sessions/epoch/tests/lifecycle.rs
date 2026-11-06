use super::*;

#[tokio::test]
async fn list_and_revoke_others_are_account_epoch_and_current_incarnation_scoped() {
    let (_dir, _legacy, store) = store();
    for (digest, account, original) in [(1, 7, 1), (2, 7, 1), (3, 7, 1), (4, 7, 2), (5, 8, 1)] {
        assert!(store.put_if_absent([digest; 32], record(account, 3600), AccountId(account), epoch(original)).await.unwrap());
    }
    let current = store.get_live([1; 32]).await.unwrap().unwrap();
    assert_eq!(store.list_bound(AccountId(7)).await.unwrap().len(), 4);
    assert_eq!(store.revoke_others(&current).await.unwrap(), Some(2));
    for digest in [1, 4, 5] {
        assert!(store.get_live([digest; 32]).await.unwrap().is_some());
    }
    for digest in [2, 3] {
        assert!(store.get_snapshot([digest; 32]).await.unwrap().is_none());
    }
    let tx = store.db.begin_read().unwrap();
    let bindings = tx.open_table(EPOCHS).unwrap();
    let index = tx.open_table(ACCOUNT_SESSIONS).unwrap();
    for digest in [2, 3] {
        assert!(bindings.get(&[digest; 32]).unwrap().is_none());
        assert!(index.get((7, &[digest; 32])).unwrap().is_none());
    }
}

#[tokio::test]
async fn same_epoch_delete_reinsert_cannot_authorize_late_bulk_revoke() {
    let (_dir, _legacy, store) = store();
    store.put_if_absent([1; 32], record(7, 3600), AccountId(7), epoch(1)).await.unwrap();
    store.put_if_absent([2; 32], record(7, 3600), AccountId(7), epoch(1)).await.unwrap();
    let stale = store.get_live([1; 32]).await.unwrap().unwrap();
    assert!(store.delete(&stale).await.unwrap());
    store.put_if_absent([1; 32], stale.record.clone(), AccountId(7), epoch(1)).await.unwrap();
    assert_eq!(store.revoke_others(&stale).await.unwrap(), None);
    assert_eq!(store.list_bound(AccountId(7)).await.unwrap().len(), 2);
}

#[tokio::test]
async fn cross_account_index_corruption_rolls_back_the_entire_bulk_revoke() {
    let (_dir, _legacy, store) = store();
    for (digest, account) in [(1, 7), (2, 7), (3, 8)] {
        store.put_if_absent([digest; 32], record(account, 3600), AccountId(account), epoch(1)).await.unwrap();
    }
    let current = store.get_live([1; 32]).await.unwrap().unwrap();
    let tx = store.db.begin_write().unwrap();
    tx.open_table(ACCOUNT_SESSIONS).unwrap().insert((7, &[3; 32]), ()).unwrap();
    tx.commit().unwrap();
    assert!(store.revoke_others(&current).await.is_err());
    assert!(store.get_live([2; 32]).await.unwrap().is_some(), "earlier deletion rolled back");
    assert!(store.get_live([3; 32]).await.unwrap().is_some(), "other account untouched");
    assert!(store.list_bound(AccountId(7)).await.is_err(), "no index repair/adoption");
}

#[tokio::test]
async fn bounded_expiry_purge_removes_record_binding_and_index_across_multiple_batches() {
    let (_dir, _legacy, store) = store();
    let tx = store.db.begin_write().unwrap();
    let count = super::super::super::PURGE_BATCH_SIZE + 3;
    for index in 0..count {
        let mut digest = [0_u8; 32];
        digest[..8].copy_from_slice(&u64::try_from(index).unwrap().to_be_bytes());
        let record = record(7, -1);
        let binding = Binding::with_random(AccountId(7), epoch(1), |bytes| {
            bytes.fill(7);
            Ok(())
        })
        .unwrap();
        assert!(insert(&tx, digest, &record, &encode_session(&record).unwrap(), binding).unwrap());
    }
    tx.commit().unwrap();
    store.put_if_absent([255; 32], record(7, 3600), AccountId(7), epoch(1)).await.unwrap();
    assert_eq!(store.purge_expired_bound().await.unwrap(), count);
    let tx = store.db.begin_read().unwrap();
    use redb::ReadableTableMetadata;
    assert_eq!(tx.open_table(SESSIONS).unwrap().len().unwrap(), 1);
    assert_eq!(tx.open_table(ACCOUNT_SESSIONS).unwrap().len().unwrap(), 1);
    assert_eq!(tx.open_table(EPOCHS).unwrap().len().unwrap(), 1);
    drop(tx);
    assert_eq!(store.list_bound(AccountId(7)).await.unwrap().len(), 1);
}

#[tokio::test]
async fn late_expiry_batch_never_deletes_replaced_same_digest_same_epoch_record() {
    let (_dir, _legacy, store) = store();
    store.put_if_absent([1; 32], record(7, -1), AccountId(7), epoch(1)).await.unwrap();
    let expired = store.get_snapshot([1; 32]).await.unwrap().unwrap();
    assert!(store.delete(&expired).await.unwrap());
    store.put_if_absent([1; 32], record(7, 3600), AccountId(7), epoch(1)).await.unwrap();
    assert_eq!(super::super::lifecycle::purge_batch(&store.db, &[expired], Timestamp::now()).unwrap(), 0);
    assert!(store.get_live([1; 32]).await.unwrap().is_some());
}

#[tokio::test]
async fn legacy_missing_binding_is_not_adopted_by_listing_or_expiry_cleanup() {
    let (_dir, legacy, store) = store();
    legacy.put([1; 32], record(7, -1)).await.unwrap();
    assert!(store.list_bound(AccountId(7)).await.is_err());
    assert!(store.purge_expired_bound().await.is_err());
    assert!(store.get_snapshot([1; 32]).await.unwrap().unwrap().is_legacy_initial());
}
