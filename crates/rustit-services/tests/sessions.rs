//! Session listing, revocation and the password-change rule against real, temporary
//! SQLite and redb stores.

use std::{sync::Arc, time::Duration};

use rustit_auth::SessionToken;
use rustit_core::{Config, User};
use rustit_services::{
    ServiceError, Services, SignUp,
    accounts::ClientInfo,
    admin::ListQuery,
    audit::{AuditAction, AuditCategory},
};
use rustit_storage::{Database, KvStore};

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

    assert!(fixture.eventually_dead(&first).await);
    assert!(fixture.eventually_dead(&second).await);
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
    // Let the subscriber run first, as it may in production.
    tokio::time::sleep(Duration::from_millis(50)).await;
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
