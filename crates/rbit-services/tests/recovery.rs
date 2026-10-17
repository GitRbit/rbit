//! Password reset and email verification against real, temporary SQLite and redb stores,
//! with an in-memory mailer capturing every message.

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use rbit_auth::{Passwords, SessionToken};
use rbit_core::{
    AccountId, Config, Timestamp, User,
    config::{MailConfig, MailTransport},
};
use rbit_git::RepositoryStorage;
use rbit_mail::{Mail, MemoryMailer};
use rbit_services::{
    DomainEvent, MailService, RecoveryService, ServiceContext, ServiceError, Services, SignUp, ThrottleScope,
    recovery::{EMAIL_UNAVAILABLE, RequestThrottle},
};
use rbit_storage::{
    Database, KvStore, NewUser,
    sqlite::recovery::{NewEmailToken, TokenPurpose},
};

const PASSWORD: &str = "correct horse battery";
const NEW_PASSWORD: &str = "a brand new passphrase";
const BASE_URL: &str = "https://git.example.com";
const WAIT: Duration = Duration::from_secs(10);

/// Records every throttle check and refuses the keys it is told to.
#[derive(Debug, Clone, Default)]
struct RecordingThrottle {
    checks: Arc<Mutex<Vec<(ThrottleScope, String)>>>,
    refused: Arc<Mutex<Vec<String>>>,
}

impl RecordingThrottle {
    fn checks(&self) -> Vec<(ThrottleScope, String)> {
        self.checks.lock().unwrap().clone()
    }

    fn refuse(&self, key: &str) {
        self.refused.lock().unwrap().push(key.to_owned());
    }
}

impl RequestThrottle for RecordingThrottle {
    fn check(&self, scope: ThrottleScope, key: &str) -> Result<(), ServiceError> {
        self.checks.lock().unwrap().push((scope, key.to_owned()));
        if self.refused.lock().unwrap().iter().any(|refused| refused == key) {
            Err(ServiceError::RateLimited { retry_after_seconds: 60 })
        } else {
            Ok(())
        }
    }
}

struct Fixture {
    _dir: tempfile::TempDir,
    database: Database,
    services: Services,
    recovery: RecoveryService,
    outbox: MemoryMailer,
    throttle: RecordingThrottle,
}

impl Fixture {
    async fn new() -> Self {
        Self::with_transport(MailTransport::Log).await
    }

    async fn with_transport(transport: MailTransport) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.storage.data_dir = dir.path().to_path_buf();
        config.server.public_url = format!("{BASE_URL}/");
        "Forge".clone_into(&mut config.server.site_name);
        config.mail = MailConfig { transport, from_address: "git@example.com".to_owned(), ..MailConfig::default() };
        let config = Arc::new(config);

        let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
        let kv = KvStore::open(&config.storage.kv_path()).unwrap();
        let services = Services::new(Arc::clone(&config), &database, &kv).await.unwrap();

        let ctx = ServiceContext {
            config: Arc::clone(&config),
            database: database.clone(),
            kv,
            repositories: RepositoryStorage::new(config.storage.repositories_dir()),
            events: services.events.clone(),
        };
        let outbox = MemoryMailer::new();
        let throttle = RecordingThrottle::default();
        let mut builder = RecoveryService::builder(ctx).throttle(throttle.clone()).instance(services.instance.clone());
        if transport != MailTransport::Disabled {
            builder = builder.mail(MailService::with_mailer(Arc::new(outbox.clone()), Duration::from_secs(5)));
        }
        Self { _dir: dir, database, recovery: builder.build(), services, outbox, throttle }
    }

    /// A user with a real password, created directly (publishes no event).
    async fn user(&self, name: &str, is_admin: bool) -> User {
        self.database
            .accounts()
            .create_user(NewUser {
                name: name.parse().unwrap(),
                email: format!("{name}@example.com").parse().unwrap(),
                display_name: None,
                password_hash: Passwords::hash(PASSWORD.to_owned()).await.unwrap(),
                is_admin,
            })
            .await
            .unwrap()
    }

    /// Waits for the `count`-th message overall and returns it.
    async fn mail(&self, count: usize) -> Mail {
        let sent = tokio::time::timeout(WAIT, self.outbox.wait_for(count)).await.expect("message was not sent");
        sent[count - 1].clone()
    }

    /// Gives background sends a moment, then returns how many messages went out.
    async fn settled_count(&self) -> usize {
        tokio::time::sleep(Duration::from_millis(150)).await;
        self.outbox.sent().len()
    }

    async fn is_verified(&self, user: &User) -> bool {
        self.recovery.email_status(user).await.unwrap().is_verified()
    }
}

/// The secret at the end of the first link in `mail` under `prefix`.
fn token_in(mail: &Mail, prefix: &str) -> String {
    let link = mail.links().into_iter().find(|link| link.starts_with(&format!("{BASE_URL}{prefix}"))).expect("link in message");
    assert!(mail.html.contains(link), "the HTML part carries the same link");
    link.rsplit('/').next().unwrap().to_owned()
}

#[tokio::test]
async fn reset_requests_do_not_reveal_whether_an_account_exists() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice", false).await;

    let unknown = fixture.recovery.request_password_reset("nobody@example.com", "203.0.113.9").await;
    let known = fixture.recovery.request_password_reset(" ALICE@example.com ", "203.0.113.9").await;
    assert!(unknown.is_ok() && known.is_ok(), "same outcome either way");

    let mail = fixture.mail(1).await;
    assert_eq!(mail.to, alice.email);
    assert_eq!(mail.subject, "Reset your Forge password");
    assert!(mail.text.contains("Hi alice,"));
    assert_eq!(fixture.settled_count().await, 1, "nothing is sent to unknown addresses");

    let invalid = fixture.recovery.request_password_reset("not an address", "203.0.113.9").await;
    assert!(matches!(invalid, Err(ServiceError::Invalid(_))));
}

#[tokio::test]
async fn reset_link_sets_a_new_password_once() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice", false).await;
    let mut events = fixture.services.events.subscribe();

    fixture.recovery.request_password_reset("alice@example.com", "198.51.100.1").await.unwrap();
    let token = token_in(&fixture.mail(1).await, "/password/reset/");
    assert_eq!(fixture.recovery.reset_target(&token).await.unwrap(), "alice");

    // An invalid password is rejected like at sign-up, and the link keeps working.
    let short = fixture.recovery.reset_password(&token, "short".to_owned()).await;
    assert!(matches!(short, Err(ServiceError::Invalid(message)) if message.contains("at least 10")));
    assert!(fixture.recovery.reset_target(&token).await.is_ok());

    fixture.recovery.reset_password(&token, NEW_PASSWORD.to_owned()).await.unwrap();
    assert_eq!(events.recv().await.unwrap(), DomainEvent::PasswordChanged { user: alice.id() });
    assert!(fixture.services.accounts.authenticate("alice", NEW_PASSWORD.to_owned()).await.is_ok());
    assert!(fixture.services.accounts.authenticate("alice", PASSWORD.to_owned()).await.is_err());
    assert!(fixture.is_verified(&alice).await, "the link proved control of the mailbox");

    // Single use.
    assert!(matches!(fixture.recovery.reset_target(&token).await, Err(ServiceError::NotFound)));
    let again = fixture.recovery.reset_password(&token, "yet another passphrase".to_owned()).await;
    assert!(matches!(again, Err(ServiceError::NotFound)));
    assert!(matches!(fixture.recovery.reset_target("garbage").await, Err(ServiceError::NotFound)));
}

#[tokio::test]
async fn a_completed_reset_invalidates_older_links() {
    let fixture = Fixture::new().await;
    fixture.user("alice", false).await;

    fixture.recovery.request_password_reset("alice@example.com", "198.51.100.1").await.unwrap();
    fixture.recovery.request_password_reset("alice@example.com", "198.51.100.1").await.unwrap();
    let first = token_in(&fixture.mail(1).await, "/password/reset/");
    let second = token_in(&fixture.mail(2).await, "/password/reset/");
    assert_ne!(first, second);

    fixture.recovery.reset_password(&second, NEW_PASSWORD.to_owned()).await.unwrap();
    assert!(matches!(fixture.recovery.reset_target(&first).await, Err(ServiceError::NotFound)));
}

#[tokio::test]
async fn expired_links_are_refused() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice", false).await;

    let mut tokens = Vec::new();
    for purpose in [TokenPurpose::Reset, TokenPurpose::Verify] {
        let secret = SessionToken::generate();
        let token = NewEmailToken {
            digest: secret.digest(),
            account_id: alice.id(),
            purpose,
            email: alice.email.clone(),
            expires_at: Timestamp(Timestamp::now().seconds() - 1),
        };
        fixture.database.recovery().insert(&token).await.unwrap();
        tokens.push(secret.encode());
    }

    assert!(matches!(fixture.recovery.reset_target(&tokens[0]).await, Err(ServiceError::NotFound)));
    assert!(matches!(fixture.recovery.reset_password(&tokens[0], NEW_PASSWORD.to_owned()).await, Err(ServiceError::NotFound)));
    assert!(matches!(fixture.recovery.verify_email(&tokens[1]).await, Err(ServiceError::NotFound)));
    assert!(!fixture.is_verified(&alice).await);
}

#[tokio::test]
async fn suspended_users_cannot_reset() {
    let fixture = Fixture::new().await;
    let admin = fixture.user("root", true).await;
    let alice = fixture.user("alice", false).await;

    // A link issued before the suspension stops working.
    fixture.recovery.request_password_reset("alice@example.com", "198.51.100.1").await.unwrap();
    let token = token_in(&fixture.mail(1).await, "/password/reset/");
    fixture.services.admin.set_suspended(&admin, alice.id(), true).await.unwrap();
    assert!(matches!(fixture.recovery.reset_target(&token).await, Err(ServiceError::Forbidden)));
    assert!(matches!(fixture.recovery.reset_password(&token, NEW_PASSWORD.to_owned()).await, Err(ServiceError::Forbidden)));

    // New requests look the same as for anyone else, but send nothing.
    fixture.recovery.request_password_reset("alice@example.com", "198.51.100.1").await.unwrap();
    assert_eq!(fixture.settled_count().await, 1);

    let by_admin = fixture.recovery.admin_send_password_reset(&admin, alice.id()).await;
    assert!(matches!(by_admin, Err(ServiceError::Invalid(message)) if message.contains("suspended")));
}

#[tokio::test]
async fn reset_requests_are_throttled_by_client_and_recipient() {
    let fixture = Fixture::new().await;
    fixture.user("alice", false).await;

    fixture.recovery.request_password_reset("Alice@Example.com", "192.0.2.7").await.unwrap();
    assert_eq!(
        fixture.throttle.checks(),
        [(ThrottleScope::EmailRequest, "192.0.2.7".to_owned()), (ThrottleScope::EmailRequest, "alice@example.com".to_owned())]
    );

    fixture.throttle.refuse("192.0.2.7");
    let limited = fixture.recovery.request_password_reset("alice@example.com", "192.0.2.7").await;
    assert!(matches!(limited, Err(ServiceError::RateLimited { .. })));

    fixture.throttle.refuse("alice@example.com");
    let limited = fixture.recovery.request_password_reset("alice@example.com", "192.0.2.8").await;
    assert!(matches!(limited, Err(ServiceError::RateLimited { .. })));
    assert_eq!(fixture.settled_count().await, 1, "throttled requests send nothing");
}

#[tokio::test]
async fn sign_up_sends_a_verification_link() {
    let fixture = Fixture::new().await;
    fixture.recovery.spawn_background();

    let input = SignUp { username: "bob".to_owned(), email: "bob@example.com".to_owned(), password: PASSWORD.to_owned() };
    let bob = fixture.services.accounts.sign_up(input).await.unwrap();
    assert!(!fixture.is_verified(&bob).await);

    let mail = fixture.mail(1).await;
    assert_eq!(mail.to, bob.email);
    assert!(mail.text.contains("Welcome to Forge!"));
    let token = token_in(&mail, "/email/verify/");

    let verified = fixture.recovery.verify_email(&token).await.unwrap();
    assert_eq!(verified.account_id, bob.id());
    assert_eq!(verified.address, bob.email);
    assert!(fixture.is_verified(&bob).await);
    assert!(matches!(fixture.recovery.verify_email(&token).await, Err(ServiceError::NotFound)), "single use");
}

#[tokio::test]
async fn changing_the_address_verifies_the_new_one_and_notifies_the_old_one() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice", false).await;

    // Verified first, through a requested link that is then outdated by the change.
    fixture.recovery.resend_verification(&alice).await.unwrap();
    let old_link = token_in(&fixture.mail(1).await, "/email/verify/");
    fixture.recovery.resend_verification(&alice).await.unwrap();
    let stale_link = token_in(&fixture.mail(2).await, "/email/verify/");
    fixture.recovery.verify_email(&old_link).await.unwrap();
    assert!(fixture.is_verified(&alice).await);
    assert!(matches!(fixture.recovery.verify_email(&stale_link).await, Err(ServiceError::NotFound)), "siblings are used up");

    fixture.recovery.spawn_background();
    fixture.services.profiles.change_email(&alice, PASSWORD.to_owned(), "alice@new.example").await.unwrap();
    let alice = fixture.database.accounts().find_user(alice.id()).await.unwrap().unwrap();
    assert!(!fixture.is_verified(&alice).await, "a new address starts unverified");

    let first = fixture.mail(3).await;
    let second = fixture.mail(4).await;
    let (verification, notice) = if first.to == alice.email { (first, second) } else { (second, first) };
    assert_eq!(verification.to.as_str(), "alice@new.example");
    assert!(verification.text.contains("new email address"));
    assert_eq!(notice.to.as_str(), "alice@example.com");
    assert_eq!(notice.subject, "Your Forge email address was changed");
    assert!(notice.text.contains("a••••@new.example"), "the new address is masked");
    assert!(notice.links().iter().all(|link| !link.contains("/verify/")), "no link goes to the old address");

    fixture.recovery.verify_email(&token_in(&verification, "/email/verify/")).await.unwrap();
    assert!(fixture.is_verified(&alice).await);
}

#[tokio::test]
async fn resending_requires_an_unverified_address_and_is_throttled() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice", false).await;

    fixture.throttle.refuse("alice@example.com");
    assert!(matches!(fixture.recovery.resend_verification(&alice).await, Err(ServiceError::RateLimited { .. })));
    assert_eq!(fixture.throttle.checks(), [(ThrottleScope::EmailRequest, "alice@example.com".to_owned())]);
    assert_eq!(fixture.settled_count().await, 0);

    fixture.database.recovery().mark_verified(alice.id(), &alice.email, Timestamp::now()).await.unwrap();
    let verified = fixture.recovery.resend_verification(&alice).await;
    assert!(matches!(verified, Err(ServiceError::Invalid(message)) if message.contains("already verified")));
}

#[tokio::test]
async fn administrators_send_resets_and_set_passwords() {
    let fixture = Fixture::new().await;
    let admin = fixture.user("root", true).await;
    let alice = fixture.user("alice", false).await;
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
    assert_eq!(events.recv().await.unwrap(), DomainEvent::PasswordChanged { user: alice.id() });
    assert!(fixture.services.accounts.authenticate("alice", NEW_PASSWORD.to_owned()).await.is_ok());
    assert!(matches!(fixture.recovery.reset_target(&token).await, Err(ServiceError::NotFound)));

    let verified = fixture.recovery.verified_among(&admin, &[admin.id(), alice.id()]).await.unwrap();
    assert!(verified.is_empty());
    assert!(matches!(fixture.recovery.verified_among(&alice, &[alice.id()]).await, Err(ServiceError::Forbidden)));
}

#[tokio::test]
async fn every_flow_explains_when_email_is_disabled() {
    let fixture = Fixture::with_transport(MailTransport::Disabled).await;
    let admin = fixture.user("root", true).await;
    let alice = fixture.user("alice", false).await;
    assert!(!fixture.recovery.email_available());
    assert!(!fixture.services.recovery.email_available(), "the default configuration has no email");

    let unavailable =
        |result: Result<(), ServiceError>| matches!(result, Err(ServiceError::Invalid(message)) if message == EMAIL_UNAVAILABLE);
    assert!(unavailable(fixture.recovery.request_password_reset("alice@example.com", "192.0.2.1").await));
    assert!(unavailable(fixture.recovery.resend_verification(&alice).await));
    assert!(unavailable(fixture.recovery.admin_send_password_reset(&admin, alice.id()).await.map(drop)));
    assert!(fixture.throttle.checks().is_empty(), "nothing is throttled when nothing can be sent");

    // Setting a password needs no email.
    fixture.recovery.admin_set_password(&admin, alice.id(), NEW_PASSWORD.to_owned()).await.unwrap();
}
