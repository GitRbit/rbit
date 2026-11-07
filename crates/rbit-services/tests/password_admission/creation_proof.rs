//! Explicit signup producer over the same SQL authority and `PasswordWork` owner.
use super::*;
use rbit_core::Timestamp;
use rbit_storage::count_work;

#[tokio::test]
async fn original_signup_keeps_setup_admission_order_and_returns_transaction_epoch() {
    let fixture = Fixture::new().await;
    let setup = fixture.services.accounts.setup_token().unwrap().to_owned();
    let held = fixture.passwords.begin().unwrap();
    assert!(matches!(fixture.services.accounts.sign_up_original(input("alice"), "wrong").await, Err(ServiceError::Invalid(_))));
    unavailable(&fixture.services.accounts.sign_up_original(input("alice"), &setup).await);
    assert_eq!(fixture.database.accounts().count_users().await.unwrap(), 0);
    drop(held);
    assert_eq!(fixture.passwords.admitted(), 0);
    let (original, work) = count_work(fixture.services.accounts.sign_up_original(input("alice"), &setup)).await;
    let original = original.unwrap();
    assert_eq!(fixture.passwords.admitted(), 0, "completed signup releases its active worker slot");
    assert!(original.user().is_admin);
    assert_eq!(original.receipt().original_epoch().get(), 1);
    assert_eq!(original.receipt().account(), original.user().id());
    // One preflight count + three insertion/policy statements + one global allocation.
    assert_eq!(work.statements, 5);
    fixture
        .database
        .profiles()
        .set_password_hash_with_intent(original.user().id(), "new PHC", original.receipt().original_epoch(), Timestamp(20))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(original.receipt().original_epoch().get(), 1);
    assert!(
        fixture
            .database
            .accounts()
            .find_session_user_at_epoch(original.user().id(), false, original.receipt().original_epoch())
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn original_admin_provisioning_keeps_authorization_and_busy_before_hash_or_writes() {
    let f = Fixture::new().await;
    let admin = f.user("root", true).await;
    let outsider = f.user("outsider", false).await;
    let held = f.passwords.begin().unwrap();
    let (denied, work) = count_work(f.services.admin.create_user_original(&outsider, input("denied"), false)).await;
    assert!(matches!(denied, Err(ServiceError::Forbidden)));
    assert_eq!(work, rbit_storage::Work::default());
    let (busy, work) = count_work(f.services.admin.create_user_original(&admin, input("busy"), false)).await;
    unavailable(&busy);
    assert_eq!(work, rbit_storage::Work::default());
    drop(held);
    let (legacy, before) = count_work(f.services.admin.create_user(&admin, input("legacy"), false)).await;
    legacy.unwrap();
    let (original, after) = count_work(f.services.admin.create_user_original(&admin, input("original"), false)).await;
    let original = original.unwrap();
    assert_eq!(after.acquisitions, before.acquisitions);
    assert_eq!(after.statements, before.statements + 1);
    assert!(f.database.accounts().password_change_required(original.id()).await.unwrap());
    let snapshot = f.database.accounts().find_login_snapshot("original").await.unwrap().unwrap();
    assert!(snapshot.fence.epoch.get() > 0);
    assert_eq!(f.passwords.admitted(), 0);
    assert!(f.database.accounts().find_account_by_name("busy").await.unwrap().is_none());
    assert!(f.database.accounts().find_account_by_name("denied").await.unwrap().is_none());
}

#[tokio::test]
async fn original_offline_provisioning_has_one_global_allocation_and_retains_first_admin_rule() {
    let f = Fixture::new().await;
    let config = Config::default();
    let store = f.database.accounts();
    let (created, work) = count_work(rbit_services::accounts::create_user_offline_original(&config, &store, input("alice"), None)).await;
    assert!(created.unwrap().is_admin);
    assert_eq!(work, rbit_storage::Work { acquisitions: 1, statements: 4, cache_writes: 0 });
    let snapshot = store.find_login_snapshot("alice").await.unwrap().unwrap();
    assert_eq!(snapshot.fence.epoch.get(), 1);
}
