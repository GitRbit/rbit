use super::*;
use redb::ReadableTableMetadata;

mod integrity;
mod lifecycle;
mod races;
mod revocation;

fn store() -> (tempfile::TempDir, super::super::SessionStore, EpochSessionStore) {
    let dir = tempfile::tempdir().unwrap();
    let db = Arc::new(redb::Database::create(dir.path().join("sessions.redb")).unwrap());
    let legacy = super::super::SessionStore::new(db);
    legacy.ensure_table().unwrap();
    let epoch = legacy.epoch_sidecar();
    (dir, legacy, epoch)
}
fn record(account: i64, expires_in: i64) -> SessionRecord {
    let now = Timestamp::now();
    SessionRecord::at(AccountId(account), Timestamp(now.0 + expires_in), Some("192.0.2.1".into()), "fixture secret user agent", now)
}
fn epoch(value: i64) -> CredentialEpoch {
    CredentialEpoch::new(value).unwrap()
}
fn assert_tables(store: &EpochSessionStore, digest: [u8; 32], expected: Option<&SessionRecord>) {
    let tx = store.db.begin_read().unwrap();
    let sessions = tx.open_table(SESSIONS).unwrap();
    let epochs = tx.open_table(EPOCHS).unwrap();
    let index = tx.open_table(ACCOUNT_SESSIONS).unwrap();
    if let Some(record) = expected {
        assert_eq!(sessions.get(&digest).unwrap().unwrap().value(), encode_session(record).unwrap());
        let binding = epochs.get(&digest).unwrap().unwrap();
        assert_eq!(binding.value().len(), 32);
        assert!(Binding::decode(binding.value(), record.account_id).is_ok());
        assert!(index.get((record.account_id.0, &digest)).unwrap().is_some());
    } else {
        assert!(sessions.get(&digest).unwrap().is_none());
        assert!(epochs.get(&digest).unwrap().is_none());
        assert_eq!(index.len().unwrap(), 0);
    }
}

#[tokio::test]
async fn atomic_binding_keeps_exact_postcard_and_reopens_with_the_same_snapshot() {
    let (dir, legacy, store) = store();
    let original = record(7, 3600);
    assert!(store.put_if_absent([1; 32], original.clone(), AccountId(7), epoch(i64::MAX)).await.unwrap());
    assert_tables(&store, [1; 32], Some(&original));
    let snapshot = store.get_live([1; 32]).await.unwrap().unwrap();
    assert_eq!(snapshot.record(), &original);
    assert_eq!(snapshot.epoch(), epoch(i64::MAX));
    assert!(!snapshot.is_legacy_initial());
    let binding = snapshot.binding;
    drop(legacy);
    drop(store);
    let reopened = EpochSessionStore::new(Arc::new(redb::Database::create(dir.path().join("sessions.redb")).unwrap()));
    let observed = reopened.get_live([1; 32]).await.unwrap().unwrap();
    assert!(observed.binding == binding);
    assert_eq!(observed.record(), &original);
    assert_tables(&reopened, [1; 32], Some(&original));
}

#[tokio::test]
async fn missing_sidecar_is_read_only_initial_not_automatic_adoption() {
    let (_dir, legacy, store) = store();
    let original = record(7, 3600);
    legacy.put([1; 32], original.clone()).await.unwrap();
    let snapshot = store.get_live([1; 32]).await.unwrap().unwrap();
    assert!(snapshot.is_legacy_initial());
    assert_eq!(snapshot.epoch(), CredentialEpoch::INITIAL);
    assert!(!store.renew(&snapshot, original.expires_at, original.last_seen_at).await.unwrap());
    assert!(!store.delete(&snapshot).await.unwrap());
    assert!(!store.put_if_absent([1; 32], original.clone(), AccountId(7), epoch(10)).await.unwrap());
    let tx = store.db.begin_read().unwrap();
    assert!(matches!(tx.open_table(EPOCHS), Err(redb::TableError::TableDoesNotExist(_))));
    drop(tx);
    assert_eq!(legacy.get([1; 32]).await.unwrap(), Some(original));
}

#[tokio::test]
async fn failed_random_source_is_before_write_and_cannot_leave_partial_state() {
    let (_dir, _legacy, mut store) = store();
    store.random = |_| Err(());
    assert!(matches!(store.put_if_absent([1; 32], record(7, 3600), AccountId(7), epoch(1)).await, Err(StorageError::Corrupt(_))));
    let tx = store.db.begin_read().unwrap();
    assert_eq!(tx.open_table(SESSIONS).unwrap().len().unwrap(), 0);
    assert_eq!(tx.open_table(ACCOUNT_SESSIONS).unwrap().len().unwrap(), 0);
    assert!(matches!(tx.open_table(EPOCHS), Err(redb::TableError::TableDoesNotExist(_))));
}

#[tokio::test]
async fn mismatched_account_or_existing_digest_never_overwrites_or_reindexes() {
    let (_dir, _legacy, store) = store();
    let original = record(7, 3600);
    assert!(!store.put_if_absent([1; 32], original.clone(), AccountId(8), epoch(1)).await.unwrap());
    assert!(store.put_if_absent([1; 32], original.clone(), AccountId(7), epoch(1)).await.unwrap());
    assert!(!store.put_if_absent([1; 32], record(8, 3600), AccountId(8), epoch(2)).await.unwrap());
    assert_tables(&store, [1; 32], Some(&original));
    let snapshot = store.get_live([1; 32]).await.unwrap().unwrap();
    let debug = format!("{snapshot:?}");
    assert!(!debug.contains("fixture secret user agent") && !debug.contains("192.0.2.1"));
    assert!(!debug.contains("digest") && !debug.contains("incarnation"));
}

#[tokio::test]
async fn zero_epoch_sidecars_are_distinct_from_missing_and_expired_records_cannot_renew() {
    let (_dir, _legacy, store) = store();
    let expired = record(7, -1);
    assert!(store.put_if_absent([1; 32], expired.clone(), AccountId(7), epoch(0)).await.unwrap());
    assert!(store.get_live([1; 32]).await.unwrap().is_none());
    let live = record(7, 3600);
    store.put_if_absent([2; 32], live, AccountId(7), epoch(0)).await.unwrap();
    let snapshot = store.get_live([2; 32]).await.unwrap().unwrap();
    assert!(!snapshot.is_legacy_initial());
    let mut expired = snapshot.record().clone();
    expired.expires_at = Timestamp(Timestamp::now().0 - 1);
    let tx = store.db.begin_write().unwrap();
    tx.open_table(SESSIONS).unwrap().insert(&[2; 32], encode_session(&expired).unwrap().as_slice()).unwrap();
    tx.commit().unwrap();
    assert!(!store.renew(&snapshot, Timestamp::now(), Timestamp::now()).await.unwrap());
}
