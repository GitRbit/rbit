//! Real stores and the actual private post-provider-verification seam; no remote OAuth mock.
mod fixture;
use super::*;
use crate::external_auth::FLOW_TTL_SECONDS;
use fixture::*;
use rbit_storage::{Work, count_work};

#[tokio::test]
async fn manual_context_is_single_use_owned_and_preserves_the_existing_four_statement_write_budget() {
    let f = Fixture::new().await;
    let callback = f.verified("new").await;
    let flow = f.last_flow();
    let debug = format!("{callback:?}");
    assert!(!debug.contains(&flow.state));
    let initiating_client = client();
    let abandoned = f.services.complete_original_link(callback, &f.current, &initiating_client);
    drop(abandoned);
    // An unpolled caller cannot claim context before owned dispatch.
    let callback = f.verified_from_flow(&flow, "new");
    let (result, work) = count_work(f.services.complete_original_link(callback, &f.current, &client())).await;
    result.unwrap();
    assert_eq!(work, Work { acquisitions: 4, statements: 4, cache_writes: 0 });
    assert!(f.database.external_identities().find("github", "new").await.unwrap().is_some());
    assert!(
        f.services.external_auth.inner.original_links.verified(&flow.state, fixture::callback("new", f.current.user().user.id())).is_err()
    );
}

#[tokio::test]
async fn reset_and_fresh_login_cannot_rehabilitate_the_session_origin_captured_before_provider_redirect() {
    let f = Fixture::new().await;
    let old_callback = f.verified("new").await;
    let flow = f.last_flow();
    let intent = f
        .database
        .profiles()
        .set_password_hash_with_intent(f.current.user().user.id(), "reset", f.current.snapshot().epoch(), Timestamp(20))
        .await
        .unwrap()
        .unwrap();
    f.services.accounts.recover_original_session_intent(intent).await.unwrap();
    assert!(f.services.complete_original_link(old_callback, &f.current, &client()).await.is_err());
    let fresh = f.fresh_session().await;
    let old_callback = f.verified_from_flow(&flow, "new");
    assert!(matches!(f.services.complete_original_link(old_callback, &fresh, &client()).await, Err(ServiceError::Forbidden)));
    assert!(f.database.external_identities().find("github", "new").await.unwrap().is_none());
    // A genuinely new flow from the new session succeeds; no latest-epoch restamping.
    let callback = f.verified_for("new", &fresh).await;
    f.services.complete_original_link(callback, &fresh, &client()).await.unwrap();
}

#[tokio::test]
async fn legacy_bare_id_callback_and_unretained_or_tampered_flow_cannot_mint_link_authority() {
    let f = Fixture::new().await;
    let (result, work) =
        count_work(f.services.complete_external_sign_in_original(fixture::callback("new", f.current.user().user.id()), &client())).await;
    assert!(matches!(result, Err(ServiceError::Forbidden)));
    assert_eq!(work, Work::default());
    let (_, legacy) = f.services.external_auth.begin("github", "/settings/security", Some(f.current.user().user.id())).await.unwrap();
    let key = f.services.external_auth.key().await.unwrap();
    let flow = key.open(&legacy, Timestamp::now().seconds(), FLOW_TTL_SECONDS).unwrap();
    assert!(
        f.services
            .external_auth
            .original_link_callback(
                "github",
                Some(&legacy),
                CallbackParams { state: Some(&flow.state), code: Some("never-exchanged"), ..CallbackParams::default() },
                &client()
            )
            .await
            .is_err()
    );
    let _ = f.verified("new").await;
    let flow = f.last_flow();
    let malformed = format!("{}x", key.seal(&flow));
    assert!(
        f.services
            .external_auth
            .original_link_callback(
                "github",
                Some(&malformed),
                CallbackParams { state: Some(&flow.state), code: Some("never-exchanged"), ..CallbackParams::default() },
                &client()
            )
            .await
            .is_err()
    );
    assert!(f.database.external_identities().find("github", "new").await.unwrap().is_none());
}

#[tokio::test]
async fn fixed_epoch_insert_refuses_reset_pending_suspension_and_account_recreation_without_link_effects() {
    use rbit_storage::sqlite::external_identities::GuardedIdentityLink;
    let f = Fixture::new().await;
    let store = f.database.external_identities();
    let account = f.current.user().user.id();
    let original = f.current.snapshot().epoch(); // captured BEFORE concurrent writer wins
    let intent = f.database.profiles().set_password_hash_with_intent(account, "reset", original, Timestamp(20)).await.unwrap().unwrap();
    let identity = NewExternalIdentity { provider_id: "github", subject: "guarded", email: None, username: None };
    let (stale, work) = count_work(store.link_at_epoch(account, identity, Timestamp(21), original)).await;
    assert!(matches!(stale.unwrap(), GuardedIdentityLink::Unavailable));
    assert_eq!(work, Work { acquisitions: 1, statements: 1, cache_writes: 0 });
    let target = rbit_identity_models::credential::CredentialEpoch::new(intent.target_epoch).unwrap();
    assert!(matches!(store.link_at_epoch(account, identity, Timestamp(21), target).await.unwrap(), GuardedIdentityLink::Unavailable));
    f.services.accounts.recover_original_session_intent(intent).await.unwrap();
    assert!(matches!(store.link_at_epoch(account, identity, Timestamp(22), target).await.unwrap(), GuardedIdentityLink::Linked(_)));
}

#[tokio::test]
async fn closed_dispatch_admission_does_not_consume_the_original_flow_context() {
    let f = Fixture::new().await;
    let callback = f.verified("new").await;
    let flow = f.last_flow();
    f.services.accounts.close_session_effects();
    let (result, work) = count_work(f.services.complete_original_link(callback, &f.current, &client())).await;
    assert!(matches!(result, Err(ServiceError::Unavailable { .. })));
    assert_eq!(work, Work::default());
    assert!(f.services.external_auth.inner.original_links.validate_callback(&flow.state, "github").is_ok());
    assert!(f.database.external_identities().find("github", "new").await.unwrap().is_none());
    f.services.accounts.shutdown_session_effects().await;
}

#[tokio::test]
async fn already_linked_completion_still_revalidates_fixed_epoch_with_an_explicit_extra_security_query() {
    let f = Fixture::new().await;
    let identity = NewExternalIdentity { provider_id: "github", subject: "source", email: None, username: None };
    f.database.external_identities().link(f.current.user().user.id(), identity, Timestamp(2)).await.unwrap().unwrap();
    let context = f.verified("source").await;
    let (result, work) = count_work(f.services.complete_original_link(context, &f.current, &client())).await;
    result.unwrap();
    assert_eq!(work, Work { acquisitions: 2, statements: 2, cache_writes: 0 });
}

#[tokio::test]
async fn auto_email_adapter_uses_the_first_origin_in_the_atomic_addition_not_a_post_reset_epoch() {
    use crate::external_auth::original::{Mode, Original};
    use rbit_identity::accounts::ProviderAuthentication;
    let f = Fixture::new().await;
    let account = f.current.user().user.id();
    let email = &f.current.user().user.email;
    f.database.recovery().mark_verified(account, email, Timestamp::now()).await.unwrap();
    let store = f.database.external_identities();
    let original = ProviderAuthentication::capture_verified_email(&store, email.as_str()).await.unwrap().unwrap();
    let intent = f
        .database
        .profiles()
        .set_password_hash_with_intent(account, "reset", original.original_epoch(), Timestamp(20))
        .await
        .unwrap()
        .unwrap();
    let identity = NewExternalIdentity { provider_id: "github", subject: "new-auto", email: None, username: None };
    let (result, work) = count_work(Original::link_accepted(&store, account, identity, Timestamp(21), &original)).await;
    assert!(matches!(result, Err(ServiceError::Unavailable { .. })));
    assert_eq!(work, Work { acquisitions: 1, statements: 1, cache_writes: 0 });
    let pending = ProviderAuthentication::capture_verified_email(&store, email.as_str()).await.unwrap().unwrap();
    f.services.accounts.recover_original_session_intent(intent).await.unwrap();
    let (result, work) = count_work(Original::link_accepted(&store, account, identity, Timestamp(22), &pending)).await;
    assert!(matches!(result, Err(ServiceError::Unavailable { .. })));
    assert_eq!(work, Work::default(), "captured pending cannot be rehabilitated or write");
    assert!(store.find("github", "new-auto").await.unwrap().is_none());
    let fresh = ProviderAuthentication::capture_verified_email(&store, email.as_str()).await.unwrap().unwrap();
    assert!(Original::link_accepted(&store, account, identity, Timestamp(23), &fresh).await.unwrap().is_ok());
}
