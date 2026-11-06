//! Deterministic private post-lookup barriers over real temporary SQLite/redb stores.
use super::*;
#[path = "../../../tests/support/authenticator.rs"]
mod authenticator;
use authenticator::Authenticator;
mod fixture;
use fixture::*;
use rbit_storage::{Work, count_work};

#[tokio::test]
async fn reset_after_lookup_cannot_consume_any_original_second_factor() {
    for factor in Factor::ALL {
        invalidated(factor, Change::Reset).await;
    }
}

#[tokio::test]
async fn suspension_after_lookup_cannot_consume_any_original_second_factor() {
    for factor in Factor::ALL {
        invalidated(factor, Change::Suspend).await;
    }
}

#[tokio::test]
async fn current_pending_after_lookup_cannot_consume_any_original_second_factor() {
    for factor in Factor::ALL {
        invalidated(factor, Change::Pending).await;
    }
}

async fn invalidated(factor: Factor, change: Change) {
    let fixture = Fixture::new().await;
    let material = fixture.install(factor).await;
    let pending = fixture.pending(true).await;
    let answer = material.answer(&fixture, &pending).await;
    let before = fixture.proof_state().await;
    let audit_before = fixture.services.audit.recent_for(&fixture.alice, 100).await.unwrap();
    let (selected, at_select) = tokio::sync::oneshot::channel();
    let (resume, resumed) = tokio::sync::oneshot::channel();
    let client = client();
    let complete = fixture.services.complete_second_factor_after(&pending, factor.proof(&answer), &client, true, async {
        selected.send(()).unwrap();
        resumed.await.unwrap();
    });
    let mutation = async {
        at_select.await.unwrap();
        fixture.change(change).await;
        resume.send(()).unwrap();
    };
    let (result, ()) = tokio::join!(complete, mutation);
    assert!(matches!(result, Err(ServiceError::NotFound)), "{factor:?}/{change:?}");
    assert_eq!(fixture.proof_state().await, before, "no counter, TOTP step or recovery use after invalidation");
    assert!(fixture.services.two_factor.pending_account(&pending).is_none());
    let audit_after = fixture.services.audit.recent_for(&fixture.alice, 100).await.unwrap();
    assert_eq!(audit_after.len(), audit_before.len(), "invalidated origin is not a wrong-factor audit");
    fixture.recover(change).await;
    let fresh = fixture.pending(true).await;
    let answer = material.answer(&fixture, &fresh).await;
    let proven = fixture.services.complete_original_sign_in(&fresh, factor.proof(&answer), &client).await.unwrap();
    assert_eq!(proven.receipt().account(), fixture.alice.id());
    assert_eq!(proven.receipt().original_epoch().get(), if matches!(change, Change::Reset) { 2 } else { 1 });
    fixture.raw.close().await;
}

#[tokio::test]
async fn successful_original_claims_match_legacy_statement_and_acquisition_counts() {
    for factor in Factor::ALL {
        let legacy = Fixture::new().await;
        let material = legacy.install(factor).await;
        let pending = legacy.pending(false).await;
        let answer = material.answer(&legacy, &pending).await;
        let (result, baseline) = count_work(legacy.services.complete_sign_in(&pending, factor.proof(&answer), &client())).await;
        assert_eq!(result.unwrap().id(), legacy.alice.id());
        let original = Fixture::new().await;
        let material = original.install(factor).await;
        let pending = original.pending(true).await;
        let answer = material.answer(&original, &pending).await;
        let (result, work) = count_work(original.services.complete_original_sign_in(&pending, factor.proof(&answer), &client())).await;
        assert_eq!(result.unwrap().receipt().account(), original.alice.id());
        assert_eq!(work, baseline, "{factor:?}: no success-path diagnostic query or extra acquisition");
        assert_ne!(work, Work::default());
        legacy.raw.close().await;
        original.raw.close().await;
    }
}
