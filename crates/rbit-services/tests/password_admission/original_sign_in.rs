//! The closed first-factor orchestration, not a production issuer switch.
use super::*;
use rbit_core::Timestamp;
use rbit_services::two_factor::{OriginalSignInOutcome, SignInOutcome};
use rbit_storage::{
    count_work,
    sqlite::{audit::AuditAction, credential_revocations as intents, writer_compatibility::register},
};
use sqlx::sqlite::SqliteConnectOptions;

fn assert_send<T: Send>(_: &T) {}

#[tokio::test]
async fn original_first_factor_matches_legacy_success_work_and_audit_without_refreshing_its_epoch() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice", false).await;
    let client = ClientInfo::default();
    let (legacy, baseline) = count_work(fixture.services.sign_in(" alice ", PASSWORD.into(), &client)).await;
    let SignInOutcome::SignedIn(legacy) = legacy.unwrap() else { panic!("no MFA enrolled") };
    let (original, work) = count_work(fixture.services.sign_in_original(" alice ", PASSWORD.into(), &client)).await;
    let OriginalSignInOutcome::SignedIn(proven) = original.unwrap() else { panic!("no MFA enrolled") };
    assert_eq!(proven.user().id(), legacy.id());
    assert_eq!(proven.user().email, legacy.email);
    assert_eq!(proven.user().is_admin, legacy.is_admin);
    assert_eq!(proven.receipt().account(), alice.id());
    assert_eq!(proven.receipt().original_epoch().get(), 0);
    assert_eq!(work, baseline, "same queries/acquisitions, no second password worker");
    assert_eq!(fixture.passwords.admitted(), 0, "completed work releases its slot");
    let audit = fixture.services.audit.recent_for(&alice, 100).await.unwrap();
    let successes: Vec<_> = audit.iter().filter(|entry| entry.action == AuditAction::SignIn).collect();
    assert_eq!(successes.len(), 2);
    assert!(successes.iter().all(|entry| entry.detail == "password"));
    let raw = raw(&fixture).await;
    let mut tx = raw.begin().await.unwrap();
    intents::stage(&mut tx, alice.id(), Timestamp(20)).await.unwrap();
    tx.commit().await.unwrap();
    assert_eq!(proven.receipt().original_epoch().get(), 0, "later reset cannot restamp original proof");
    assert!(
        fixture
            .database
            .accounts()
            .find_session_user_at_epoch(alice.id(), false, proven.receipt().original_epoch())
            .await
            .unwrap()
            .is_none()
    );
    raw.close().await;
}

#[tokio::test]
async fn original_busy_and_closed_share_admission_without_failed_credential_charges() {
    for closed in [false, true] {
        let fixture = Fixture::new().await;
        let alice = fixture.user("alice", false).await;
        let client = ClientInfo::default();
        let before = fixture.services.audit.recent_for(&alice, 100).await.unwrap();
        let held = if closed {
            fixture.passwords.close();
            None
        } else {
            Some(fixture.passwords.begin().unwrap())
        };
        for _ in 0..8 {
            for name in ["alice", "missing"] {
                let operation = fixture.services.sign_in_original(name, PASSWORD.into(), &client);
                assert_send(&operation);
                unavailable(&operation.await);
            }
        }
        assert_eq!(fixture.passwords.admitted(), usize::from(!closed));
        assert_eq!(fixture.services.audit.recent_for(&alice, 100).await.unwrap(), before);
        drop(held);
        if !closed {
            assert!(matches!(
                fixture.services.sign_in_original("alice", PASSWORD.into(), &client).await.unwrap(),
                OriginalSignInOutcome::SignedIn(_)
            ));
        }
    }
}

#[tokio::test]
async fn current_intent_is_unavailable_without_failed_login_penalty_and_then_retries() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice", false).await;
    let raw = raw(&fixture).await;
    let mut tx = raw.begin().await.unwrap();
    let intent = intents::stage(&mut tx, alice.id(), Timestamp(20)).await.unwrap();
    tx.commit().await.unwrap();
    let client = ClientInfo::default();
    let before = fixture.services.audit.recent_for(&alice, 100).await.unwrap();
    for _ in 0..8 {
        unavailable(&fixture.services.sign_in_original("alice", PASSWORD.into(), &client).await);
    }
    assert_eq!(fixture.services.audit.recent_for(&alice, 100).await.unwrap(), before);
    let mut tx = raw.begin().await.unwrap();
    assert_eq!(
        intents::complete(&mut tx, &intent, intents::RevocationOutcome::EndedAll, Timestamp(21)).await.unwrap(),
        intents::Completion::Applied
    );
    tx.commit().await.unwrap();
    let OriginalSignInOutcome::SignedIn(proven) = fixture.services.sign_in_original("alice", PASSWORD.into(), &client).await.unwrap()
    else {
        panic!("no MFA enrolled")
    };
    assert_eq!(proven.receipt().original_epoch().get(), 1);
    raw.close().await;
}

#[tokio::test]
async fn wrong_password_and_suspension_keep_legacy_status_audit_and_lookup_counts() {
    for suspended in [false, true] {
        let fixture = Fixture::new().await;
        fixture.user("root", true).await;
        let alice = fixture.user("alice", false).await;
        if suspended {
            fixture.database.admin().set_suspended(alice.id(), true).await.unwrap();
        }
        let client = ClientInfo::default();
        let password = if suspended { PASSWORD } else { "wrong" };
        let (legacy, baseline) = count_work(fixture.services.sign_in("alice", password.into(), &client)).await;
        let (original, work) = count_work(fixture.services.sign_in_original("alice", password.into(), &client)).await;
        if suspended {
            assert!(matches!(legacy, Err(ServiceError::Forbidden)));
            assert!(matches!(original, Err(ServiceError::Forbidden)));
        } else {
            assert!(matches!(legacy, Err(ServiceError::InvalidCredentials)));
            assert!(matches!(original, Err(ServiceError::InvalidCredentials)));
        }
        assert_eq!(work, baseline);
        let audit = fixture.database.audit().list(None, ListFilter { search: "", limit: 100, offset: 0 }).await.unwrap();
        let failures: Vec<_> = audit.items.iter().filter(|entry| entry.action == AuditAction::SignInFailed).collect();
        assert_eq!(failures.len(), 2);
        assert_eq!(failures[0].detail, failures[1].detail);
        assert_eq!(failures[0].actor_name, failures[1].actor_name);
    }
}

async fn raw(fixture: &Fixture) -> sqlx::SqlitePool {
    sqlx::SqlitePool::connect_with(register(SqliteConnectOptions::new().filename(fixture.directory.path().join("rbit.db")))).await.unwrap()
}
