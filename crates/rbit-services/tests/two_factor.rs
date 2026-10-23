//! Two-factor enrolment, the two-step sign-in, recovery codes, Git password refusal and
//! sign-in throttling against real, temporary `SQLite` and redb stores.

use std::sync::Arc;

use rbit_auth::{TotpSecret, totp::time_step};
use rbit_core::{Config, Timestamp, User};
use rbit_services::{
    ServiceError, Services, SignUp,
    accounts::ClientInfo,
    admin::ListQuery,
    audit::{AuditAction, AuditCategory},
    two_factor::{GitPasswordOutcome, RECOVERY_CODE_COUNT, SignInOutcome},
};
use rbit_storage::{Database, KvStore};

const PASSWORD: &str = "correct horse battery";

struct Fixture {
    _dir: tempfile::TempDir,
    database: Database,
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
        Self { _dir: dir, database, services }
    }

    async fn user(&self, name: &str) -> User {
        let input = SignUp { username: name.to_owned(), email: format!("{name}@example.com"), password: PASSWORD.to_owned() };
        self.services.accounts.create_user(input, None).await.unwrap()
    }

    /// The user's stored secret, as their authenticator app knows it.
    async fn secret(&self, user: &User) -> TotpSecret {
        let record = self.database.two_factor().find(user.id()).await.unwrap().unwrap();
        TotpSecret::from_bytes(&record.secret).unwrap()
    }

    /// Enrols `user`, returning their secret, the step used to confirm and the recovery codes.
    async fn enrol(&self, user: &User) -> (TotpSecret, u64, Vec<String>) {
        self.services.two_factor.begin_enrolment(user, "rbit").await.unwrap();
        let secret = self.secret(user).await;
        let step = time_step(Timestamp::now().seconds());
        let codes = self.services.confirm_two_factor(user, &code(&secret, step), &client()).await.unwrap().expect("first factor");
        (secret, step, codes)
    }

    async fn pending(&self, login: &str) -> rbit_auth::SessionToken {
        match self.services.sign_in(login, PASSWORD.to_owned(), &client()).await.unwrap() {
            SignInOutcome::SecondFactorRequired(token) => token,
            SignInOutcome::SignedIn(_) => panic!("expected a second step"),
        }
    }
}

fn client() -> ClientInfo {
    ClientInfo { ip: Some("203.0.113.5".parse().unwrap()), user_agent: "test".to_owned() }
}

fn code(secret: &TotpSecret, step: u64) -> String {
    format!("{:06}", secret.code_at_step(step))
}

#[tokio::test]
async fn enrolment_needs_a_confirming_code_and_issues_recovery_codes() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice").await;
    let two_factor = &fixture.services.two_factor;

    let first = two_factor.begin_enrolment(&alice, "My Forge").await.unwrap();
    assert!(first.provisioning_uri.starts_with("otpauth://totp/My%20Forge:alice?secret="));
    assert_eq!(two_factor.begin_enrolment(&alice, "My Forge").await.unwrap(), first, "reloading keeps the scanned secret");
    assert!(!two_factor.status(&alice).await.unwrap().is_enabled());

    let wrong = fixture.services.confirm_two_factor(&alice, "000000", &client()).await;
    assert!(matches!(wrong, Err(ServiceError::InvalidCredentials)));
    assert!(!two_factor.is_enabled(alice.id()).await.unwrap());

    let secret = fixture.secret(&alice).await;
    let codes = fixture
        .services
        .confirm_two_factor(&alice, &code(&secret, time_step(Timestamp::now().0)), &client())
        .await
        .unwrap()
        .expect("first factor");
    assert_eq!(codes.len(), RECOVERY_CODE_COUNT);
    let status = two_factor.status(&alice).await.unwrap();
    assert!(status.is_enabled());
    assert_eq!(status.recovery_codes_left, i64::try_from(RECOVERY_CODE_COUNT).unwrap());
    assert!(matches!(two_factor.begin_enrolment(&alice, "x").await, Err(ServiceError::Invalid(_))));
}

#[tokio::test]
async fn sign_in_asks_for_a_code_that_works_once() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice").await;
    let (secret, step, _) = fixture.enrol(&alice).await;

    let pending = fixture.pending("alice").await;
    // The code used to confirm enrolment has been spent.
    let replay = fixture.services.complete_sign_in(&pending, &code(&secret, step), &client()).await;
    assert!(matches!(replay, Err(ServiceError::InvalidCredentials)));

    let user = fixture.services.complete_sign_in(&pending, &code(&secret, step + 1), &client()).await.unwrap();
    assert_eq!(user.id(), alice.id());
    // The pending sign-in is gone, and the same code cannot open another one.
    assert!(matches!(fixture.services.complete_sign_in(&pending, &code(&secret, step + 1), &client()).await, Err(ServiceError::NotFound)));
    let again = fixture.pending("alice").await;
    assert!(matches!(
        fixture.services.complete_sign_in(&again, &code(&secret, step + 1), &client()).await,
        Err(ServiceError::InvalidCredentials)
    ));

    let log = fixture.services.audit.list(&alice, Some(AuditCategory::Authentication), &ListQuery::default()).await.unwrap();
    let actions: Vec<_> = log.items.iter().map(|event| event.action).collect();
    assert_eq!(actions, [AuditAction::TwoFactorFailed, AuditAction::SignIn, AuditAction::TwoFactorFailed]);
}

#[tokio::test]
async fn recovery_codes_are_single_use() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice").await;
    let (_, _, codes) = fixture.enrol(&alice).await;

    let pending = fixture.pending("alice@example.com").await;
    fixture.services.complete_sign_in(&pending, &codes[3].to_uppercase(), &client()).await.unwrap();
    let pending = fixture.pending("alice").await;
    assert!(matches!(fixture.services.complete_sign_in(&pending, &codes[3], &client()).await, Err(ServiceError::InvalidCredentials)));
    assert_eq!(
        fixture.services.two_factor.status(&alice).await.unwrap().recovery_codes_left,
        i64::try_from(RECOVERY_CODE_COUNT).unwrap() - 1
    );

    // Regenerating replaces every code.
    let fresh = fixture.services.regenerate_recovery_codes(&alice, PASSWORD.to_owned(), &codes[0], &client()).await.unwrap();
    assert_eq!(fresh.len(), RECOVERY_CODE_COUNT);
    let pending = fixture.pending("alice").await;
    assert!(fixture.services.complete_sign_in(&pending, &codes[1], &client()).await.is_err());
    fixture.services.complete_sign_in(&pending, &fresh[1], &client()).await.unwrap();
}

#[tokio::test]
async fn a_pending_sign_in_can_only_be_completed_once_even_with_different_proofs() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice").await;
    let (_, _, codes) = fixture.enrol(&alice).await;
    let pending = fixture.pending("alice").await;
    let client = client();
    let (first, second) = tokio::join!(
        fixture.services.complete_sign_in(&pending, &codes[0], &client),
        fixture.services.complete_sign_in(&pending, &codes[1], &client),
    );
    assert_eq!(usize::from(first.is_ok()) + usize::from(second.is_ok()), 1);
    let failure = if first.is_err() { first } else { second };
    assert!(matches!(failure, Err(ServiceError::NotFound)), "{failure:?}");
}

#[tokio::test]
async fn disabling_needs_the_password_and_a_code() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice").await;
    let (secret, step, _) = fixture.enrol(&alice).await;

    let wrong_password = fixture.services.disable_two_factor(&alice, "nope".to_owned(), &code(&secret, step + 1), &client()).await;
    assert!(matches!(wrong_password, Err(ServiceError::InvalidCredentials)));
    let replayed = fixture.services.disable_two_factor(&alice, PASSWORD.to_owned(), &code(&secret, step), &client()).await;
    assert!(matches!(replayed, Err(ServiceError::InvalidCredentials)));
    fixture.services.disable_two_factor(&alice, PASSWORD.to_owned(), &code(&secret, step + 1), &client()).await.unwrap();

    assert!(!fixture.services.two_factor.is_enabled(alice.id()).await.unwrap());
    assert!(matches!(fixture.services.sign_in("alice", PASSWORD.to_owned(), &client()).await.unwrap(), SignInOutcome::SignedIn(_)));

    let log = fixture.services.audit.list(&alice, Some(AuditCategory::TwoFactor), &ListQuery::default()).await.unwrap();
    let actions: Vec<_> = log.items.iter().map(|event| event.action).collect();
    assert_eq!(actions, [AuditAction::TwoFactorDisabled, AuditAction::TwoFactorEnabled]);
}

#[tokio::test]
async fn git_refuses_the_password_of_enrolled_accounts() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice").await;
    let client = client();
    let git = |password: &str| fixture.services.authenticate_git_password("alice", password.to_owned(), &client);

    assert!(matches!(git(PASSWORD).await.unwrap(), GitPasswordOutcome::Authenticated(_)));
    assert!(matches!(git("wrong").await, Err(ServiceError::InvalidCredentials)));
    fixture.enrol(&alice).await;
    assert!(matches!(git(PASSWORD).await.unwrap(), GitPasswordOutcome::TokenRequired));
}

#[tokio::test]
async fn wrong_codes_are_throttled_per_account() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice").await;
    fixture.enrol(&alice).await;

    let pending = fixture.pending("alice").await;
    let mut outcomes = Vec::new();
    for _ in 0..10 {
        outcomes.push(fixture.services.complete_sign_in(&pending, "000000", &client()).await);
        if matches!(outcomes.last(), Some(Err(ServiceError::RateLimited { .. }))) {
            break;
        }
    }
    assert!(matches!(outcomes.last(), Some(Err(ServiceError::RateLimited { .. }))), "{outcomes:?}");
    assert!(outcomes.len() <= 5);
}

/// Guesses `login`'s password from `client` until throttled; panics if never.
async fn guess_until_limited(fixture: &Fixture, login: &str, client: &ClientInfo) {
    for _ in 0..20 {
        match fixture.services.sign_in(login, "guess".to_owned(), client).await {
            Err(ServiceError::InvalidCredentials) => {}
            Err(ServiceError::RateLimited { .. }) => return,
            other => panic!("unexpected {other:?}"),
        }
    }
    panic!("{login} was never throttled");
}

fn address(ip: &str) -> ClientInfo {
    ClientInfo { ip: Some(ip.parse().unwrap()), user_agent: String::new() }
}

#[tokio::test]
async fn a_username_and_its_email_share_one_budget() {
    let fixture = Fixture::new().await;
    fixture.user("alice").await;

    guess_until_limited(&fixture, "alice", &address("192.0.2.1")).await;
    let other_alias = fixture.services.sign_in("alice@example.com", PASSWORD.to_owned(), &address("192.0.2.2")).await;
    assert!(matches!(other_alias, Err(ServiceError::RateLimited { .. })), "{other_alias:?}");

    // Unknown logins keep separate budgets per spelling, as before.
    guess_until_limited(&fixture, "ghost", &address("192.0.2.3")).await;
    let unknown = fixture.services.sign_in("ghost@example.com", "guess".to_owned(), &address("192.0.2.4")).await;
    assert!(matches!(unknown, Err(ServiceError::InvalidCredentials)), "{unknown:?}");
}

#[tokio::test]
async fn a_successful_sign_in_restores_the_login_budget_but_not_the_address_budget() {
    let fixture = Fixture::new().await;
    fixture.user("mallory").await;
    let attacker = address("192.0.2.66");

    // Spread guesses over many logins (each well under its own budget), signing in to
    // one's own account in between: the address still runs out.
    let mut limited = false;
    for round in 0..40 {
        for victim in 0..3 {
            match fixture.services.sign_in(&format!("victim{round}-{victim}"), "guess".to_owned(), &attacker).await {
                Err(ServiceError::InvalidCredentials) => {}
                Err(ServiceError::RateLimited { .. }) => limited = true,
                other => panic!("unexpected {other:?}"),
            }
        }
        if limited {
            break;
        }
        match fixture.services.sign_in("mallory", PASSWORD.to_owned(), &attacker).await {
            Ok(SignInOutcome::SignedIn(_)) => {}
            Err(ServiceError::RateLimited { .. }) => limited = true,
            other => panic!("unexpected {other:?}"),
        }
    }
    assert!(limited, "the address budget must not be refilled by signing in");

    // The login's own budget does refill on success.
    let fresh = Fixture::new().await;
    fresh.user("alice").await;
    for _ in 0..5 {
        assert!(matches!(fresh.services.sign_in("alice", "guess".to_owned(), &client()).await, Err(ServiceError::InvalidCredentials)));
    }
    assert!(fresh.services.sign_in("alice", PASSWORD.to_owned(), &client()).await.is_ok());
    for _ in 0..5 {
        assert!(matches!(fresh.services.sign_in("alice", "guess".to_owned(), &client()).await, Err(ServiceError::InvalidCredentials)));
    }
}

#[tokio::test]
async fn wrong_passwords_are_throttled_and_audited() {
    let fixture = Fixture::new().await;
    let admin = fixture.user("root").await;
    fixture.user("alice").await;

    let mut limited = false;
    for _ in 0..20 {
        match fixture.services.sign_in("alice", "guess".to_owned(), &client()).await {
            Err(ServiceError::InvalidCredentials) => {}
            Err(ServiceError::RateLimited { retry_after_seconds }) => {
                assert!(retry_after_seconds > 0);
                limited = true;
                break;
            }
            other => panic!("unexpected {other:?}"),
        }
    }
    assert!(limited);
    // The login is throttled even from another address; another login from a fresh address is not.
    let elsewhere = ClientInfo { ip: Some("198.51.100.99".parse().unwrap()), user_agent: String::new() };
    assert!(matches!(fixture.services.sign_in("alice", PASSWORD.to_owned(), &elsewhere).await, Err(ServiceError::RateLimited { .. })));
    assert!(fixture.services.sign_in("root", PASSWORD.to_owned(), &elsewhere).await.is_ok());

    let failures = fixture.services.audit.list(&admin, Some(AuditCategory::Authentication), &ListQuery::default()).await.unwrap();
    assert!(failures.items.iter().filter(|event| event.action == AuditAction::SignInFailed).count() >= 5);
    assert!(failures.items.iter().any(|event| event.ip_address.as_deref() == Some("203.0.113.5")));
}
