//! Shared admission at real service boundaries, without elapsed-time assertions.

use std::{num::NonZeroUsize, sync::Arc, time::Duration};

use rbit_core::{Config, TokenScope, User};
use rbit_identity::PasswordWork;
use rbit_kernel::BlockingBudget;
use rbit_services::{CreateAccessToken, ServiceError, Services, SignUp, accounts::ClientInfo};
use rbit_storage::sqlite::admin::ListFilter;
use rbit_storage::{Database, KvStore};

const PASSWORD: &str = "correct horse battery";
const REPLACEMENT: &str = "a new correct passphrase";

struct Fixture {
    _dir: tempfile::TempDir,
    database: Database,
    services: Services,
    passwords: PasswordWork,
}

impl Fixture {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.storage.data_dir = dir.path().to_path_buf();
        config.security.max_password_jobs = 1;
        let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
        let kv = KvStore::open(&config.storage.kv_path()).unwrap();
        let passwords = PasswordWork::new(BlockingBudget::new(NonZeroUsize::new(1).unwrap()));
        let services = Services::with_password_work(Arc::new(config), &database, &kv, passwords.clone()).await.unwrap();
        Self { _dir: dir, database, services, passwords }
    }

    async fn user(&self, name: &str, admin: bool) -> User {
        self.services.accounts.create_user(input(name), Some(admin)).await.unwrap()
    }
}

fn input(name: &str) -> SignUp {
    SignUp { username: name.into(), email: format!("{name}@example.com"), password: PASSWORD.into() }
}

fn unavailable<T>(result: &Result<T, ServiceError>) {
    assert!(matches!(result, Err(ServiceError::Unavailable { retry_after_seconds: 1 })));
}

#[tokio::test]
async fn busy_login_shapes_share_rejection_without_failure_surcharges_or_token_disruption() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice", false).await;
    let (_, token) = fixture
        .services
        .tokens
        .create(&alice, CreateAccessToken { name: "automation".into(), scope: TokenScope::Write, expires_in_days: None })
        .await
        .unwrap();
    let client = ClientInfo { ip: Some("198.51.100.7".parse().unwrap()), user_agent: "test".into() };
    let audit_before = fixture.database.audit().list(None, ListFilter { search: "", limit: 100, offset: 0 }).await.unwrap();
    let occupied = fixture.passwords.begin().unwrap();
    for _ in 0..8 {
        unavailable(&fixture.services.sign_in("alice", "wrong".into(), &client).await);
        unavailable(&fixture.services.sign_in("missing", "wrong".into(), &client).await);
    }
    unavailable(&fixture.services.accounts.authenticate("alice", PASSWORD.into()).await);
    let (authenticated, scope) = fixture.services.tokens.authenticate(&token).await.unwrap().unwrap();
    assert_eq!(authenticated.id(), alice.id());
    assert_eq!(scope, TokenScope::Write);
    let audit_after = fixture.database.audit().list(None, ListFilter { search: "", limit: 100, offset: 0 }).await.unwrap();
    assert_eq!(audit_after.total, audit_before.total);
    assert_eq!(audit_after.items, audit_before.items, "overload must not append failed-login audits for known or missing names");
    drop(occupied);
    // Eight ordinary request charges fit; eight bad-credential surcharges would not.
    fixture.services.sign_in("alice", PASSWORD.into(), &client).await.unwrap();
}

#[tokio::test]
async fn every_password_mutation_rejects_before_changing_account_or_session() {
    let fixture = Fixture::new().await;
    let admin = fixture.user("root", true).await;
    let alice = fixture.user("alice", false).await;
    let before = fixture.database.accounts().find_login("alice").await.unwrap().unwrap().password_hash;
    let session = fixture.services.accounts.start_session(&alice).await.unwrap();
    let occupied = fixture.passwords.begin().unwrap();
    unavailable(&fixture.services.accounts.create_user(input("bob"), Some(false)).await);
    unavailable(&fixture.services.admin.create_user(&admin, input("carol"), false).await);
    unavailable(&fixture.services.profiles.change_username(&alice, PASSWORD.into(), "renamed").await);
    unavailable(&fixture.services.profiles.change_email(&alice, PASSWORD.into(), "new@example.com").await);
    unavailable(&fixture.services.profiles.change_password(&alice, PASSWORD.into(), REPLACEMENT.into()).await);
    unavailable(&fixture.services.recovery.admin_set_password(&admin, alice.id(), REPLACEMENT.into()).await);
    assert!(fixture.database.accounts().find_login("bob").await.unwrap().is_none());
    assert!(fixture.database.accounts().find_login("carol").await.unwrap().is_none());
    let after = fixture.database.accounts().find_login("alice").await.unwrap().unwrap();
    assert_eq!(after.password_hash, before);
    assert_eq!(after.user.email, alice.email);
    assert!(fixture.services.accounts.resume_session(&session).await.unwrap().is_some());
    drop(occupied);
}

#[tokio::test]
async fn closed_admission_and_malformed_credentials_remain_distinct() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice", false).await;
    for malformed in ["", "$argon2id$v=19$m=19456"] {
        fixture.database.profiles().set_password_hash(alice.id(), malformed).await.unwrap();
        assert!(matches!(fixture.services.accounts.authenticate("alice", PASSWORD.into()).await, Err(ServiceError::InvalidCredentials)));
    }
    fixture.services.shutdown().await;
    unavailable(&fixture.services.accounts.authenticate("alice", PASSWORD.into()).await);
    unavailable(&fixture.services.accounts.authenticate("missing", PASSWORD.into()).await);
}

#[tokio::test]
async fn injected_capacity_cannot_bypass_configuration() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = Config::default();
    config.storage.data_dir = dir.path().into();
    config.security.max_password_jobs = 1;
    let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
    let kv = KvStore::open(&config.storage.kv_path()).unwrap();
    let excessive = PasswordWork::new(BlockingBudget::new(NonZeroUsize::new(2).unwrap()));
    assert!(matches!(Services::with_password_work(Arc::new(config), &database, &kv, excessive).await, Err(ServiceError::Invalid(_))));
}

#[test]
fn concurrent_password_replacement_cannot_overwrite_a_newer_hash() {
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().max_blocking_threads(1).build().unwrap();
    runtime.block_on(async {
        let fixture = Fixture::new().await;
        let alice = fixture.user("alice", false).await;
        let concurrent = fixture.passwords.begin().unwrap().hash("administrator replacement".into()).await.unwrap();
        let (release, wait) = std::sync::mpsc::channel();
        let (started, ready) = tokio::sync::oneshot::channel();
        let blocker = tokio::task::spawn_blocking(move || {
            started.send(()).unwrap();
            wait.recv().unwrap();
        });
        ready.await.unwrap();
        let profiles = fixture.services.profiles.clone();
        let user = alice.clone();
        let change = tokio::spawn(async move { profiles.change_password(&user, PASSWORD.into(), REPLACEMENT.into()).await });
        tokio::time::timeout(Duration::from_secs(10), async {
            while fixture.passwords.admitted() == 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        // Its worker is queued behind the barrier, after the old hash was read.
        fixture.database.profiles().set_password_hash(alice.id(), &concurrent).await.unwrap();
        release.send(()).unwrap();
        blocker.await.unwrap();
        assert!(matches!(change.await.unwrap(), Err(ServiceError::InvalidCredentials)));
        assert_eq!(fixture.database.accounts().find_login("alice").await.unwrap().unwrap().password_hash, concurrent);
    });
}
