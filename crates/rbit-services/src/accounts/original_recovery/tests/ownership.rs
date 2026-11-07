use super::*;
use crate::credentials;
use rbit_core::Timestamp;
use rbit_identity::sessions::SessionRecoveryDisposition as Disposition;
use rbit_identity_models::credential::CredentialEpoch;
use rbit_storage::{Work, count_work};
use std::task::Poll;

#[tokio::test]
async fn no_recovery_wait_queue_or_unpolled_side_effect_is_created_under_busy_admission() {
    let fixture = Fixture::new().await;
    let intent = fixture.change().await;
    let held = fixture.services.accounts.recover_original_session_intent(intent);
    assert_eq!(fixture.services.accounts.session_effect_report().active_owners, 1);
    let busy = fixture.services.accounts.recover_original_session_intent(intent);
    assert_eq!(fixture.services.accounts.session_effect_report().active_owners, 1, "busy did not retain another scope owner");
    assert!(matches!(busy.await, Err(ServiceError::Unavailable { .. })));
    assert_eq!(fixture.outcome(intent.id).await, None);
    drop(held);
    assert_eq!(fixture.services.accounts.session_effect_report().active_owners, 0);
    assert_eq!(fixture.recover(intent).await.unwrap(), Disposition::Completed);
}

#[tokio::test]
async fn gate_is_reserved_before_sql_and_busy_leaves_the_next_password_transaction_unstarted() {
    let fixture = Fixture::new().await;
    let intent = fixture.change().await;
    let held = fixture.services.accounts.recover_original_session_intent(intent);
    let permit = fixture.services.accounts.reserve_password_change().unwrap();
    let finalizer = fixture.services.accounts.original_credential_finalizer();
    let profiles = fixture.database.profiles();
    let account = fixture.account;
    let result = credentials::finish_update(permit, move |context| async move {
        let operation = finalizer.begin(&context)?;
        let intent = profiles
            .set_password_hash_with_intent(account, "must not commit", CredentialEpoch::new(intent.target_epoch).unwrap(), Timestamp::now())
            .await?
            .unwrap();
        operation.committed(&intent).await
    })
    .await;
    assert!(matches!(result, Err(ServiceError::Unavailable { .. })));
    let state: (String, i64) = sqlx::query_as("SELECT password_hash,authentication_epoch FROM user_logins WHERE account_id=?")
        .bind(account.0)
        .fetch_one(&fixture.raw)
        .await
        .unwrap();
    assert_eq!(state, ("new PHC".into(), intent.target_epoch));
    drop(held);
}

#[tokio::test]
async fn cancellation_after_sql_commit_retains_real_reconciliation_through_close() {
    let fixture = Fixture::new().await;
    let permit = fixture.services.accounts.reserve_password_change().unwrap();
    let finalizer = fixture.services.accounts.original_credential_finalizer();
    let profiles = fixture.database.profiles();
    let account = fixture.account;
    let epoch = fixture.original;
    let promotion = fixture.current.promotion_spec().unwrap();
    let (committed, commit) = tokio::sync::oneshot::channel();
    let (release, resume) = tokio::sync::oneshot::channel();
    let request = tokio::spawn(credentials::finish_update(permit, move |context| async move {
        let operation = finalizer.begin(&context)?;
        let intent = profiles
            .set_password_hash_if_current_preserving(account, "new PHC", "old PHC", epoch, Timestamp::now(), promotion)
            .await?
            .unwrap();
        committed.send(intent).unwrap();
        resume.await.unwrap();
        operation.committed(&intent).await
    }));
    let intent = commit.await.unwrap();
    request.abort();
    assert!(request.await.unwrap_err().is_cancelled());
    assert!(matches!(fixture.services.accounts.recover_original_session_intent(intent).await, Err(ServiceError::Unavailable { .. })));
    let mut shutdown = Box::pin(fixture.services.accounts.shutdown_session_effects());
    assert!(std::future::poll_fn(|cx| Poll::Ready(shutdown.as_mut().poll(cx))).await.is_pending());
    assert!(fixture.services.accounts.session_effect_report().closed);
    release.send(()).unwrap();
    shutdown.await;
    let report = fixture.services.accounts.session_effect_report();
    assert!(report.drained());
    assert_eq!(report.abnormal, 0);
    assert_eq!(fixture.outcome(intent.id).await.as_deref(), Some("kept_current"));
    let sessions = fixture.kv.sessions().epoch_sidecar();
    assert_eq!(sessions.get_live([1; 32]).await.unwrap().unwrap().epoch().get(), intent.target_epoch);
    assert!(sessions.get_snapshot([2; 32]).await.unwrap().is_none());
}

#[tokio::test]
async fn foreign_scope_context_is_refused_before_pair_reads_or_effects() {
    let fixture = Fixture::new().await;
    let other = Fixture::new().await;
    let intent = fixture.change().await;
    let permit = other.services.accounts.reserve_password_change().unwrap();
    let finalizer = fixture.services.accounts.original_credential_finalizer();
    let (result, work) =
        count_work(credentials::finish_update(permit, move |context| async move { finalizer.committed(&context, &intent).await })).await;
    assert!(result.is_err());
    assert_eq!(work, Work::default());
    assert_eq!(fixture.outcome(intent.id).await, None);
    assert_eq!(fixture.kv.sessions().epoch_sidecar().get_live([1; 32]).await.unwrap().unwrap().epoch(), fixture.original);
}

#[tokio::test]
async fn independent_service_compositions_share_the_backend_recovery_gate() {
    let fixture = Fixture::new().await;
    let other = crate::Services::new(Arc::new(fixture.config.clone()), &fixture.database, &fixture.kv.clone()).await.unwrap();
    let intent = fixture.change().await;
    let held = fixture.services.accounts.recover_original_session_intent(intent);
    let (result, work) = count_work(other.accounts.recover_original_session_intent(intent)).await;
    assert!(matches!(result, Err(ServiceError::Unavailable { .. })));
    assert_eq!(work, Work::default(), "foreign composition cannot classify or apply while another owner holds the backend");
    assert_eq!(other.accounts.session_effect_report().active_owners, 0);
    assert_eq!(fixture.outcome(intent.id).await, None);
    assert_eq!(fixture.kv.sessions().epoch_sidecar().get_live([1; 32]).await.unwrap().unwrap().epoch(), fixture.original);
    drop(held);
    assert_eq!(other.accounts.recover_original_session_intent(intent).await.unwrap(), Disposition::Completed);
    let now = Timestamp::now();
    let target = CredentialEpoch::new(intent.target_epoch).unwrap();
    let sessions = fixture.kv.sessions().epoch_sidecar();
    sessions
        .put_bound(
            [9; 32],
            rbit_storage::SessionRecord::at(fixture.account, Timestamp(now.0 + 3600), None, "after ack", now),
            fixture.account,
            target,
        )
        .await
        .unwrap();
    assert_eq!(fixture.recover(intent).await.unwrap(), Disposition::AlreadyResolved);
    assert_eq!(other.accounts.recover_original_session_intent(intent).await.unwrap(), Disposition::AlreadyResolved);
    assert!(sessions.get_live([9; 32]).await.unwrap().is_some(), "neither composition replays cleanup after paired ACK");
}
