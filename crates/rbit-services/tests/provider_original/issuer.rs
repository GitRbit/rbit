//! Intermediate provider facts never replace final authoritative issuance.
use super::*;

#[tokio::test]
async fn captured_user_is_informational_and_final_issuer_alone_reloads_authoritative_flags() {
    let f = Fixture::new(false, false).await;
    let alice = f.user("alice").await;
    f.linked(&alice, "linked").await;
    let store = f.database.external_identities();
    let (_, origin) = ProviderAuthentication::capture_linked(&store, "corp", "linked").await.unwrap().unwrap();
    let (proven, work) = count_work(std::future::ready(ProviderAuthentication::first_factor(origin))).await;
    assert_eq!(work, Work::default(), "intermediate receipt adds no second lookup");
    let proven = proven.unwrap();
    sqlx::query("UPDATE accounts SET name='renamed' WHERE id=?").bind(alice.id().0).execute(&f.raw).await.unwrap();
    sqlx::query("UPDATE user_logins SET is_admin=0 WHERE account_id=?").bind(alice.id().0).execute(&f.raw).await.unwrap();
    assert_eq!(proven.user().name().as_str(), "alice");
    assert!(proven.user().is_admin, "captured user is not refreshed or issuance authority");
    let (issued, work) = count_work(f.services.accounts.start_original_session_for(proven.receipt(), &client())).await;
    let issued = issued.unwrap();
    assert_eq!(work, Work { acquisitions: 1, statements: 1, cache_writes: 0 });
    assert_eq!(issued.session.user().user.name().as_str(), "renamed");
    assert!(!issued.session.user().user.is_admin);
}

#[tokio::test]
async fn old_provider_origin_never_adopts_reset_epoch_and_captured_pending_stays_unavailable() {
    let f = Fixture::new(false, false).await;
    let alice = f.user("alice").await;
    f.linked(&alice, "linked").await;
    let store = f.database.external_identities();
    let (_, before_reset) = ProviderAuthentication::capture_linked(&store, "corp", "linked").await.unwrap().unwrap();
    let intent = f.stage(alice.id()).await;
    let (_, pending) = ProviderAuthentication::capture_linked(&store, "corp", "linked").await.unwrap().unwrap();
    f.complete_intent(&intent).await;
    assert!(ProviderAuthentication::first_factor(pending).is_none());
    let old = ProviderAuthentication::first_factor(before_reset).unwrap();
    assert!(f.services.accounts.start_original_session_for(old.receipt(), &client()).await.is_err());
    let (_, fresh) = ProviderAuthentication::capture_linked(&store, "corp", "linked").await.unwrap().unwrap();
    let fresh = ProviderAuthentication::first_factor(fresh).unwrap();
    assert_eq!(fresh.receipt().original_epoch().get(), intent.target_epoch);
    f.services.accounts.start_original_session_for(fresh.receipt(), &client()).await.unwrap();
}

#[tokio::test]
async fn deleted_account_id_cannot_rehabilitate_an_old_provider_origin() {
    let f = Fixture::new(false, false).await;
    f.user("root").await;
    let alice = f.user("alice").await;
    f.linked(&alice, "linked").await;
    let (_, origin) = ProviderAuthentication::capture_linked(&f.database.external_identities(), "corp", "linked").await.unwrap().unwrap();
    let old = ProviderAuthentication::first_factor(origin).unwrap();
    assert_eq!(f.database.admin().delete_user(alice.id()).await.unwrap(), rbit_storage::sqlite::admin::Moderation::Applied);
    let bob = f.user("bob").await;
    assert_eq!(bob.id(), alice.id(), "exercise actual SQLite highest-ID reuse");
    assert!(f.services.accounts.start_original_session_for(old.receipt(), &client()).await.is_err());
    assert!(f.database.accounts().find_session_user_at_epoch(bob.id(), false, old.receipt().original_epoch()).await.unwrap().is_none());
}
