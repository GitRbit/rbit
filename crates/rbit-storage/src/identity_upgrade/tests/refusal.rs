use super::*;
use crate::{Database, KvStore};
use redb::{ReadableDatabase, ReadableTableMetadata};
use std::num::{NonZeroU32, NonZeroU64};
#[tokio::test]
async fn old_native_or_application_kv_handle_prevents_the_stopped_owner() {
    let f = Fixture::new().await;
    let held = KvStore::open(&f.kv).unwrap();
    assert!(StoppedIdentityUpgrade::open(&f.sql, &f.kv, limits()).await.is_err());
    let session = held.sessions();
    drop(held);
    assert!(
        StoppedIdentityUpgrade::open(&f.sql, &f.kv, limits()).await.is_err(),
        "an old derived session handle still owns the real backend"
    );
    drop(session);
    let native = redb::Database::open(&f.kv).unwrap();
    assert!(StoppedIdentityUpgrade::open(&f.sql, &f.kv, limits()).await.is_err());
    drop(native);
    assert!(StoppedIdentityUpgrade::open(&f.sql, &f.kv, limits()).await.is_ok());
    assert_eq!(f.state().await, "legacy");
}
#[tokio::test]
async fn entry_or_record_limits_refuse_before_any_mode_pair_or_legacy_mutation() {
    for limit in [
        IdentityUpgradeLimits { max_entries: NonZeroU64::new(1).unwrap(), ..limits() },
        IdentityUpgradeLimits { max_record_bytes: NonZeroU32::new(1).unwrap(), ..limits() },
    ] {
        let f = Fixture::new().await;
        f.orphan_epoch(500);
        let owner = StoppedIdentityUpgrade::open(&f.sql, &f.kv, limit).await.unwrap();
        assert!(matches!(owner.prepare().await.unwrap_err().error(), IdentityUpgradeError::IncompleteScan));
        assert_eq!(f.state().await, "legacy");
        let pair: i64 =
            sqlx::query_scalar("SELECT count(*) FROM credential_session_pair").fetch_one(&mut f.connection().await).await.unwrap();
        assert_eq!(pair, 0);
        let db = redb::Database::open(&f.kv).unwrap();
        let tx = db.begin_read().unwrap();
        assert_eq!(tx.open_table(LEGACY).unwrap().len().unwrap(), 1);
    }
}
#[tokio::test]
async fn corrupt_or_unknown_session_schema_refuses_before_mode_changes() {
    for unknown in [false, true] {
        let f = Fixture::new().await;
        {
            let db = redb::Database::open(&f.kv).unwrap();
            let tx = db.begin_write().unwrap();
            if unknown {
                tx.open_table(redb::TableDefinition::<u8, u8>::new("session_epochs_v99")).unwrap();
            } else {
                tx.open_table(RECORDS).unwrap().insert(&[1; 32], &[0xff_u8][..]).unwrap();
            }
            tx.commit().unwrap();
        }
        assert!(f.owner().await.prepare().await.is_err());
        assert_eq!(f.state().await, "legacy");
    }
}
#[tokio::test]
async fn exhausted_orphan_floor_and_one_sided_pair_refuse_before_mode_changes() {
    let f = Fixture::new().await;
    f.orphan_epoch(i64::MAX);
    assert!(matches!(f.owner().await.prepare().await.unwrap_err().error(), IdentityUpgradeError::Exhausted));
    assert_eq!(f.state().await, "legacy");
    let f = Fixture::new().await;
    sqlx::query("INSERT INTO credential_session_pair VALUES(1,zeroblob(16))").execute(&mut f.connection().await).await.unwrap();
    assert!(matches!(f.owner().await.prepare().await.unwrap_err().error(), IdentityUpgradeError::Mismatch));
    assert_eq!(f.state().await, "legacy");
}
#[tokio::test]
async fn normal_open_refuses_before_key_creation_or_preindex_table_deletion() {
    let f = Fixture::new().await;
    drop(f.owner().await.prepare().await.unwrap());
    std::fs::remove_file(f.sql.with_file_name(crate::sealing::KEY_FILE_NAME)).unwrap();
    assert!(Database::open(&f.sql).await.is_err());
    assert!(!f.sql.with_file_name(crate::sealing::KEY_FILE_NAME).exists());
    {
        let db = redb::Database::open(&f.kv).unwrap();
        let tx = db.begin_write().unwrap();
        tx.open_table(LEGACY).unwrap().insert(&[1; 32], &[0; 16]).unwrap();
        tx.commit().unwrap();
    }
    assert!(KvStore::open(&f.kv).is_err());
    let db = redb::Database::open(&f.kv).unwrap();
    let tx = db.begin_read().unwrap();
    assert_eq!(tx.open_table(LEGACY).unwrap().len().unwrap(), 1);
}

#[tokio::test]
async fn missing_existing_files_never_create_stores_or_sealing_keys() {
    let directory = tempfile::tempdir().unwrap();
    let sql = directory.path().join("missing.db");
    let kv = directory.path().join("missing.redb");
    assert!(StoppedIdentityUpgrade::open(&sql, &kv, limits()).await.is_err());
    assert!(!sql.exists());
    assert!(!kv.exists());
    assert!(!sql.with_file_name(crate::sealing::KEY_FILE_NAME).exists());
}

#[tokio::test]
async fn missing_installed_protocol_or_guard_is_not_a_legacy_fallback() {
    let f = Fixture::new().await;
    sqlx::query("DROP TABLE credential_protocol").execute(&mut f.connection().await).await.unwrap();
    assert!(Database::open(&f.sql).await.is_err());
    assert!(StoppedIdentityUpgrade::open(&f.sql, &f.kv, limits()).await.is_err());
    let f = Fixture::new().await;
    sqlx::query("DROP TRIGGER identity_v2_accounts_update").execute(&mut f.connection().await).await.unwrap();
    assert!(StoppedIdentityUpgrade::open(&f.sql, &f.kv, limits()).await.is_err());
}
