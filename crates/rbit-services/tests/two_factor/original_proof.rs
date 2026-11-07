//! Dormant original-password receipts carried through the real bounded MFA owner.
use super::*;
use rbit_storage::{Work, count_work, sqlite::credential_revocations as intents};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

async fn original(fixture: &Fixture, login: &str) -> rbit_services::accounts::ProvenAccount {
    fixture.services.accounts.authenticate_original_proof(login, PASSWORD.into(), |_| Ok(())).await.1.unwrap()
}
async fn raw(fixture: &Fixture) -> sqlx::SqlitePool {
    let mut config = Config::default();
    config.storage.data_dir = fixture.directory.path().to_path_buf();
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            rbit_storage::sqlite::writer_compatibility::register(SqliteConnectOptions::new())
                .filename(config.storage.sqlite_path())
                .foreign_keys(true),
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn original_and_legacy_tokens_cannot_cross_finish_apis_or_consume_proof_on_rejection() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice").await;
    let (_, _, codes) = fixture.enrol(&alice).await;
    let proven = original(&fixture, "alice").await;
    let clone = fixture.services.two_factor.clone();
    let pending = clone.start_original_password_pending(proven);
    let before = fixture.database.two_factor().remaining_recovery_codes(alice.id()).await.unwrap();
    let (result, work) = count_work(fixture.services.complete_sign_in(&pending, &codes[0], &client())).await;
    assert!(matches!(result, Err(ServiceError::NotFound)));
    assert_eq!(work, Work::default());
    assert_eq!(fixture.database.two_factor().remaining_recovery_codes(alice.id()).await.unwrap(), before);
    assert_eq!(fixture.services.two_factor.pending_account(&pending), Some(alice.id()));
    let returned = fixture.services.complete_original_sign_in(&pending, &codes[0], &client()).await.unwrap();
    assert_eq!(returned.receipt().original_epoch().get(), 0);
    assert_eq!(returned.user().id(), alice.id());
    assert!(fixture.services.two_factor.pending_account(&pending).is_none());
    assert!(matches!(fixture.services.complete_original_sign_in(&pending, &codes[1], &client()).await, Err(ServiceError::NotFound)));
    let legacy = fixture.pending("alice").await;
    let before = fixture.database.two_factor().remaining_recovery_codes(alice.id()).await.unwrap();
    let (result, work) = count_work(fixture.services.complete_original_sign_in(&legacy, &codes[1], &client())).await;
    assert!(matches!(result, Err(ServiceError::NotFound)));
    assert_eq!(work, Work::default());
    assert_eq!(fixture.database.two_factor().remaining_recovery_codes(alice.id()).await.unwrap(), before);
    fixture.services.complete_sign_in(&legacy, &codes[1], &client()).await.unwrap();
}

#[tokio::test]
async fn original_finish_matches_legacy_sql_work_and_reattaches_authoritative_renamed_user() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice").await;
    let (_, _, codes) = fixture.enrol(&alice).await;
    let legacy = fixture.pending("alice").await;
    let (_, baseline) = count_work(fixture.services.complete_sign_in(&legacy, &codes[0], &client())).await;
    assert!(baseline.statements > 0 && baseline.acquisitions > 0);
    let proven = original(&fixture, "alice").await;
    let (mut forged, receipt) = proven.into_parts();
    forged.is_admin = !alice.is_admin;
    let pending = fixture.services.two_factor.start_original_password_pending(receipt.bind_user(forged).unwrap());
    fixture.database.accounts().rename(alice.id(), &"alice-renamed".parse().unwrap()).await.unwrap();
    let (returned, work) = count_work(fixture.services.complete_original_sign_in(&pending, &codes[1], &client())).await;
    assert_eq!(work, baseline, "same account query + one-use proof + audit; no second epoch lookup");
    let returned = returned.unwrap();
    assert_eq!(returned.user().name().as_str(), "alice-renamed");
    assert_eq!(returned.user().is_admin, alice.is_admin, "public bind_user does not authorize mutable privilege flags");
    assert_eq!(returned.receipt().account(), alice.id());
    assert_eq!(returned.receipt().original_epoch().get(), 0);
}

#[tokio::test]
async fn reset_epoch_rejects_before_spending_recovery_code_or_recording_wrong_proof() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice").await;
    let (_, _, codes) = fixture.enrol(&alice).await;
    let pending = fixture.services.two_factor.start_original_password_pending(original(&fixture, "alice").await);
    let before = fixture.database.two_factor().remaining_recovery_codes(alice.id()).await.unwrap();
    let raw = raw(&fixture).await;
    let mut tx = raw.begin().await.unwrap();
    let intent = intents::stage(&mut tx, alice.id(), Timestamp(20)).await.unwrap();
    tx.commit().await.unwrap();
    let (result, work) = count_work(fixture.services.complete_original_sign_in(&pending, &codes[0], &client())).await;
    assert!(matches!(result, Err(ServiceError::NotFound)));
    assert_eq!(work, Work { acquisitions: 1, statements: 1, cache_writes: 0 });
    assert_eq!(fixture.database.two_factor().remaining_recovery_codes(alice.id()).await.unwrap(), before);
    assert!(fixture.services.two_factor.pending_account(&pending).is_none());
    let mut tx = raw.begin().await.unwrap();
    intents::complete(&mut tx, &intent, intents::RevocationOutcome::EndedAll, Timestamp(21)).await.unwrap();
    tx.commit().await.unwrap();
    let fresh = fixture.services.two_factor.start_original_password_pending(original(&fixture, "alice").await);
    let result = fixture.services.complete_original_sign_in(&fresh, &codes[0], &client()).await.unwrap();
    assert_eq!(result.receipt().original_epoch().get(), 1);
    raw.close().await;
}

#[tokio::test]
async fn wrong_proof_preserves_pending_retry_and_cancel_never_recreates_it() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice").await;
    let (_, _, codes) = fixture.enrol(&alice).await;
    let pending = fixture.services.two_factor.start_original_password_pending(original(&fixture, "alice").await);
    assert!(matches!(
        fixture.services.complete_original_sign_in(&pending, "bad code", &client()).await,
        Err(ServiceError::InvalidCredentials)
    ));
    assert_eq!(fixture.services.two_factor.pending_account(&pending), Some(alice.id()));
    let good = fixture.services.complete_original_sign_in(&pending, &codes[0], &client()).await.unwrap();
    assert_eq!(good.receipt().original_epoch().get(), 0);
    let cancelled = fixture.services.two_factor.start_original_password_pending(original(&fixture, "alice").await);
    fixture.services.two_factor.end_pending(&cancelled);
    let before = fixture.database.two_factor().remaining_recovery_codes(alice.id()).await.unwrap();
    assert!(matches!(fixture.services.complete_original_sign_in(&cancelled, &codes[1], &client()).await, Err(ServiceError::NotFound)));
    assert_eq!(fixture.database.two_factor().remaining_recovery_codes(alice.id()).await.unwrap(), before);
}

#[tokio::test]
async fn suspension_rejects_original_pending_before_proof_and_reactivation_requires_new_pending() {
    let fixture = Fixture::new().await;
    let _root = fixture.user("root").await;
    let alice = fixture.user("alice").await;
    let (_, _, codes) = fixture.enrol(&alice).await;
    let pending = fixture.services.two_factor.start_original_password_pending(original(&fixture, "alice").await);
    fixture.database.admin().set_suspended(alice.id(), true).await.unwrap();
    let before = fixture.database.two_factor().remaining_recovery_codes(alice.id()).await.unwrap();
    assert!(matches!(fixture.services.complete_original_sign_in(&pending, &codes[0], &client()).await, Err(ServiceError::NotFound)));
    assert_eq!(fixture.database.two_factor().remaining_recovery_codes(alice.id()).await.unwrap(), before);
    fixture.database.admin().set_suspended(alice.id(), false).await.unwrap();
    assert!(matches!(fixture.services.complete_original_sign_in(&pending, &codes[0], &client()).await, Err(ServiceError::NotFound)));
    let fresh = fixture.services.two_factor.start_original_password_pending(original(&fixture, "alice").await);
    assert_eq!(fixture.services.complete_original_sign_in(&fresh, &codes[0], &client()).await.unwrap().receipt().account(), alice.id());
}
#[tokio::test]
async fn original_totp_success_claims_the_step_once_and_returns_same_epoch() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice").await;
    let (secret, step, _) = fixture.enrol(&alice).await;
    let pending = fixture.services.two_factor.start_original_password_pending(original(&fixture, "alice").await);
    let response = code(&secret, step + 1);
    let completed = fixture.services.complete_original_sign_in(&pending, &response, &client()).await.unwrap();
    assert_eq!(completed.receipt().original_epoch().get(), 0);
    let again = fixture.services.two_factor.start_original_password_pending(original(&fixture, "alice").await);
    assert!(matches!(
        fixture.services.complete_original_sign_in(&again, &response, &client()).await,
        Err(ServiceError::InvalidCredentials)
    ));
    assert_eq!(fixture.services.two_factor.pending_account(&again), Some(alice.id()));
}
