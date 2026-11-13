//! Actual prior-schema entry and retained native owner; no normal `Database::open` is used.
use super::*;
use crate::KvStore;
use sqlx::Connection;
use std::{future::Future, num::NonZeroU64, task::Poll, time::Duration};
#[tokio::test]
async fn pinned_main_layout_installs_under_owner_without_key_or_session_cleanup_then_prepares() {
    let f = Fixture::main_snapshot().await;
    let before = f.migration_rows().await;
    assert_eq!(before.last().unwrap().0, 59);
    let owner = StoppedIdentityUpgrade::open_for_install(&f.sql, &f.kv, limits())
        .await
        .unwrap()
        .install_layout()
        .await
        .unwrap_or_else(|failure| panic!("layout installation failed: {:?}", failure.error()));
    assert_eq!(f.migration_rows().await.last().unwrap().0, 87);
    assert!(!f.sql.with_file_name(crate::sealing::KEY_FILE_NAME).exists());
    assert_eq!(f.state().await, "legacy");
    assert!(KvStore::open(&f.kv).is_err(), "installed owner still retains the actual existing KV file");
    let prepared = owner.prepare().await.unwrap();
    assert_eq!((prepared.summary().rebased_accounts, prepared.summary().final_epoch_floor), (1, 1));
}
#[tokio::test]
async fn foreign_kv_handle_or_bad_or_overbudget_kv_preserves_migration_history() {
    for failure in ["held", "bad", "budget"] {
        let f = Fixture::main_snapshot().await;
        let before = f.migration_rows().await;
        if failure == "held" {
            let held = KvStore::open(&f.kv).unwrap();
            assert!(StoppedIdentityUpgrade::open_for_install(&f.sql, &f.kv, limits()).await.is_err());
            drop(held);
        } else {
            if failure == "bad" {
                let db = redb::Database::open(&f.kv).unwrap();
                let tx = db.begin_write().unwrap();
                tx.open_table(RECORDS).unwrap().insert(&[1; 32], &[0xff_u8][..]).unwrap();
                tx.commit().unwrap();
            }
            let limit =
                if failure == "budget" { IdentityUpgradeLimits { max_entries: NonZeroU64::new(1).unwrap(), ..limits() } } else { limits() };
            let owner = StoppedIdentityUpgrade::open_for_install(&f.sql, &f.kv, limit).await.unwrap();
            let failure = owner.install_layout().await.unwrap_err();
            assert!(KvStore::open(&f.kv).is_err(), "ordinary failure retains its native owner");
            drop(failure);
        }
        assert_eq!(f.migration_rows().await, before);
        let installed: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE name='credential_protocol')")
            .fetch_one(&mut f.connection().await)
            .await
            .unwrap();
        assert!(!installed);
    }
}
#[tokio::test]
async fn unknown_future_dirty_or_changed_checksum_history_refuses_without_migration() {
    for mutation in [
        "INSERT INTO _sqlx_migrations(version,description,success,checksum,execution_time) VALUES(999,'future',1,zeroblob(48),0)",
        "UPDATE _sqlx_migrations SET success=0 WHERE version=59",
        "UPDATE _sqlx_migrations SET checksum=zeroblob(48) WHERE version=59",
    ] {
        let f = Fixture::main_snapshot().await;
        sqlx::query(mutation).execute(&mut f.connection().await).await.unwrap();
        let before = f.migration_rows().await;
        assert!(StoppedIdentityUpgrade::open_for_install(&f.sql, &f.kv, limits()).await.is_err());
        assert_eq!(f.migration_rows().await, before);
    }
}
#[tokio::test]
async fn cancellation_of_install_waiter_does_not_drop_queued_operation_native_owner() {
    let f = Fixture::main_snapshot().await;
    let owner = StoppedIdentityUpgrade::open_for_install(&f.sql, &f.kv, limits()).await.unwrap();
    let mut connection = f.connection().await;
    let write_lock = connection.begin_with("BEGIN IMMEDIATE").await.unwrap();
    let mut request = Box::pin(owner.install_layout());
    assert!(std::future::poll_fn(|cx| Poll::Ready(request.as_mut().poll(cx))).await.is_pending());
    drop(request);
    assert!(KvStore::open(&f.kv).is_err(), "the spawned operation, not the abandoned outer future, owns KV");
    write_lock.commit().await.unwrap();
    let owner = tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            match StoppedIdentityUpgrade::open_for_install(&f.sql, &f.kv, limits()).await {
                Ok(owner) => break owner,
                Err(_) => tokio::task::yield_now().await,
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(f.migration_rows().await.last().unwrap().0, 87);
    assert_eq!(f.state().await, "legacy");
    drop(owner);
}

#[tokio::test]
async fn supported_rewrite_prefix_is_preserved_and_prepare_cannot_skip_installation() {
    let f = Fixture::main_snapshot().await;
    let owner = StoppedIdentityUpgrade::open_for_install(&f.sql, &f.kv, limits()).await.unwrap();
    let failure = owner.prepare().await.unwrap_err();
    assert!(matches!(failure.error(), IdentityUpgradeError::Invalid));
    drop(failure);
    let mut connection = f.connection().await;
    sqlx::migrate!("./migrations").run_to(64, &mut connection).await.unwrap();
    connection.close().await.unwrap();
    let before = f.migration_rows().await;
    assert_eq!(before.last().unwrap().0, 64);
    let owner = StoppedIdentityUpgrade::open_for_install(&f.sql, &f.kv, limits())
        .await
        .unwrap()
        .install_layout()
        .await
        .unwrap_or_else(|failure| panic!("layout installation failed: {:?}", failure.error()));
    let after = f.migration_rows().await;
    assert_eq!(&after[..before.len()], before.as_slice());
    assert_eq!(owner.prepare().await.unwrap().summary().rebased_accounts, 1);
}

#[tokio::test]
async fn failed_suffix_migration_rolls_back_earlier_migrations_and_retains_the_owner() {
    let f = Fixture::main_snapshot().await;
    sqlx::query("CREATE TABLE wiki_store_identity(fixture INTEGER)").execute(&mut f.connection().await).await.unwrap();
    let before = f.migration_rows().await;
    let owner = StoppedIdentityUpgrade::open_for_install(&f.sql, &f.kv, limits()).await.unwrap();
    let failure = owner.install_layout().await.unwrap_err();
    assert_eq!(f.migration_rows().await, before);
    assert!(KvStore::open(&f.kv).is_err(), "failed layout installation still retains native ownership");
    let allocated: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE name='credential_epoch_allocator')")
        .fetch_one(&mut f.connection().await)
        .await
        .unwrap();
    assert!(!allocated);
    drop(failure);
    assert!(KvStore::open(&f.kv).is_ok());
}
