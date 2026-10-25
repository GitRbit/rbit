//! Session listing, revocation and the password-change rule against real, temporary
//! `SQLite` and redb stores, and the maximum session age.

use std::{sync::Arc, time::Duration};

use rbit_auth::SessionToken;
use rbit_core::{Config, Timestamp, User};
use rbit_services::{
    ServiceError, Services, SignUp,
    accounts::ClientInfo,
    admin::ListQuery,
    audit::{AuditAction, AuditCategory},
};
use rbit_storage::{Database, KvStore, SessionRecord};

const PASSWORD: &str = "correct horse battery";
const NEW_PASSWORD: &str = "a whole new password";

struct Fixture {
    _dir: tempfile::TempDir,
    services: Services,
}

impl Fixture {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.storage.data_dir = dir.path().to_path_buf();
        let config = Arc::new(config);
        let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
        let kv = KvStore::open(&config.storage.kv_path()).unwrap();
        let services = Services::new(config, &database, &kv).await.unwrap();
        Self { _dir: dir, services }
    }

    async fn user(&self, name: &str) -> User {
        let input = SignUp { username: name.to_owned(), email: format!("{name}@example.com"), password: PASSWORD.to_owned() };
        self.services.accounts.create_user(input, None).await.unwrap()
    }

    async fn session(&self, user: &User, agent: &str) -> SessionToken {
        let client = ClientInfo { ip: Some("192.0.2.10".parse().unwrap()), user_agent: agent.to_owned() };
        self.services.accounts.start_session_for(user, &client).await.unwrap()
    }

    async fn is_live(&self, token: &SessionToken) -> bool {
        self.services.accounts.resume_session(token).await.unwrap().is_some()
    }

    /// Waits for the background subscriber to act (it runs on its own task).
    async fn eventually_dead(&self, token: &SessionToken) -> bool {
        for _ in 0..100 {
            if !self.is_live(token).await {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        false
    }
}

fn client() -> ClientInfo {
    ClientInfo { ip: Some("198.51.100.1".parse().unwrap()), user_agent: "test".to_owned() }
}

#[tokio::test]
async fn lists_and_revokes_sessions() {
    let fixture = Fixture::new().await;
    let accounts = &fixture.services.accounts;
    let alice = fixture.user("alice").await;
    let bob = fixture.user("bob").await;
    let laptop = fixture.session(&alice, "Firefox").await;
    let phone = fixture.session(&alice, "Safari").await;
    let tablet = fixture.session(&alice, "Chrome").await;
    let bobs = fixture.session(&bob, "Edge").await;

    let sessions = accounts.sessions(&alice, Some(&laptop)).await.unwrap();
    assert_eq!(sessions.len(), 3);
    assert_eq!(sessions.iter().filter(|s| s.is_current).count(), 1);
    let current = sessions.iter().find(|s| s.is_current).unwrap();
    assert_eq!(current.user_agent, "Firefox");
    assert_eq!(current.ip_address.as_deref(), Some("192.0.2.10"));

    // Revoke one by id; ids of other users' sessions are not found.
    let phone_id = sessions.iter().find(|s| s.user_agent == "Safari").unwrap().id.clone();
    let bob_id = accounts.sessions(&bob, None).await.unwrap()[0].id.clone();
    assert!(matches!(accounts.revoke_session(&alice, &bob_id, &client()).await, Err(ServiceError::NotFound)));
    accounts.revoke_session(&alice, &phone_id, &client()).await.unwrap();
    assert!(!fixture.is_live(&phone).await);
    assert!(fixture.is_live(&bobs).await);

    // Sign out everywhere else.
    assert_eq!(accounts.revoke_other_sessions(&alice, &laptop, &client()).await.unwrap(), 1);
    assert!(fixture.is_live(&laptop).await);
    assert!(!fixture.is_live(&tablet).await);
    assert!(fixture.is_live(&bobs).await);

    let admin = alice.clone(); // the first user is an administrator
    assert!(admin.is_admin);
    let log = fixture.services.audit.list(&admin, Some(AuditCategory::Sessions), &ListQuery::default()).await.unwrap();
    let actions: Vec<_> = log.items.iter().map(|event| event.action).collect();
    assert_eq!(actions, [AuditAction::OtherSessionsRevoked, AuditAction::SessionRevoked]);
}

#[tokio::test]
async fn a_password_change_elsewhere_ends_every_session() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice").await;
    let bob = fixture.user("bob").await;
    let first = fixture.session(&alice, "one").await;
    let second = fixture.session(&alice, "two").await;
    let bobs = fixture.session(&bob, "Edge").await;

    // As a password reset or an administrator would: no session of Alice's is involved.
    fixture.services.profiles.change_password(&alice, PASSWORD.to_owned(), NEW_PASSWORD.to_owned()).await.unwrap();

    assert!(!fixture.is_live(&first).await, "revocation completes before the password change returns");
    assert!(!fixture.is_live(&second).await);
    assert!(fixture.is_live(&bobs).await);
}

#[tokio::test]
async fn changing_your_own_password_keeps_the_current_session() {
    let fixture = Fixture::new().await;
    let accounts = &fixture.services.accounts;
    let alice = fixture.user("alice").await;
    let current = fixture.session(&alice, "current").await;
    let other = fixture.session(&alice, "other").await;

    let guard = accounts.guard_password_change(alice.id(), &current);
    fixture.services.profiles.change_password(&alice, PASSWORD.to_owned(), NEW_PASSWORD.to_owned()).await.unwrap();
    guard.finish(true).await;

    assert!(!fixture.is_live(&other).await);
    assert!(fixture.is_live(&current).await);

    // A failed change (wrong current password) publishes nothing and ends nothing.
    let another = fixture.session(&alice, "another").await;
    let guard = accounts.guard_password_change(alice.id(), &current);
    assert!(fixture.services.profiles.change_password(&alice, "wrong".to_owned(), NEW_PASSWORD.to_owned()).await.is_err());
    guard.finish(false).await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(fixture.is_live(&another).await);
    assert!(fixture.is_live(&current).await);
}

#[tokio::test]
async fn an_abandoned_request_does_not_shield_its_session() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice").await;
    let current = fixture.session(&alice, "current").await;

    let guard = fixture.services.accounts.guard_password_change(alice.id(), &current);
    fixture.services.profiles.change_password(&alice, PASSWORD.to_owned(), NEW_PASSWORD.to_owned()).await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    drop(guard);

    assert!(fixture.eventually_dead(&current).await);
}

/// Services whose sessions end `max_age_days` after sign-in, with their store and a user.
async fn with_max_age(dir: &tempfile::TempDir, max_age_days: u32) -> (Services, KvStore, User) {
    let mut config = Config::default();
    config.storage.data_dir = dir.path().to_path_buf();
    config.security.session_max_age_days = max_age_days;
    let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
    let kv = KvStore::open(&config.storage.kv_path()).unwrap();
    let services = Services::new(Arc::new(config), &database, &kv).await.unwrap();
    let input = SignUp { username: "alice".to_owned(), email: "alice@example.com".to_owned(), password: PASSWORD.to_owned() };
    let alice = services.accounts.create_user(input, None).await.unwrap();
    (services, kv, alice)
}

#[tokio::test]
async fn sessions_end_at_their_maximum_age_however_active() {
    let dir = tempfile::tempdir().unwrap();
    let (services, kv, alice) = with_max_age(&dir, 90).await;
    let day = 24 * 60 * 60;
    let now = Timestamp::now().seconds();
    // Created `age` days ago and used just now, so the idle expiry is still ahead.
    let session = |age: i64| {
        let mut record = SessionRecord::new(alice.id(), Timestamp(now + day), None, "test");
        record.created_at = Timestamp(now - age * day);
        record
    };
    let old = SessionToken::generate();
    kv.sessions().put(old.digest(), session(91)).await.unwrap();
    let recent = SessionToken::generate();
    kv.sessions().put(recent.digest(), session(89)).await.unwrap();

    assert!(services.accounts.resume_session(&old).await.unwrap().is_none());
    assert!(kv.sessions().get(old.digest()).await.unwrap().is_none(), "discarded");
    assert!(services.accounts.resume_session(&recent).await.unwrap().is_some());

    // Renewals never reach past the maximum age.
    let mut stale = session(80);
    stale.expires_at = Timestamp(now + 60);
    stale.last_seen_at = Timestamp(now - day);
    let renewed = SessionToken::generate();
    kv.sessions().put(renewed.digest(), stale).await.unwrap();
    assert!(services.accounts.resume_session(&renewed).await.unwrap().is_some());
    let record = kv.sessions().get(renewed.digest()).await.unwrap().unwrap();
    assert_eq!(record.expires_at.seconds(), now + 10 * day, "capped at 90 days after creation");
    assert!(record.last_seen_at.seconds() >= now, "the visit was recorded");
}

#[tokio::test]
async fn new_sessions_expire_by_the_maximum_age_or_never_without_one() {
    for max_age_days in [0, 7] {
        let dir = tempfile::tempdir().unwrap();
        let (services, kv, alice) = with_max_age(&dir, max_age_days).await;

        let token = services.accounts.start_session(&alice).await.unwrap();
        let record = kv.sessions().get(token.digest()).await.unwrap().unwrap();
        let lifetime = record.expires_at.seconds() - record.created_at.seconds();
        let expected = if max_age_days == 0 { services.accounts.session_lifetime_seconds() } else { 7 * 24 * 60 * 60 };
        assert!((lifetime - expected).abs() <= 2, "{max_age_days}: {lifetime} vs {expected}");

        let mut ancient = SessionRecord::new(alice.id(), Timestamp(Timestamp::now().seconds() + 60), None, "test");
        ancient.created_at = Timestamp(0);
        let token = SessionToken::generate();
        kv.sessions().put(token.digest(), ancient).await.unwrap();
        assert_eq!(services.accounts.resume_session(&token).await.unwrap().is_some(), max_age_days == 0);
    }
}
