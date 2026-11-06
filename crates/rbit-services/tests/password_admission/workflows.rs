//! Real service delegates preserve authorization, event and provisioning side effects.

use super::*;

#[tokio::test]
async fn unauthorized_admin_creation_wins_before_validation_or_closed_work() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice", false).await;
    let current = fixture.services.accounts.start_session(&alice).await.unwrap();
    let before = fixture.database.accounts().count_users().await.unwrap();
    let audit_before = fixture.database.audit().list(None, ListFilter { search: "", limit: 100, offset: 0 }).await.unwrap();
    fixture.passwords.close();
    let invalid = SignUp { username: "../invalid".into(), email: "invalid".into(), password: "short".into() };
    assert!(matches!(fixture.services.admin.create_user(&alice, invalid, true).await, Err(ServiceError::Forbidden)));
    assert_eq!(fixture.passwords.admitted(), 0);
    assert_eq!(fixture.database.accounts().count_users().await.unwrap(), before);
    assert!(fixture.services.accounts.resume_session(&current).await.unwrap().is_some());
    let after = fixture.database.audit().list(None, ListFilter { search: "", limit: 100, offset: 0 }).await.unwrap();
    assert_eq!(after.items, audit_before.items);
}

#[tokio::test]
async fn provisioning_port_retains_initial_password_marker_and_one_post_commit_event() {
    let fixture = Fixture::new().await;
    let admin = fixture.user("root", true).await;
    let mut events = fixture.services.events.subscribe();
    let user = fixture.services.admin.create_user(&admin, input("provisioned"), false).await.unwrap();
    assert!(fixture.database.accounts().password_change_required(user.id()).await.unwrap());
    assert!(matches!(events.try_recv().unwrap(), rbit_services::DomainEvent::UserSignedUp { user: id } if id == user.id()));
    assert!(events.try_recv().is_err());
    assert_eq!(fixture.services.accounts.authenticate("provisioned", PASSWORD.into()).await.unwrap().id(), user.id());
    // Failed SQL uniqueness must not publish an additional registration event.
    assert!(matches!(fixture.services.accounts.create_user(input("provisioned"), Some(false)).await, Err(ServiceError::Conflict(_))));
    assert!(events.try_recv().is_err());
}

#[test]
fn storage_record_and_legacy_signup_paths_are_exact_feature_owned_types() {
    use std::any::TypeId;
    assert_eq!(TypeId::of::<SignUp>(), TypeId::of::<rbit_identity::accounts::SignUp>());
    assert_eq!(TypeId::of::<rbit_storage::NewUser>(), TypeId::of::<rbit_identity::accounts::NewUser>());
    assert_eq!(TypeId::of::<rbit_storage::sqlite::LoginRecord>(), TypeId::of::<rbit_identity::accounts::LoginRecord>());
    assert!(
        !format!("{:?}", SignUp { password: "never-log-this-password".into(), ..SignUp::default() }).contains("never-log-this-password")
    );
}
