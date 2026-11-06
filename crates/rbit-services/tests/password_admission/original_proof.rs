//! Real SQL snapshot port and service mapping, without switching production issuers.
use super::*;
use rbit_core::Timestamp;
use rbit_storage::{Work, count_work, sqlite::credential_revocations as intents};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

#[tokio::test]
async fn original_receipt_adapter_reads_once_and_keeps_original_epoch_across_later_reset() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice", false).await;
    let before = fixture.passwords.admitted();
    let ((account, result), work) =
        count_work(fixture.services.accounts.authenticate_original_proof("alice", PASSWORD.into(), |account| {
            assert_eq!(account, Some(alice.id()));
            Ok(())
        }))
        .await;
    assert_eq!(account, Some(alice.id()));
    assert_eq!(work, Work { acquisitions: 1, statements: 1, cache_writes: 0 });
    assert_eq!(fixture.passwords.admitted(), before, "completed proof verification releases its slot");
    let proven = result.unwrap();
    assert_eq!(proven.user().id(), alice.id());
    assert_eq!(proven.receipt().original_epoch().get(), 0);
    let raw = raw(&fixture).await;
    let mut tx = raw.begin().await.unwrap();
    sqlx::query("UPDATE user_logins SET password_hash='changed PHC fixture' WHERE account_id=?")
        .bind(alice.id().0)
        .execute(&mut *tx)
        .await
        .unwrap();
    intents::stage(&mut tx, alice.id(), Timestamp(20)).await.unwrap();
    tx.commit().await.unwrap();
    assert_eq!(proven.receipt().original_epoch().get(), 0);
    assert!(
        fixture
            .database
            .accounts()
            .find_session_user_at_epoch(alice.id(), false, proven.receipt().original_epoch())
            .await
            .unwrap()
            .is_none()
    );
    let snapshot = fixture.database.accounts().find_login_snapshot("alice").await.unwrap().unwrap();
    assert_eq!(snapshot.fence.epoch.get(), 1);
    assert!(snapshot.fence.pending);
    raw.close().await;
}

#[tokio::test]
async fn pending_valid_password_is_generic_unavailable_and_wrong_password_stays_invalid() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice", false).await;
    let raw = raw(&fixture).await;
    let mut tx = raw.begin().await.unwrap();
    let intent = intents::stage(&mut tx, alice.id(), Timestamp(20)).await.unwrap();
    tx.commit().await.unwrap();
    let audit_before = fixture.database.audit().list(None, ListFilter { search: "", limit: 100, offset: 0 }).await.unwrap();
    let (_, result) = fixture.services.accounts.authenticate_original_proof("alice", PASSWORD.into(), |_| Ok(())).await;
    unavailable(&result);
    let (_, wrong) = fixture.services.accounts.authenticate_original_proof("alice", "wrong".into(), |_| Ok(())).await;
    assert!(matches!(wrong, Err(ServiceError::InvalidCredentials)));
    let audit_after = fixture.database.audit().list(None, ListFilter { search: "", limit: 100, offset: 0 }).await.unwrap();
    assert_eq!(audit_after.items, audit_before.items, "adapter does not invent failed-login/success accounting");
    let mut tx = raw.begin().await.unwrap();
    intents::complete(&mut tx, &intent, intents::RevocationOutcome::EndedAll, Timestamp(21)).await.unwrap();
    tx.commit().await.unwrap();
    let (_, result) = fixture.services.accounts.authenticate_original_proof("alice", PASSWORD.into(), |_| Ok(())).await;
    assert_eq!(result.unwrap().receipt().original_epoch().get(), 1);
    raw.close().await;
}

#[tokio::test]
async fn original_proof_keeps_callback_and_shared_overload_precedence() {
    for closed in [false, true] {
        let fixture = Fixture::new().await;
        fixture.user("alice", false).await;
        let held = if closed {
            fixture.passwords.close();
            None
        } else {
            Some(fixture.passwords.begin().unwrap())
        };
        for name in ["alice", "missing"] {
            let (_, result) = fixture.services.accounts.authenticate_original_proof(name, PASSWORD.into(), |_| Ok(())).await;
            unavailable(&result);
            let (_, callback) = fixture
                .services
                .accounts
                .authenticate_original_proof(name, PASSWORD.into(), |_| Err(ServiceError::RateLimited { retry_after_seconds: 7 }))
                .await;
            assert!(matches!(callback, Err(ServiceError::RateLimited { retry_after_seconds: 7 })));
        }
        drop(held);
    }
}

async fn raw(fixture: &Fixture) -> sqlx::SqlitePool {
    let mut config = Config::default();
    config.storage.data_dir = fixture.directory.path().to_path_buf();
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::new().filename(config.storage.sqlite_path()).foreign_keys(true))
        .await
        .unwrap()
}
