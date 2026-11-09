//! Authenticator enrolment and one-use code proofs against an in-memory store.

use rbit_contracts::{AccountId, Timestamp};
use rbit_identity::{
    Account, AccountKind, User,
    two_factor::{
        CodeFactor, RECOVERY_CODE_COUNT, RecoveryDigest, SecondFactorClaims, TwoFactorApplication, TwoFactorError, TwoFactorRecord,
        TwoFactorRepository,
        totp::{RecoveryCode, TotpSecret, time_step},
    },
};
use std::{
    fmt,
    future::{Future, ready},
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
    record: Option<TwoFactorRecord>,
    codes: Vec<(RecoveryDigest, bool)>,
    other_factor: bool,
}

#[derive(Default)]
struct Store(Mutex<State>);

impl TwoFactorRepository for Store {
    type Error = Failure;
    fn find(&self, _: AccountId) -> impl Future<Output = Result<Option<TwoFactorRecord>, Failure>> + Send {
        ready(Ok(self.0.lock().unwrap().record.clone()))
    }
    fn begin_enrolment(&self, _: AccountId, secret: &[u8]) -> impl Future<Output = Result<bool, Failure>> + Send {
        let mut state = self.0.lock().unwrap();
        if state.record.as_ref().is_some_and(TwoFactorRecord::is_enabled) {
            return ready(Ok(false));
        }
        state.record =
            Some(TwoFactorRecord { secret: secret.to_vec(), created_at: Timestamp(0), confirmed_at: None, last_used_step: None });
        ready(Ok(true))
    }
    fn cancel_enrolment(&self, _: AccountId) -> impl Future<Output = Result<(), Failure>> + Send {
        let mut state = self.0.lock().unwrap();
        if !state.record.as_ref().is_some_and(TwoFactorRecord::is_enabled) {
            state.record = None;
        }
        ready(Ok(()))
    }
    fn confirm(
        &self,
        _: AccountId,
        secret: &[u8],
        step: u64,
        recovery: &[RecoveryDigest],
    ) -> impl Future<Output = Result<Option<bool>, Failure>> + Send {
        let mut state = self.0.lock().unwrap();
        let first = !state.other_factor;
        let Some(record) = state.record.as_mut().filter(|record| !record.is_enabled() && record.secret == secret) else {
            return ready(Ok(None));
        };
        record.confirmed_at = Some(Timestamp(1));
        record.last_used_step = Some(step);
        if first {
            state.codes = recovery.iter().map(|digest| (*digest, false)).collect();
        }
        ready(Ok(Some(first)))
    }
    fn replace_recovery_codes(&self, _: AccountId, digests: &[RecoveryDigest]) -> impl Future<Output = Result<bool, Failure>> + Send {
        let mut state = self.0.lock().unwrap();
        let enabled = state.record.as_ref().is_some_and(TwoFactorRecord::is_enabled) || state.other_factor;
        if enabled {
            state.codes = digests.iter().map(|digest| (*digest, false)).collect();
        }
        ready(Ok(enabled))
    }
}

impl SecondFactorClaims for Store {
    type Error = &'static str;
    fn spend_recovery_code(&self, _: AccountId, digest: &RecoveryDigest) -> impl Future<Output = Result<bool, &'static str>> + Send {
        let mut state = self.0.lock().unwrap();
        let mut spent = false;
        if let Some(entry) = state.codes.iter_mut().find(|(stored, used)| stored == digest && !used) {
            entry.1 = true;
            spent = true;
        }
        ready(Ok(spent))
    }
    fn claim_step(&self, _: AccountId, _: &[u8], step: u64) -> impl Future<Output = Result<bool, &'static str>> + Send {
        let mut state = self.0.lock().unwrap();
        let Some(record) = state.record.as_mut() else { return ready(Ok(false)) };
        let fresh = record.last_used_step.is_none_or(|last| last < step);
        if fresh {
            record.last_used_step = Some(step);
        }
        ready(Ok(fresh))
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

fn current_code(store: &Store, offset: u64) -> String {
    let secret = TotpSecret::from_bytes(&store.0.lock().unwrap().record.as_ref().unwrap().secret).unwrap();
    format!("{:06}", secret.code_at_step(time_step(Timestamp::now().seconds()) + offset))
}

fn rejection<T: fmt::Debug, C: fmt::Debug>(result: Result<T, TwoFactorError<Failure, C>>) -> String {
    match result {
        Err(TwoFactorError::Rejected(rejection)) => rejection.to_string(),
        other => panic!("expected a rejection, got {other:?}"),
    }
}

#[tokio::test]
async fn enrolment_resumes_the_pending_secret_and_issues_codes_for_the_first_factor() {
    let store = Store::default();
    let app = TwoFactorApplication::new(&store);
    let first = app.begin_enrolment(&user(), "rbit").await.unwrap();
    let again = app.begin_enrolment(&user(), "rbit").await.unwrap();
    assert_eq!(first, again, "a reloaded page keeps the scanned secret");
    assert!(first.provisioning_uri.starts_with("otpauth://totp/rbit:alice?secret="));
    assert!(matches!(app.confirm(&user(), "not a code").await, Err(TwoFactorError::InvalidCredentials)));

    let codes = app.confirm(&user(), &current_code(&store, 0)).await.unwrap().expect("first factor");
    assert_eq!(codes.len(), RECOVERY_CODE_COUNT);
    assert_eq!(rejection(app.begin_enrolment(&user(), "rbit").await), "an authenticator app is already set up");
    assert_eq!(rejection(app.confirm(&user(), "123456").await), "there is no authenticator setup in progress; start again");
}

#[tokio::test]
async fn a_second_factor_keeps_existing_recovery_codes() {
    let store = Store::default();
    store.0.lock().unwrap().other_factor = true;
    let app = TwoFactorApplication::new(&store);
    app.begin_enrolment(&user(), "rbit").await.unwrap();
    assert!(app.confirm(&user(), &current_code(&store, 0)).await.unwrap().is_none());
}

#[tokio::test]
async fn codes_work_once_and_recovery_codes_are_spent() {
    let store = Store::default();
    let app = TwoFactorApplication::new(&store);
    app.begin_enrolment(&user(), "rbit").await.unwrap();
    let codes = app.confirm(&user(), &current_code(&store, 0)).await.unwrap().unwrap();

    let next = current_code(&store, 1);
    assert_eq!(app.verify_code(&user(), &next, &store).await.unwrap(), CodeFactor::Authenticator);
    assert!(matches!(app.verify_code(&user(), &next, &store).await, Err(TwoFactorError::InvalidCredentials)), "replay");
    assert!(
        matches!(app.verify_code(&user(), &current_code(&store, 0), &store).await, Err(TwoFactorError::InvalidCredentials)),
        "older step"
    );

    let typed = codes[0].formatted().to_uppercase();
    assert_eq!(app.verify_code(&user(), &typed, &store).await.unwrap(), CodeFactor::RecoveryCode);
    assert!(matches!(app.verify_code(&user(), &typed, &store).await, Err(TwoFactorError::InvalidCredentials)), "spent");
    assert!(RecoveryCode::parse(&typed).is_some());
}

#[tokio::test]
async fn regeneration_replaces_codes_and_needs_a_factor() {
    let store = Store::default();
    let app = TwoFactorApplication::new(&store);
    assert_eq!(rejection(app.regenerate_recovery_codes(&user()).await), "two-factor authentication is not enabled");
    app.begin_enrolment(&user(), "rbit").await.unwrap();
    let old = app.confirm(&user(), &current_code(&store, 0)).await.unwrap().unwrap();
    let new = app.regenerate_recovery_codes(&user()).await.unwrap();
    assert_eq!(new.len(), RECOVERY_CODE_COUNT);
    assert!(matches!(app.verify_code(&user(), &old[0].formatted(), &store).await, Err(TwoFactorError::InvalidCredentials)));
    assert_eq!(app.verify_code(&user(), &new[0].formatted(), &store).await.unwrap(), CodeFactor::RecoveryCode);
}
