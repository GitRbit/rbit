use super::*;

#[tokio::test]
async fn concurrent_insert_has_one_winner_and_binds_all_three_tables() {
    let (_dir, _legacy, store) = store();
    let other = store.clone();
    let a = record(7, 3600);
    let b = record(8, 3600);
    let (left, right) = tokio::join!(
        store.put_if_absent([1; 32], a.clone(), AccountId(7), epoch(1)),
        other.put_if_absent([1; 32], b.clone(), AccountId(8), epoch(2)),
    );
    let left = left.unwrap();
    let right = right.unwrap();
    assert_ne!(left, right);
    let expected = if left { &a } else { &b };
    assert_tables(&store, [1; 32], Some(expected));
    let tx = store.db.begin_read().unwrap();
    let index = tx.open_table(ACCOUNT_SESSIONS).unwrap();
    assert_eq!(index.len().unwrap(), 1);
}

#[tokio::test]
async fn delete_reinsert_same_account_epoch_and_bytes_rejects_old_snapshot_aba() {
    let (_dir, _legacy, store) = store();
    let original = record(7, 3600);
    store.put_if_absent([1; 32], original.clone(), AccountId(7), epoch(1)).await.unwrap();
    let stale = store.get_live([1; 32]).await.unwrap().unwrap();
    assert!(store.delete(&stale).await.unwrap());
    assert_tables(&store, [1; 32], None);
    assert!(!store.renew(&stale, original.expires_at, original.last_seen_at).await.unwrap());
    store.put_if_absent([1; 32], original.clone(), AccountId(7), epoch(1)).await.unwrap();
    let fresh = store.get_live([1; 32]).await.unwrap().unwrap();
    assert!(fresh.binding != stale.binding);
    assert!(!store.renew(&stale, Timestamp(original.expires_at.0 + 10), original.last_seen_at).await.unwrap());
    assert!(!store.delete(&stale).await.unwrap());
    assert_tables(&store, [1; 32], Some(&original));
    assert!(store.delete(&fresh).await.unwrap());
}

#[tokio::test]
async fn overlapping_renewals_are_monotonic_and_delete_never_resurrects() {
    let (_dir, _legacy, store) = store();
    let original = record(7, 3600);
    store.put_if_absent([1; 32], original.clone(), AccountId(7), epoch(1)).await.unwrap();
    let snapshot = store.get_live([1; 32]).await.unwrap().unwrap();
    let later = Timestamp(original.expires_at.0 + 3600);
    let seen = Timestamp(original.last_seen_at.0 + 60);
    let (a, b) = tokio::join!(store.renew(&snapshot, later, original.last_seen_at), store.renew(&snapshot, original.expires_at, seen));
    assert!(a.unwrap() && b.unwrap());
    let renewed = store.get_live([1; 32]).await.unwrap().unwrap();
    assert_eq!(renewed.record().expires_at, later);
    assert_eq!(renewed.record().last_seen_at, seen);
    assert!(renewed.binding == snapshot.binding);
    let (renew, delete) = tokio::join!(store.renew(&snapshot, later, seen), store.delete(&snapshot));
    let _ = renew.unwrap();
    assert!(delete.unwrap());
    assert!(store.get_live([1; 32]).await.unwrap().is_none());
    assert!(!store.renew(&snapshot, later, seen).await.unwrap());
}

#[tokio::test]
async fn changed_epoch_account_or_immutable_identity_refuses_stale_mutation() {
    let (_dir, _legacy, store) = store();
    let original = record(7, 3600);
    store.put_if_absent([1; 32], original.clone(), AccountId(7), epoch(1)).await.unwrap();
    let snapshot = store.get_live([1; 32]).await.unwrap().unwrap();
    // A future explicit promotion changes binding; an old renewal cannot overwrite it.
    let tx = store.db.begin_write().unwrap();
    let replacement = Binding::with_random(AccountId(7), epoch(2), |bytes| {
        bytes.fill(9);
        Ok(())
    })
    .unwrap();
    tx.open_table(EPOCHS).unwrap().insert(&[1; 32], &replacement.encode()).unwrap();
    tx.commit().unwrap();
    assert!(!store.renew(&snapshot, original.expires_at, original.last_seen_at).await.unwrap());
    assert!(!store.delete(&snapshot).await.unwrap());
    let promoted = store.get_live([1; 32]).await.unwrap().unwrap();
    let tx = store.db.begin_write().unwrap();
    let mut changed = original.clone();
    changed.user_agent = "changed immutable identity".into();
    tx.open_table(SESSIONS).unwrap().insert(&[1; 32], encode_session(&changed).unwrap().as_slice()).unwrap();
    tx.commit().unwrap();
    assert!(!store.renew(&promoted, original.expires_at, original.last_seen_at).await.unwrap());
    assert!(!store.delete(&promoted).await.unwrap());
    // Legacy mutation is deliberately not activation-safe; this partial identity check
    // cannot detect identical-byte legacy overwrite/ABA and is not claimed to do so.
}
