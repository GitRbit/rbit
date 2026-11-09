//! Profile application policy against an in-memory store with real bounded password work.

use rbit_contracts::{AccountId, Timestamp};
use rbit_identity::{
    Account, AccountKind, AccountName, Email, PasswordWork, User,
    profiles::{
        AccountProfile, AccountRenameRepository, CredentialThrottle, ProfileApplication, ProfileError, ProfileRepository, ProfileUpdate,
        commit_password_change,
    },
};
use rbit_kernel::BlockingBudget;
use std::{
    fmt,
    future::{Future, ready},
    num::NonZeroUsize,
    sync::Mutex,
};

#[derive(Debug)]
struct Failure;
impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("store unavailable")
    }
}
impl std::error::Error for Failure {}

#[derive(Default)]
struct State {
    profile: AccountProfile,
    hash: Option<String>,
    email: Option<String>,
    name: Option<String>,
    calls: Vec<String>,
    throttle_closed: bool,
}

#[derive(Default)]
struct Store(Mutex<State>);

impl Store {
    async fn with_password(password: &str, work: &PasswordWork) -> Self {
        let hash = work.begin().unwrap().hash(password.into()).await.unwrap();
        Self(Mutex::new(State { hash: Some(hash), ..State::default() }))
    }
    fn calls(&self) -> Vec<String> {
        self.0.lock().unwrap().calls.clone()
    }
    fn log(&self, call: &str) {
        self.0.lock().unwrap().calls.push(call.to_owned());
    }
}

impl ProfileRepository for Store {
    type Error = Failure;
    fn find_profile(&self, _: AccountId) -> impl Future<Output = Result<Option<AccountProfile>, Failure>> + Send {
        ready(Ok(Some(self.0.lock().unwrap().profile.clone())))
    }
    fn update_profile(&self, _: AccountId, profile: &AccountProfile) -> impl Future<Output = Result<(), Failure>> + Send {
        self.log("update");
        self.0.lock().unwrap().profile = profile.clone();
        ready(Ok(()))
    }
    fn password_hash(&self, _: AccountId) -> impl Future<Output = Result<Option<String>, Failure>> + Send {
        self.log("hash");
        ready(Ok(self.0.lock().unwrap().hash.clone()))
    }
    fn set_email_if_password(&self, _: AccountId, email: &Email, expected: &str) -> impl Future<Output = Result<bool, Failure>> + Send {
        self.log("email");
        let mut state = self.0.lock().unwrap();
        let current = state.hash.as_deref() == Some(expected);
        if current {
            state.email = Some(email.to_string());
        }
        ready(Ok(current))
    }
    fn set_password_hash_if_current(&self, _: AccountId, hash: &str, expected: &str) -> impl Future<Output = Result<bool, Failure>> + Send {
        self.log("password");
        let mut state = self.0.lock().unwrap();
        let current = state.hash.as_deref() == Some(expected);
        if current {
            state.hash = Some(hash.to_owned());
        }
        ready(Ok(current))
    }
}

impl AccountRenameRepository for Store {
    type Error = Failure;
    fn rename_if_password(&self, _: AccountId, name: &AccountName, expected: &str) -> impl Future<Output = Result<bool, Failure>> + Send {
        self.log("rename");
        let mut state = self.0.lock().unwrap();
        let current = state.hash.as_deref() == Some(expected);
        if current {
            state.name = Some(name.to_string());
        }
        ready(Ok(current))
    }
}

impl CredentialThrottle for Store {
    type Error = &'static str;
    fn check_rename(&self, _: AccountId) -> Result<(), &'static str> {
        self.log("check_rename");
        if self.0.lock().unwrap().throttle_closed { Err("throttled") } else { Ok(()) }
    }
    fn check_password(&self, _: AccountId) -> Result<(), &'static str> {
        self.log("check_password");
        if self.0.lock().unwrap().throttle_closed { Err("throttled") } else { Ok(()) }
    }
    fn record_password(&self, _: AccountId, valid: bool) {
        self.log(if valid { "reset" } else { "failure" });
    }
}

fn user() -> User {
    User {
        account: Account {
            id: AccountId(7),
            kind: AccountKind::User,
            name: "alice".parse().unwrap(),
            display_name: None,
            bio: None,
            created_at: Timestamp(0),
        },
        email: "alice@example.com".parse().unwrap(),
        is_admin: false,
    }
}

fn work() -> PasswordWork {
    PasswordWork::new(BlockingBudget::new(NonZeroUsize::new(2).unwrap()))
}

fn rejected<T: fmt::Debug>(result: Result<T, ProfileError<Failure, &'static str, ()>>) -> String {
    match result {
        Err(ProfileError::Rejected(rejection)) => rejection.to_string(),
        other => panic!("expected a rejection, got {other:?}"),
    }
}

#[tokio::test]
async fn profile_updates_are_validated_before_storage() {
    let (work, store) = (work(), Store::default());
    let app = ProfileApplication::new(&store, &store, &store, &work, 8);
    let input = ProfileUpdate { display_name: " Alice ".into(), website: "https://example.com".into(), ..ProfileUpdate::default() };
    let profile = app.update_profile(&user(), input).await.unwrap();
    assert_eq!(profile.display_name.as_deref(), Some("Alice"));
    assert_eq!(app.public_profile(AccountId(7)).await.unwrap().website.as_deref(), Some("https://example.com"));
    let bad = ProfileUpdate { website: "javascript:alert(1)".into(), ..ProfileUpdate::default() };
    assert!(matches!(app.update_profile(&user(), bad).await, Err(ProfileError::Rejected(_))));
    assert_eq!(store.calls(), ["update"]);
}

#[tokio::test]
async fn rename_checks_its_throttle_first_and_commits_against_the_verified_hash() {
    let work = work();
    let store = Store::with_password("correct horse", &work).await;
    let app = ProfileApplication::new(&store, &store, &store, &work, 8);
    store.0.lock().unwrap().throttle_closed = true;
    assert!(matches!(app.change_username(&user(), "correct horse".into(), "not a name!").await, Err(ProfileError::Throttle("throttled"))));
    assert_eq!(store.calls(), ["check_rename"], "throttled before the name is parsed");

    store.0.lock().unwrap().throttle_closed = false;
    store.0.lock().unwrap().calls.clear();
    assert!(matches!(app.change_username(&user(), "wrong".into(), "bob").await, Err(ProfileError::InvalidCredentials)));
    assert_eq!(store.calls(), ["check_rename", "check_password", "hash", "failure"]);
    let renamed = app.change_username(&user(), "correct horse".into(), " bob ").await.unwrap();
    assert_eq!(renamed.to_string(), "bob");
    assert_eq!(store.0.lock().unwrap().name.as_deref(), Some("bob"));
}

#[tokio::test]
async fn a_concurrent_password_change_makes_email_change_lose() {
    let work = work();
    let store = Store::with_password("correct horse", &work).await;
    let app = ProfileApplication::new(&store, &store, &store, &work, 8);
    assert!(matches!(app.change_email(&user(), "x".into(), "not an email").await, Err(ProfileError::Name(_))));
    let email = app.change_email(&user(), "correct horse".into(), " new@example.com ").await.unwrap();
    assert_eq!(email.to_string(), "new@example.com");
    // A hash replaced between verification and commit makes the commit lose.
    assert!(!store.set_email_if_password(AccountId(7), &email, "stale").await.unwrap());
    assert!(matches!(commit_password_change(&store, AccountId(7), "new", "stale").await, Err(ProfileError::InvalidCredentials)));
}

#[tokio::test]
async fn password_changes_validate_reserve_verify_then_hash() {
    let alice = user();
    let work = work();
    let store = Store::with_password("correct horse", &work).await;
    let app = ProfileApplication::new(&store, &store, &store, &work, 8);
    let short = app.prepare_password_change(&alice, "correct horse".into(), "short".into(), || Ok::<_, ()>(()));
    assert_eq!(rejected(short.await), "new password must be at least 8 characters");
    let long = app.prepare_password_change(&alice, "correct horse".into(), "x".repeat(257), || Ok::<_, ()>(()));
    assert_eq!(rejected(long.await), "new password must be at most 256 characters");
    let same = app.prepare_password_change(&alice, "correct horse".into(), "correct horse".into(), || Ok::<_, ()>(()));
    assert_eq!(rejected(same.await), "choose a different password from your current password");
    assert_eq!(store.calls(), Vec::<String>::new(), "validation precedes throttle and storage");

    let closed = app.prepare_password_change(&alice, "correct horse".into(), "battery staple".into(), || Err::<(), _>(()));
    assert!(matches!(closed.await, Err(ProfileError::Reserve(()))));
    assert_eq!(store.calls(), ["check_password", "hash"], "a closed owner is reported after loading, before verification");

    store.0.lock().unwrap().calls.clear();
    let wrong = app.prepare_password_change(&alice, "wrong".into(), "battery staple".into(), || Ok::<_, ()>(()));
    assert!(matches!(wrong.await, Err(ProfileError::InvalidCredentials)));
    assert_eq!(store.calls(), ["check_password", "hash", "failure"]);

    let prepared = app.prepare_password_change(&alice, "correct horse".into(), "battery staple".into(), || Ok::<_, ()>(7)).await.unwrap();
    assert_eq!(prepared.ownership, 7);
    commit_password_change(&store, AccountId(7), &prepared.hash, &prepared.verified).await.unwrap();
    let stored = store.0.lock().unwrap().hash.clone().unwrap();
    assert!(work.begin().unwrap().verify("battery staple".into(), stored).await.unwrap());
}
