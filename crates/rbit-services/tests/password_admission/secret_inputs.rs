//! Eager ownership retains the same future/Send API without doing work before polling.
use super::*;
use rbit_services::two_factor::SecondFactorProof;
use std::future::Future;

fn discard_send(future: impl Future + Send) {
    drop(future);
}

#[tokio::test]
async fn unpolled_outer_password_entries_have_no_admission_proof_or_store_side_effects() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice", false).await;
    let client = ClientInfo::default();
    let before = fixture.database.accounts().find_login("alice").await.unwrap().unwrap().password_hash;
    discard_send(fixture.services.sign_in("alice", PASSWORD.into(), &client));
    discard_send(fixture.services.authenticate_git_password("alice", PASSWORD.into(), &client));
    discard_send(fixture.services.profiles.change_username(&alice, PASSWORD.into(), "renamed"));
    discard_send(fixture.services.profiles.change_email(&alice, PASSWORD.into(), "new@example.com"));
    discard_send(fixture.services.profiles.change_password(&alice, PASSWORD.into(), REPLACEMENT.into()));
    discard_send(fixture.services.recovery.reset_password("invalid-token", REPLACEMENT.into()));
    discard_send(fixture.services.recovery.admin_set_password(&alice, alice.id(), REPLACEMENT.into()));
    discard_send(fixture.services.remove_passkey(&alice, 1, PASSWORD.into(), SecondFactorProof::Code("invalid"), &client));
    discard_send(fixture.services.disable_two_factor(&alice, PASSWORD.into(), SecondFactorProof::Code("invalid"), &client));
    discard_send(fixture.services.regenerate_recovery_codes(&alice, PASSWORD.into(), SecondFactorProof::Code("invalid"), &client));
    assert_eq!(fixture.passwords.admitted(), 0);
    assert_eq!(fixture.services.accounts.session_effect_report().active_owners, 0);
    let after = fixture.database.accounts().find_login("alice").await.unwrap().unwrap();
    assert_eq!(after.password_hash, before);
    assert_eq!(after.user.email, alice.email);
    assert!(fixture.database.accounts().find_login("renamed").await.unwrap().is_none());
}

struct ConversionMustNotRun;
impl<'a> From<ConversionMustNotRun> for SecondFactorProof<'a> {
    fn from(_: ConversionMustNotRun) -> Self {
        panic!("proof conversion ran before missing passkey lookup");
    }
}

#[tokio::test]
async fn owned_secret_early_errors_preserve_authorization_validation_and_lookup_precedence() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice", false).await;
    fixture.passwords.close();
    assert!(matches!(fixture.services.recovery.admin_set_password(&alice, alice.id(), "short".into()).await, Err(ServiceError::Forbidden)));
    assert!(matches!(
        fixture.services.profiles.change_password(&alice, PASSWORD.into(), "short".into()).await,
        Err(ServiceError::Invalid(_))
    ));
    assert!(matches!(fixture.services.recovery.reset_password("not-a-token", REPLACEMENT.into()).await, Err(ServiceError::NotFound)));
    let client = ClientInfo::default();
    assert!(matches!(
        fixture.services.remove_passkey(&alice, 1, PASSWORD.into(), ConversionMustNotRun, &client).await,
        Err(ServiceError::NotFound)
    ));
    unavailable(&fixture.services.profiles.change_password(&alice, PASSWORD.into(), REPLACEMENT.into()).await);
    unavailable(&fixture.services.disable_two_factor(&alice, PASSWORD.into(), SecondFactorProof::Code("invalid"), &client).await);
    assert_eq!(fixture.passwords.admitted(), 0);
    assert_eq!(fixture.services.accounts.session_effect_report().active_owners, 0);
}
