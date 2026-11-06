//! Administrative and overloaded reset behavior against the shared fixture.

use super::*;

#[tokio::test]
async fn overloaded_reset_preserves_token_password_and_live_sessions() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice", false).await;
    let session = fixture.services.accounts.start_session(&alice).await.unwrap();
    fixture.recovery.request_password_reset(alice.email.as_str(), "198.51.100.9").await.unwrap();
    let token = token_in(&fixture.mail(1).await, "/password/reset/");
    let occupied = fixture.passwords.begin().unwrap();
    assert!(matches!(
        fixture.recovery.reset_password(&token, NEW_PASSWORD.into()).await,
        Err(ServiceError::Unavailable { retry_after_seconds: 1 })
    ));
    assert!(fixture.recovery.reset_target(&token).await.is_ok());
    assert!(fixture.services.accounts.resume_session(&session).await.unwrap().is_some());
    drop(occupied);
    assert!(fixture.services.accounts.authenticate("alice", PASSWORD.into()).await.is_ok());
    fixture.recovery.reset_password(&token, NEW_PASSWORD.into()).await.unwrap();
    assert!(fixture.services.accounts.resume_session(&session).await.unwrap().is_none());
    assert!(matches!(fixture.recovery.reset_target(&token).await, Err(ServiceError::NotFound)));
}

#[tokio::test]
async fn administrators_send_resets_and_set_passwords() {
    let fixture = Fixture::new().await;
    let admin = fixture.user("root", true).await;
    let alice = fixture.user("alice", false).await;
    let session = fixture.services.accounts.start_session(&alice).await.unwrap();
    let guard = fixture.services.accounts.guard_password_change(alice.id(), &session);
    let mut events = fixture.services.events.subscribe();

    assert!(matches!(fixture.recovery.admin_send_password_reset(&alice, admin.id()).await, Err(ServiceError::Forbidden)));
    assert_eq!(fixture.recovery.admin_send_password_reset(&admin, alice.id()).await.unwrap(), alice.email);
    assert_eq!(fixture.mail(1).await.to, alice.email);
    let missing = fixture.recovery.admin_send_password_reset(&admin, AccountId(999)).await;
    assert!(matches!(missing, Err(ServiceError::NotFound)));

    let denied = fixture.recovery.admin_set_password(&alice, admin.id(), NEW_PASSWORD.to_owned()).await;
    assert!(matches!(denied, Err(ServiceError::Forbidden)));
    let own = fixture.recovery.admin_set_password(&admin, admin.id(), NEW_PASSWORD.to_owned()).await;
    assert!(matches!(own, Err(ServiceError::Invalid(_))));
    let short = fixture.recovery.admin_set_password(&admin, alice.id(), "short".to_owned()).await;
    assert!(matches!(short, Err(ServiceError::Invalid(_))));

    // The emailed link stops working once an administrator has set the password.
    let token = token_in(&fixture.mail(1).await, "/password/reset/");
    fixture.recovery.admin_set_password(&admin, alice.id(), NEW_PASSWORD.to_owned()).await.unwrap();
    assert!(fixture.services.accounts.resume_session(&session).await.unwrap().is_none(), "revoked before the change returns");
    guard.finish(false).await;
    assert_eq!(events.recv().await.unwrap(), DomainEvent::PasswordChanged { user: alice.id() });
    assert!(fixture.services.accounts.authenticate("alice", NEW_PASSWORD.to_owned()).await.is_ok());
    assert!(matches!(fixture.recovery.reset_target(&token).await, Err(ServiceError::NotFound)));

    let verified = fixture.recovery.verified_among(&admin, &[admin.id(), alice.id()]).await.unwrap();
    assert!(verified.is_empty());
    assert!(matches!(fixture.recovery.verified_among(&alice, &[alice.id()]).await, Err(ServiceError::Forbidden)));
}
