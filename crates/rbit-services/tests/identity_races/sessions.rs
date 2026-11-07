//! Session renewal parked across each revocation boundary: the identity application runs
//! over the real redb store and account projection, a request is parked after its read or
//! immediately before its renewal write, and the production service revokes meanwhile.

use std::future::Future;

use rbit_auth::SessionToken;
use rbit_core::Timestamp;
use rbit_identity::sessions::{SessionApplication, SessionPolicy};

use super::{
    Fixture, NEW_PASSWORD, PASSWORD, client,
    gate::{AHEAD_SECONDS, Pause, PausingSessions, TenMinutesLater},
};

const PAUSES: [Pause; 2] = [Pause::AfterRead, Pause::BeforeRenewal];

fn policy() -> SessionPolicy {
    SessionPolicy { lifetime_days: 30, max_age_days: 0, notifications: false }
}

/// Resumes `token` with the request parked at `at` until `revoke` has returned. Returns
/// whether the parked request resolved a user and how many renewal writes it issued.
async fn resume_across(fixture: &Fixture, token: &SessionToken, at: Pause, revoke: impl Future<Output = ()>) -> (bool, usize) {
    let sessions = PausingSessions::new(fixture.kv.sessions());
    let accounts = fixture.database.accounts();
    let application = SessionApplication::with_clock(&sessions, &accounts, policy(), TenMinutesLater);
    let parked = sessions.arm(at);
    let (resumed, ()) = tokio::join!(application.resume(token), async move {
        let release = parked.reached().await;
        revoke.await;
        release.send(()).unwrap();
    });
    (resumed.unwrap().is_some(), sessions.renewals())
}

async fn assert_revoked(fixture: &Fixture, token: &SessionToken) {
    assert!(fixture.kv.sessions().get(token.digest()).await.unwrap().is_none(), "the parked renewal resurrected the session");
    assert!(!fixture.is_live(token).await, "a request after revocation still resolved the session");
}

#[tokio::test]
async fn an_uncontested_parked_resume_writes_its_renewal() {
    for at in PAUSES {
        let fixture = Fixture::new().await;
        let alice = fixture.user("alice").await;
        let token = fixture.session(&alice).await;
        let before = Timestamp::now().0;
        assert_eq!(resume_across(&fixture, &token, at, async {}).await, (true, 1), "{at:?}");
        let renewed = fixture.kv.sessions().get(token.digest()).await.unwrap().unwrap();
        assert!(renewed.last_seen_at.0 >= before + AHEAD_SECONDS, "{at:?}: the renewal was applied");
    }
}

#[tokio::test]
async fn a_renewal_parked_across_single_revocation_cannot_resurrect_it() {
    for at in PAUSES {
        let fixture = Fixture::new().await;
        let alice = fixture.user("alice").await;
        let token = fixture.session(&alice).await;
        let other = fixture.session(&alice).await;
        let listed = fixture.services.accounts.sessions(&alice, Some(&token)).await.unwrap();
        let id = listed.into_iter().find(|session| session.is_current).unwrap().id;
        let revoke = async { fixture.services.accounts.revoke_session(&alice, &id, &client()).await.unwrap() };
        assert_eq!(resume_across(&fixture, &token, at, revoke).await, (false, 1), "{at:?}");
        assert_revoked(&fixture, &token).await;
        assert!(fixture.is_live(&other).await, "{at:?}: only the chosen session was revoked");
    }
}

#[tokio::test]
async fn a_renewal_parked_across_a_password_change_cannot_resurrect_any_session() {
    for at in PAUSES {
        let fixture = Fixture::new().await;
        let alice = fixture.user("alice").await;
        let parked = fixture.session(&alice).await;
        let other = fixture.session(&alice).await;
        let change = async {
            fixture.services.profiles.change_password(&alice, PASSWORD.to_owned(), NEW_PASSWORD.to_owned()).await.unwrap();
        };
        assert_eq!(resume_across(&fixture, &parked, at, change).await, (false, 1), "{at:?}");
        assert_revoked(&fixture, &parked).await;
        assert_revoked(&fixture, &other).await;
        assert!(fixture.services.accounts.sessions(&alice, None).await.unwrap().is_empty(), "{at:?}: no index entry survives");
    }
}

#[tokio::test]
async fn a_renewal_parked_across_signing_out_elsewhere_cannot_resurrect_it() {
    for at in PAUSES {
        let fixture = Fixture::new().await;
        let alice = fixture.user("alice").await;
        let current = fixture.session(&alice).await;
        let parked = fixture.session(&alice).await;
        let revoke = async {
            assert_eq!(fixture.services.accounts.revoke_other_sessions(&alice, &current, &client()).await.unwrap(), 1);
        };
        assert_eq!(resume_across(&fixture, &parked, at, revoke).await, (false, 1), "{at:?}");
        assert_revoked(&fixture, &parked).await;
        assert!(fixture.is_live(&current).await, "{at:?}: the current session is kept");
    }
}

/// Suspension leaves the row in place, so a request parked after its account lookup may
/// finish; the first request after the suspension returns is refused either way.
#[tokio::test]
async fn suspension_during_a_parked_resume_refuses_every_later_request() {
    for at in PAUSES {
        let fixture = Fixture::new().await;
        let admin = fixture.user("alice").await;
        let bob = fixture.user("bob").await;
        let token = fixture.session(&bob).await;
        let suspend = async {
            fixture.services.admin.set_suspended(&admin, bob.id(), true).await.unwrap();
        };
        let (resolved, _) = resume_across(&fixture, &token, at, suspend).await;
        if at == Pause::AfterRead {
            assert!(!resolved, "the account lookup after the suspension must refuse the session");
            assert!(fixture.kv.sessions().get(token.digest()).await.unwrap().is_none(), "the refused session is deleted");
        }
        assert!(!fixture.is_live(&token).await, "{at:?}: a suspended account's session still resolved");
    }
}
