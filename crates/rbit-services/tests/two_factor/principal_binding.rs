//! A stale in-flight user snapshot must not turn a reclaimed name into password authority.
use super::*;
use rbit_storage::{Work, count_work};
const BOB_PASSWORD: &str = "a different secure password";

#[tokio::test]
async fn reclaimed_username_cannot_supply_another_accounts_settings_password() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice").await;
    let bob = fixture
        .services
        .accounts
        .create_user(SignUp { username: "bob".into(), email: "bob@example.com".into(), password: BOB_PASSWORD.into() }, None)
        .await
        .unwrap();
    let (_, _, alice_codes) = fixture.enrol(&alice).await;
    fixture.enrol(&bob).await;
    let before = fixture.database.two_factor().remaining_recovery_codes(alice.id()).await.unwrap();
    // This is the exact interleaving: request extracted Alice, then her old name
    // was reclaimed before its password read. Numeric principal is unchanged.
    fixture.database.accounts().rename(alice.id(), &"renamed".parse().unwrap()).await.unwrap();
    fixture.database.accounts().rename(bob.id(), &"alice".parse().unwrap()).await.unwrap();
    assert_eq!(fixture.database.accounts().find_login(alice.name().as_str()).await.unwrap().unwrap().user.id(), bob.id());
    let (result, work) =
        count_work(fixture.services.disable_two_factor(&alice, BOB_PASSWORD.into(), alice_codes[0].as_str(), &client())).await;
    assert!(matches!(result, Err(ServiceError::InvalidCredentials)));
    assert_eq!(work, Work { acquisitions: 2, statements: 2, cache_writes: 0 }, "one principal lookup plus unchanged failed-step-up audit");
    assert!(fixture.services.two_factor.is_enabled(alice.id()).await.unwrap());
    assert_eq!(
        fixture.database.two_factor().remaining_recovery_codes(alice.id()).await.unwrap(),
        before,
        "wrong principal password must not consume Alice's proof"
    );
    // Own password and the SAME unused proof still work after the name changes.
    fixture.services.disable_two_factor(&alice, PASSWORD.into(), alice_codes[0].as_str(), &client()).await.unwrap();
    assert!(!fixture.services.two_factor.is_enabled(alice.id()).await.unwrap());
    assert!(fixture.services.two_factor.is_enabled(bob.id()).await.unwrap());
}
