use super::fixture::*;
use rbit_core::Timestamp;
use rbit_identity_models::{AccountLocale, credential::CredentialEpoch, sessions::EpochSessionSnapshot};
use rbit_services::{ServiceError, accounts::ClientInfo};
use rbit_storage::{SessionRecord, Work, count_work};

#[tokio::test]
async fn original_issue_and_resume_use_fixed_epoch_and_one_locale_aware_account_query() {
    let fixture = Fixture::new().await;
    fixture.database.preferences().set_locale(fixture.alice.id(), Some(AccountLocale::Spanish)).await.unwrap();
    let epoch: i64 = sqlx::query_scalar("SELECT authentication_epoch FROM user_logins WHERE account_id=?")
        .bind(fixture.alice.id().0)
        .fetch_one(&fixture.raw)
        .await
        .unwrap();
    assert_eq!(epoch, fixture.original.get());
    let proof = fixture.proof().await;
    let (issued, work) = count_work(fixture.services.accounts.start_original_session_for(&proof, &ClientInfo::default())).await;
    let issued = issued.unwrap();
    assert_eq!(work, Work { acquisitions: 1, statements: 1, cache_writes: 0 });
    assert_eq!(issued.session.snapshot().epoch(), fixture.original);
    assert_eq!(issued.session.user().locale, Some(AccountLocale::Spanish));
    assert_eq!(issued.session.user().user.id(), fixture.alice.id());
    let before = fixture.services.accounts.session_effect_report();
    let (resolved, work) = count_work(fixture.services.accounts.resume_original_session(&issued.token)).await;
    let resolved = resolved.unwrap().unwrap();
    assert_eq!(work, Work { acquisitions: 1, statements: 1, cache_writes: 0 });
    assert_eq!(resolved.user().locale, Some(AccountLocale::Spanish));
    assert_eq!(fixture.services.accounts.session_effect_report(), before, "common resume does not create an owned mutation task");
    assert_eq!(fixture.services.accounts.original_sessions(&resolved).await.unwrap().len(), 1);
}

#[tokio::test]
async fn old_password_receipt_after_committed_reset_never_issues_at_a_fresh_epoch() {
    let fixture = Fixture::new().await;
    let proof = fixture.proof().await;
    let existing = fixture.issue().await;
    fixture
        .database
        .profiles()
        .set_password_hash_with_intent(fixture.alice.id(), "replacement PHC", fixture.original, Timestamp::now())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        fixture.services.accounts.start_original_session_for(&proof, &ClientInfo::default()).await,
        Err(ServiceError::NotFound)
    ));
    assert_eq!(proof.original_epoch(), fixture.original);
    assert!(fixture.services.accounts.resume_original_session(&existing.token).await.unwrap().is_none());
    assert!(fixture.kv.sessions().epoch_sidecar().get_snapshot(existing.token.digest()).await.unwrap().is_none());
}

#[tokio::test]
async fn active_adapter_never_adopts_unbound_legacy_or_initial_bindings() {
    let fixture = Fixture::new().await;
    let now = Timestamp::now();
    let token = rbit_auth::SessionToken::generate();
    let record = SessionRecord::at(fixture.alice.id(), Timestamp(now.0 + 3600), None, "legacy", now);
    fixture.kv.sessions().put(token.digest(), record.clone()).await.unwrap();
    let (result, work) = count_work(fixture.services.accounts.resume_original_session(&token)).await;
    assert!(result.unwrap().is_none());
    assert_eq!(work, Work::default(), "no latest-epoch adoption query");
    let initial = rbit_auth::SessionToken::generate();
    fixture
        .kv
        .sessions()
        .epoch_sidecar()
        .put_if_absent(initial.digest(), record, fixture.alice.id(), CredentialEpoch::INITIAL)
        .await
        .unwrap();
    assert!(fixture.services.accounts.resume_original_session(&initial).await.unwrap().is_none());
    assert!(fixture.kv.sessions().epoch_sidecar().get_snapshot(token.digest()).await.unwrap().unwrap().is_legacy_initial());
}

#[tokio::test]
async fn original_list_single_revoke_other_revoke_and_logout_share_captured_bindings() {
    let fixture = Fixture::new().await;
    let current = fixture.issue().await;
    let other = fixture.issue().await;
    let rows = fixture.services.accounts.original_sessions(&current.session).await.unwrap();
    assert_eq!(rows.len(), 2);
    let id = rows.iter().find(|row| !row.is_current).unwrap().id.clone();
    let (revoked, work) =
        count_work(fixture.services.accounts.revoke_original_session(&current.session, &id, &ClientInfo::default())).await;
    revoked.unwrap();
    assert_eq!(work, Work { acquisitions: 1, statements: 1, cache_writes: 0 }, "owned audit remains visible");
    assert!(fixture.services.accounts.resume_original_session(&other.token).await.unwrap().is_none());
    let third = fixture.issue().await;
    assert_eq!(fixture.services.accounts.revoke_other_original_sessions(&current.session, &ClientInfo::default()).await.unwrap(), 1);
    assert!(fixture.services.accounts.resume_original_session(&third.token).await.unwrap().is_none());
    assert!(fixture.services.accounts.resume_original_session(&current.token).await.unwrap().is_some());
    fixture.services.accounts.end_original_session(&current.token).await.unwrap();
    assert!(fixture.services.accounts.resume_original_session(&current.token).await.unwrap().is_none());
    let audit = fixture.services.audit.recent_for(&fixture.alice, 100).await.unwrap();
    assert!(audit.iter().any(|entry| entry.action == rbit_services::audit::AuditAction::SessionRevoked));
    assert!(audit.iter().any(|entry| entry.action == rbit_services::audit::AuditAction::OtherSessionsRevoked));
}

#[tokio::test]
async fn closed_scope_refuses_new_mutations_but_common_reads_remain_read_only() {
    let fixture = Fixture::new().await;
    let current = fixture.issue().await;
    let proof = fixture.proof().await;
    fixture.services.accounts.shutdown_session_effects().await;
    assert!(fixture.services.accounts.resume_original_session(&current.token).await.unwrap().is_some());
    assert!(matches!(
        fixture.services.accounts.start_original_session_for(&proof, &ClientInfo::default()).await,
        Err(ServiceError::Unavailable { .. })
    ));
    assert!(matches!(fixture.services.accounts.end_original_session(&current.token).await, Err(ServiceError::Unavailable { .. })));
    assert!(fixture.kv.sessions().epoch_sidecar().get_snapshot(current.token.digest()).await.unwrap().is_some());
    assert_eq!(fixture.services.accounts.session_effect_report().active_owners, 0);
}

#[tokio::test]
async fn original_expiry_purge_removes_bound_metadata_without_touching_live_records() {
    let fixture = Fixture::new().await;
    let current = fixture.issue().await;
    let now = Timestamp::now();
    let expired = SessionRecord::at(fixture.alice.id(), Timestamp(now.0 - 1), None, "expired", Timestamp(now.0 - 100));
    let token = rbit_auth::SessionToken::generate();
    fixture.kv.sessions().epoch_sidecar().put_if_absent(token.digest(), expired, fixture.alice.id(), fixture.original).await.unwrap();
    assert_eq!(fixture.services.accounts.purge_expired_original_sessions().await.unwrap(), 1);
    assert!(fixture.kv.sessions().epoch_sidecar().get_snapshot(token.digest()).await.unwrap().is_none());
    assert!(fixture.services.accounts.resume_original_session(&current.token).await.unwrap().is_some());
}

#[tokio::test]
async fn rare_renewal_is_owned_but_maximum_age_still_rejects_before_sql() {
    let fixture = Fixture::new().await;
    let now = Timestamp::now();
    let token = rbit_auth::SessionToken::generate();
    let record = SessionRecord::at(fixture.alice.id(), Timestamp(now.0 + 3600), None, "seen", Timestamp(now.0 - 600));
    fixture.kv.sessions().epoch_sidecar().put_if_absent(token.digest(), record, fixture.alice.id(), fixture.original).await.unwrap();
    let before = fixture.services.accounts.session_effect_report();
    let (result, work) = count_work(fixture.services.accounts.resume_original_session(&token)).await;
    assert!(result.unwrap().is_some());
    assert_eq!(work, Work { acquisitions: 1, statements: 1, cache_writes: 0 });
    let after = fixture.services.accounts.session_effect_report();
    assert_eq!(after.completed, before.completed + 1);
    assert_eq!(after.active_owners, 0);
    let stored = fixture.kv.sessions().epoch_sidecar().get_snapshot(token.digest()).await.unwrap().unwrap();
    assert!(stored.record().last_seen_at >= now);
    let old = rbit_auth::SessionToken::generate();
    let record = SessionRecord::at(fixture.alice.id(), Timestamp(now.0 + 3600), None, "too old", Timestamp(now.0 - 2 * 86400));
    fixture.kv.sessions().epoch_sidecar().put_if_absent(old.digest(), record, fixture.alice.id(), fixture.original).await.unwrap();
    let (result, work) = count_work(fixture.services.accounts.resume_original_session(&old)).await;
    assert!(result.unwrap().is_none());
    assert_eq!(work, Work::default());
    assert!(fixture.kv.sessions().epoch_sidecar().get_snapshot(old.digest()).await.unwrap().is_none());
}

#[tokio::test]
async fn unpolled_revoke_future_counts_before_await_and_drops_without_mutating() {
    let fixture = Fixture::new().await;
    let current = fixture.issue().await;
    let client = ClientInfo::default();
    let before = fixture.services.accounts.session_effect_report();
    let operation = fixture.services.accounts.revoke_other_original_sessions(&current.session, &client);
    assert_eq!(fixture.services.accounts.session_effect_report().active_owners, before.active_owners + 1);
    drop(operation);
    assert_eq!(fixture.services.accounts.session_effect_report(), before);
    assert!(fixture.services.accounts.resume_original_session(&current.token).await.unwrap().is_some());
}
