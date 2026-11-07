use super::*;

#[test]
fn aborted_write_hides_all_three_staged_values_and_default_open_does_not_create_sidecars() {
    let (_dir, _legacy, store) = store();
    let read = store.db.begin_read().unwrap();
    assert!(matches!(read.open_table(EPOCHS), Err(redb::TableError::TableDoesNotExist(_))));
    drop(read);
    // Initialize only this temporary fixture's table so absence can be inspected after abort.
    let tx = store.db.begin_write().unwrap();
    tx.open_table(EPOCHS).unwrap();
    tx.commit().unwrap();
    let record = record(7, 3600);
    let binding = Binding::with_random(AccountId(7), epoch(1), |bytes| {
        bytes.fill(3);
        Ok(())
    })
    .unwrap();
    for explicit in [false, true] {
        let tx = store.db.begin_write().unwrap();
        assert!(insert(&tx, [1; 32], &record, &encode_session(&record).unwrap(), binding).unwrap());
        assert_tables(&store, [1; 32], None); // independent read sees committed state only
        if explicit {
            tx.abort().unwrap();
        } else {
            drop(tx);
        }
        assert_tables(&store, [1; 32], None);
    }
}

#[tokio::test]
async fn malformed_epoch_or_foreign_account_binding_fails_closed() {
    let (_dir, _legacy, store) = store();
    store.put_if_absent([1; 32], record(7, 3600), AccountId(7), epoch(1)).await.unwrap();
    let snapshot = store.get_live([1; 32]).await.unwrap().unwrap();
    for (account, epoch) in [(7, -1), (8, 1)] {
        let tx = store.db.begin_write().unwrap();
        let mut bytes = [0; 32];
        bytes[..8].copy_from_slice(&i64::to_be_bytes(account));
        bytes[8..16].copy_from_slice(&i64::to_be_bytes(epoch));
        tx.open_table(EPOCHS).unwrap().insert(&[1; 32], &bytes).unwrap();
        tx.commit().unwrap();
        assert!(matches!(store.get_live([1; 32]).await, Err(StorageError::Corrupt(_))));
        assert!(matches!(store.renew(&snapshot, Timestamp::now(), Timestamp::now()).await, Err(StorageError::Corrupt(_))));
        assert!(matches!(store.delete(&snapshot).await, Err(StorageError::Corrupt(_))));
    }
}

#[tokio::test]
async fn orphan_bindings_and_matching_index_entries_refuse_digest_reuse() {
    let (_dir, _legacy, store) = store();
    let binding = Binding::with_random(AccountId(7), epoch(1), |bytes| {
        bytes.fill(3);
        Ok(())
    })
    .unwrap();
    let tx = store.db.begin_write().unwrap();
    tx.open_table(EPOCHS).unwrap().insert(&[1; 32], &binding.encode()).unwrap();
    tx.open_table(ACCOUNT_SESSIONS).unwrap().insert((7, &[2; 32]), ()).unwrap();
    tx.commit().unwrap();
    for digest in [[1; 32], [2; 32]] {
        assert!(!store.put_if_absent(digest, record(7, 3600), AccountId(7), epoch(1)).await.unwrap());
        assert!(store.get_live(digest).await.unwrap().is_none());
    }
    let read = store.db.begin_read().unwrap();
    assert_eq!(read.open_table(SESSIONS).unwrap().len().unwrap(), 0);
}

#[tokio::test]
async fn record_and_binding_read_does_not_scan_derived_index_but_mutations_require_membership() {
    let (_dir, _legacy, store) = store();
    let original = record(7, 3600);
    store.put_if_absent([1; 32], original.clone(), AccountId(7), epoch(1)).await.unwrap();
    let snapshot = store.get_live([1; 32]).await.unwrap().unwrap();
    let tx = store.db.begin_write().unwrap();
    tx.open_table(ACCOUNT_SESSIONS).unwrap().remove((7, &[1; 32])).unwrap();
    tx.commit().unwrap();
    let observed = store.get_live([1; 32]).await.unwrap().unwrap();
    assert!(observed.binding == snapshot.binding);
    assert!(matches!(store.renew(&snapshot, original.expires_at, original.last_seen_at).await, Err(StorageError::Corrupt(_))));
    assert!(matches!(store.delete(&snapshot).await, Err(StorageError::Corrupt(_))));
}

#[tokio::test]
async fn wrong_sidecar_schema_is_an_error_not_an_initial_fallback() {
    let (_dir, legacy, store) = store();
    legacy.put([1; 32], record(7, 3600)).await.unwrap();
    let wrong: TableDefinition<'_, &[u8; 32], &[u8]> = TableDefinition::new("session_epochs_v1");
    let tx = store.db.begin_write().unwrap();
    tx.open_table(wrong).unwrap().insert(&[1; 32], &[1_u8][..]).unwrap();
    tx.commit().unwrap();
    assert!(store.get_live([1; 32]).await.is_err());
}
