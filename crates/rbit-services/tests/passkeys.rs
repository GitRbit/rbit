//! Passkeys as a second factor: registration, the two-step sign-in, confirming sensitive
//! changes, and how passkeys and the authenticator app share recovery codes. A software
//! authenticator plays the browser's part against real, temporary `SQLite` and redb stores.

use std::sync::Arc;

use rbit_auth::{TotpSecret, totp::time_step};
use rbit_core::{Config, Timestamp, User};
use rbit_identity::PasswordWork;
use rbit_kernel::BlockingBudget;
use rbit_services::{
    ServiceError, Services, SignUp,
    accounts::ClientInfo,
    admin::ListQuery,
    audit::{AuditAction, AuditCategory},
    two_factor::{GitPasswordOutcome, PasskeyRegistered, RECOVERY_CODE_COUNT, SecondFactorProof, SignInOutcome},
};
use rbit_storage::{Database, KvStore};
use serde_json::{Value, json};

const PASSWORD: &str = "correct horse battery";

struct Fixture {
    directory: tempfile::TempDir,
    database: Database,
    services: Services,
    passwords: PasswordWork,
}

impl Fixture {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.security.max_password_jobs = 1;
        config.storage.data_dir = dir.path().to_path_buf();
        let config = Arc::new(config);
        let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
        let kv = KvStore::open(&config.storage.kv_path()).unwrap();
        let passwords = PasswordWork::new(BlockingBudget::new(std::num::NonZeroUsize::new(1).unwrap()));
        let services = Services::with_password_work(config, &database, &kv, passwords.clone()).await.unwrap();
        Self { directory: dir, database, services, passwords }
    }

    async fn user(&self, name: &str) -> User {
        let input = SignUp { username: name.to_owned(), email: format!("{name}@example.com"), password: PASSWORD.to_owned() };
        self.services.accounts.create_user(input, None).await.unwrap()
    }

    async fn register(&self, user: &User, authenticator: &Authenticator, name: &str) -> Result<PasskeyRegistered, ServiceError> {
        let options = self.services.two_factor.creation_options(user).await.unwrap();
        self.services.add_passkey(user, name, &authenticator.create(&options), &client()).await
    }

    async fn pending(&self) -> rbit_auth::SessionToken {
        match self.services.sign_in("alice", PASSWORD.to_owned(), &client()).await.unwrap() {
            SignInOutcome::SecondFactorRequired(token) => token,
            SignInOutcome::SignedIn(_) => panic!("expected a second step"),
        }
    }

    /// Sets up the authenticator app; returns its secret and the step used to confirm.
    async fn enrol_app(&self, user: &User) -> (TotpSecret, u64) {
        self.services.two_factor.begin_enrolment(user, "rbit").await.unwrap();
        let record = self.database.two_factor().find(user.id()).await.unwrap().unwrap();
        let secret = TotpSecret::from_bytes(&record.secret).unwrap();
        let step = time_step(Timestamp::now().seconds());
        self.services.confirm_two_factor(user, &format!("{:06}", secret.code_at_step(step)), &client()).await.unwrap();
        (secret, step)
    }
}

fn client() -> ClientInfo {
    ClientInfo { ip: Some("203.0.113.9".parse().unwrap()), user_agent: "test".to_owned() }
}

#[path = "support/authenticator.rs"]
mod authenticator;
use authenticator::Authenticator;

#[tokio::test]
async fn a_passkey_alone_turns_on_two_factor_and_signs_in_once_per_challenge() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice").await;
    let key = Authenticator::new(1);

    let PasskeyRegistered::FirstFactor(codes) = fixture.register(&alice, &key, " YubiKey ").await.unwrap() else {
        panic!("the first passkey should issue recovery codes");
    };
    assert_eq!(codes.len(), RECOVERY_CODE_COUNT);
    let status = fixture.services.two_factor.status(&alice).await.unwrap();
    assert!(status.is_enabled() && !status.has_authenticator());
    assert_eq!(status.passkeys.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["YubiKey"]);
    assert!(matches!(
        fixture.services.authenticate_git_password("alice", PASSWORD.to_owned(), &client()).await.unwrap(),
        GitPasswordOutcome::TokenRequired
    ));

    let pending = fixture.pending().await;
    // Without the page's challenge, a passkey answer is refused.
    let unasked = key.get(&fixture.services.two_factor.confirmation_options(&alice).await.unwrap().unwrap());
    assert!(matches!(
        fixture.services.complete_sign_in(&pending, SecondFactorProof::Passkey(&unasked), &client()).await,
        Err(ServiceError::InvalidCredentials)
    ));

    let options = fixture.services.two_factor.sign_in_options(&pending).await.unwrap().expect("alice has a passkey");
    let answer = key.get(&options);
    let user = fixture.services.complete_sign_in(&pending, SecondFactorProof::Passkey(&answer), &client()).await.unwrap();
    assert_eq!(user.id(), alice.id());

    // The same answer can't open a second sign-in: its challenge is gone.
    let again = fixture.pending().await;
    fixture.services.two_factor.sign_in_options(&again).await.unwrap();
    assert!(matches!(
        fixture.services.complete_sign_in(&again, SecondFactorProof::Passkey(&answer), &client()).await,
        Err(ServiceError::InvalidCredentials)
    ));
    // Recovery codes work for passkey-only accounts too.
    fixture.services.complete_sign_in(&again, &codes[0], &client()).await.unwrap();

    let log = fixture.services.audit.list(&alice, Some(AuditCategory::Authentication), &ListQuery::default()).await.unwrap();
    let details: Vec<_> = log.items.iter().map(|event| (event.action, event.detail.as_str())).collect();
    assert!(details.contains(&(AuditAction::SignIn, "password and passkey")), "{details:?}");
}

#[tokio::test]
async fn registration_refuses_duplicates_and_stale_challenges() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice").await;
    let bob = fixture.user("bob").await;
    let key = Authenticator::new(2);

    fixture.register(&alice, &key, "Laptop").await.unwrap();
    assert!(matches!(fixture.register(&alice, &key, "Again").await, Err(ServiceError::Invalid(_))));
    assert!(matches!(fixture.register(&bob, &key, "Stolen").await, Err(ServiceError::Invalid(_))));

    // A recent challenge stays valid while a newer one is out (a second tab), but each
    // works once, and a sign-in confirmation challenge can't register a passkey.
    let options = fixture.services.two_factor.creation_options(&alice).await.unwrap();
    let other = Authenticator::new(3);
    let answer = other.create(&options);
    fixture.services.two_factor.creation_options(&alice).await.unwrap();
    assert_eq!(fixture.services.add_passkey(&alice, "Phone", &answer, &client()).await.unwrap(), PasskeyRegistered::Added);
    assert!(matches!(fixture.services.add_passkey(&alice, "Phone", &answer, &client()).await, Err(ServiceError::Invalid(_))));

    let confirm = fixture.services.two_factor.confirmation_options(&alice).await.unwrap().unwrap();
    let mut wrong_ceremony: Value = serde_json::from_str(&confirm).unwrap();
    wrong_ceremony["rp"] = json!({ "id": "localhost" });
    let answer = Authenticator::new(6).create(&wrong_ceremony.to_string());
    assert!(matches!(fixture.services.add_passkey(&alice, "Tablet", &answer, &client()).await, Err(ServiceError::Invalid(_))));
    assert_eq!(fixture.services.two_factor.status(&alice).await.unwrap().passkeys.len(), 2);
}

#[tokio::test]
async fn factors_share_recovery_codes_until_the_last_one_goes() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice").await;
    let key = Authenticator::new(4);
    let (secret, step) = fixture.enrol_app(&alice).await;

    // Adding a passkey next to the app keeps the existing codes.
    assert_eq!(fixture.register(&alice, &key, "Key").await.unwrap(), PasskeyRegistered::Added);
    let codes_left = || async { fixture.services.two_factor.status(&alice).await.unwrap().recovery_codes_left };
    assert_eq!(codes_left().await, i64::try_from(RECOVERY_CODE_COUNT).unwrap());

    // Removing the app, confirmed with the passkey, leaves 2FA on with the same codes.
    let options = fixture.services.two_factor.confirmation_options(&alice).await.unwrap().unwrap();
    fixture
        .services
        .disable_two_factor(&alice, PASSWORD.to_owned(), SecondFactorProof::Passkey(&key.get(&options)), &client())
        .await
        .unwrap();
    let status = fixture.services.two_factor.status(&alice).await.unwrap();
    assert!(status.is_enabled() && !status.has_authenticator());
    assert_eq!(status.recovery_codes_left, i64::try_from(RECOVERY_CODE_COUNT).unwrap());

    // An app code no longer counts; removing the last passkey needs a passkey or recovery code.
    let passkey = status.passkeys[0].id;
    let app_code = format!("{:06}", secret.code_at_step(step + 1));
    let refused = fixture.services.remove_passkey(&alice, passkey, PASSWORD.to_owned(), &app_code, &client()).await;
    assert!(matches!(refused, Err(ServiceError::InvalidCredentials)));
    let options = fixture.services.two_factor.confirmation_options(&alice).await.unwrap().unwrap();
    let wrong_password = fixture
        .services
        .remove_passkey(&alice, passkey, "nope".to_owned(), SecondFactorProof::Passkey(&key.get(&options)), &client())
        .await;
    assert!(matches!(wrong_password, Err(ServiceError::InvalidCredentials)));
    let options = fixture.services.two_factor.confirmation_options(&alice).await.unwrap().unwrap();
    fixture
        .services
        .remove_passkey(&alice, passkey, PASSWORD.to_owned(), SecondFactorProof::Passkey(&key.get(&options)), &client())
        .await
        .unwrap();

    assert!(!fixture.services.two_factor.is_enabled(alice.id()).await.unwrap());
    assert_eq!(codes_left().await, 0);
    assert!(matches!(fixture.services.sign_in("alice", PASSWORD.to_owned(), &client()).await.unwrap(), SignInOutcome::SignedIn(_)));

    let log = fixture.services.audit.list(&alice, Some(AuditCategory::TwoFactor), &ListQuery::default()).await.unwrap();
    let actions: Vec<_> = log.items.iter().map(|event| event.action).collect();
    assert_eq!(
        actions,
        [AuditAction::PasskeyRemoved, AuditAction::TwoFactorDisabled, AuditAction::PasskeyAdded, AuditAction::TwoFactorEnabled]
    );
}

#[tokio::test]
async fn recovery_codes_can_be_regenerated_with_a_passkey() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice").await;
    let key = Authenticator::new(5);
    let PasskeyRegistered::FirstFactor(old) = fixture.register(&alice, &key, "Key").await.unwrap() else { panic!() };

    let options = fixture.services.two_factor.confirmation_options(&alice).await.unwrap().unwrap();
    let fresh = fixture
        .services
        .regenerate_recovery_codes(&alice, PASSWORD.to_owned(), SecondFactorProof::Passkey(&key.get(&options)), &client())
        .await
        .unwrap();
    assert_eq!(fresh.len(), RECOVERY_CODE_COUNT);
    let pending = fixture.pending().await;
    assert!(fixture.services.complete_sign_in(&pending, &old[0], &client()).await.is_err());
    fixture.services.complete_sign_in(&pending, &fresh[0], &client()).await.unwrap();
}

#[tokio::test]
async fn overload_preserves_a_passkey_challenge_but_an_attempted_wrong_password_spends_it() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice").await;
    let key = Authenticator::new(9);
    fixture.register(&alice, &key, "Key").await.unwrap();
    let options = fixture.services.two_factor.confirmation_options(&alice).await.unwrap().unwrap();
    let response = key.get(&options);
    let occupied = fixture.passwords.begin().unwrap();
    let busy = fixture.services.regenerate_recovery_codes(&alice, PASSWORD.into(), SecondFactorProof::Passkey(&response), &client()).await;
    assert!(matches!(busy, Err(ServiceError::Unavailable { retry_after_seconds: 1 })));
    assert_eq!(fixture.services.two_factor.status(&alice).await.unwrap().recovery_codes_left, 10);
    drop(occupied);
    fixture.services.regenerate_recovery_codes(&alice, PASSWORD.into(), SecondFactorProof::Passkey(&response), &client()).await.unwrap();

    let options = fixture.services.two_factor.confirmation_options(&alice).await.unwrap().unwrap();
    let response = key.get(&options);
    let wrong =
        fixture.services.regenerate_recovery_codes(&alice, "wrong password".into(), SecondFactorProof::Passkey(&response), &client()).await;
    assert!(matches!(wrong, Err(ServiceError::InvalidCredentials)));
    let replay =
        fixture.services.regenerate_recovery_codes(&alice, PASSWORD.into(), SecondFactorProof::Passkey(&response), &client()).await;
    assert!(matches!(replay, Err(ServiceError::InvalidCredentials)));
}

mod original_proof;
