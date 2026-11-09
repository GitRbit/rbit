//! The pending second step and confirmations against in-memory ports, including races
//! released together at the step's post-lookup point.

use rbit_contracts::{AccountId, Timestamp};
use rbit_identity::{
    Account, AccountKind, PasswordWork, SessionToken, User,
    passkeys::{ChallengeBook, NewPasskey, PasskeyAdded, PasskeyClaims, PasskeyRecord, PasskeyRepository},
    second_factor::{
        PENDING_SIGN_IN_ATTEMPTS, PendingSignIns, SecondFactorProof, SecondFactorStep, SecondFactorThrottle, SecondStepAccounts,
        SecondStepError,
    },
    two_factor::{CredentialProofWrite, RecoveryDigest, SecondFactorClaims, TwoFactorRecord, TwoFactorRepository, totp::RecoveryCode},
};
use rbit_identity_models::credential::CredentialEpoch;
use rbit_kernel::BlockingBudget;
use std::{
    fmt,
    future::{Future, ready},
    num::NonZeroUsize,
    sync::{Arc, Mutex},
};
use tokio::sync::Barrier;

#[derive(Debug)]
struct Failure;
impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("store unavailable")
    }
}
impl std::error::Error for Failure {}

/// Store, accounts, throttle and claims in one recording fake.
#[derive(Default)]
struct Fake {
    codes: Mutex<Vec<(RecoveryDigest, bool)>>,
    password_hash: Mutex<Option<String>>,
    suspended: Mutex<bool>,
    calls: Mutex<Vec<String>>,
}

impl Fake {
    fn log(&self, call: &str) {
        self.calls.lock().unwrap().push(call.to_owned());
    }
    fn calls(&self) -> Vec<String> {
        std::mem::take(&mut *self.calls.lock().unwrap())
    }
    fn with_codes(codes: &[RecoveryCode]) -> Self {
        let fake = Self::default();
        *fake.codes.lock().unwrap() = codes.iter().map(|code| (code.digest(7), false)).collect();
        fake
    }
}

impl TwoFactorRepository for Fake {
    type Error = Failure;
    fn find(&self, _: AccountId) -> impl Future<Output = Result<Option<TwoFactorRecord>, Failure>> + Send {
        ready(Ok(None))
    }
    fn begin_enrolment(&self, _: AccountId, _: &[u8]) -> impl Future<Output = Result<bool, Failure>> + Send {
        ready(Ok(false))
    }
    fn cancel_enrolment(&self, _: AccountId) -> impl Future<Output = Result<(), Failure>> + Send {
        ready(Ok(()))
    }
    fn confirm(&self, _: AccountId, _: &[u8], _: u64, _: &[RecoveryDigest]) -> impl Future<Output = Result<Option<bool>, Failure>> + Send {
        ready(Ok(None))
    }
    fn replace_recovery_codes(&self, _: AccountId, _: &[RecoveryDigest]) -> impl Future<Output = Result<bool, Failure>> + Send {
        ready(Ok(false))
    }
    fn disable(&self, _: AccountId) -> impl Future<Output = Result<bool, Failure>> + Send {
        self.log("disable");
        ready(Ok(true))
    }
}

impl PasskeyRepository for Fake {
    type Error = Failure;
    fn passkeys(&self, _: AccountId) -> impl Future<Output = Result<Vec<PasskeyRecord>, Failure>> + Send {
        ready(Ok(Vec::new()))
    }
    fn find_passkey(&self, _: AccountId, _: &[u8]) -> impl Future<Output = Result<Option<PasskeyRecord>, Failure>> + Send {
        ready(Ok(None))
    }
    fn add_passkey(
        &self,
        _: AccountId,
        _: &NewPasskey<'_>,
        _: &[RecoveryDigest],
    ) -> impl Future<Output = Result<PasskeyAdded, Failure>> + Send {
        ready(Ok(PasskeyAdded::Added))
    }
    fn remove_passkey(&self, _: AccountId, _: i64) -> impl Future<Output = Result<bool, Failure>> + Send {
        ready(Ok(false))
    }
}

impl SecondStepAccounts for Fake {
    type Error = Failure;
    fn find_active_user(&self, account: AccountId) -> impl Future<Output = Result<Option<User>, Failure>> + Send {
        self.log("lookup");
        ready(Ok((!*self.suspended.lock().unwrap()).then(|| user(account))))
    }
    fn find_user_at_epoch(&self, _: AccountId, _: CredentialEpoch) -> impl Future<Output = Result<Option<User>, Failure>> + Send {
        ready(Ok(None))
    }
    fn password_hash(&self, _: AccountId) -> impl Future<Output = Result<Option<String>, Failure>> + Send {
        ready(Ok(self.password_hash.lock().unwrap().clone()))
    }
}

impl SecondFactorThrottle for Fake {
    type Error = &'static str;
    fn check(&self, _: AccountId) -> Result<(), &'static str> {
        self.log("check");
        Ok(())
    }
    fn record_failure(&self, _: AccountId) {
        self.log("failure");
    }
    fn reset(&self, _: AccountId) {
        self.log("reset");
    }
}

/// The claim ports over the fake's recovery codes.
struct Claims<'a>(&'a Fake);

impl SecondFactorClaims for Claims<'_> {
    type Error = Failure;
    fn spend_recovery_code(
        &self,
        _: AccountId,
        digest: &RecoveryDigest,
    ) -> impl Future<Output = Result<CredentialProofWrite, Failure>> + Send {
        let mut codes = self.0.codes.lock().unwrap();
        let mut outcome = CredentialProofWrite::Rejected;
        if let Some(entry) = codes.iter_mut().find(|(stored, used)| stored == digest && !used) {
            entry.1 = true;
            outcome = CredentialProofWrite::Claimed;
        }
        ready(Ok(outcome))
    }
    fn claim_step(&self, _: AccountId, _: &[u8], _: u64) -> impl Future<Output = Result<CredentialProofWrite, Failure>> + Send {
        ready(Ok(CredentialProofWrite::Rejected))
    }
}

impl PasskeyClaims for Claims<'_> {
    type Error = Failure;
    fn use_passkey(&self, _: AccountId, _: i64, _: u32, _: u32) -> impl Future<Output = Result<CredentialProofWrite, Failure>> + Send {
        ready(Ok(CredentialProofWrite::Rejected))
    }
}

fn user(account: AccountId) -> User {
    User {
        account: Account {
            id: account,
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
    PasswordWork::new(BlockingBudget::new(NonZeroUsize::new(4).unwrap()))
}

struct World {
    fake: Fake,
    pending: PendingSignIns,
    challenges: ChallengeBook,
    work: PasswordWork,
}

impl World {
    fn new(fake: Fake) -> Self {
        Self { fake, pending: PendingSignIns::default(), challenges: ChallengeBook::default(), work: work() }
    }
    fn step(&self) -> SecondFactorStep<'_, Fake, Fake, Fake> {
        SecondFactorStep::new(&self.pending, &self.fake, &self.challenges, None, &self.fake, &self.fake, &self.work)
    }
}

#[tokio::test]
async fn two_concurrent_proofs_for_one_pending_step_complete_it_once() {
    let codes = [RecoveryCode::generate(), RecoveryCode::generate()];
    let world = World::new(Fake::with_codes(&codes));
    let token = world.pending.start(AccountId(7), "password");
    let barrier = Arc::new(Barrier::new(2));
    let (first, second) = (codes[0].formatted(), codes[1].formatted());
    let race = |code: String| {
        let barrier = Arc::clone(&barrier);
        let world = &world;
        let token = &token;
        async move {
            let step = world.step();
            step.complete(token, SecondFactorProof::Code(&code), false, |_| Claims(&world.fake), async move {
                barrier.wait().await;
            })
            .await
        }
    };
    let (a, b) = tokio::join!(race(first), race(second));
    let completed = [&a, &b].iter().filter(|result| result.is_ok()).count();
    assert_eq!(completed, 1, "exactly one proof finishes the pending step");
    assert!([&a, &b].iter().any(|result| matches!(result, Err(SecondStepError::NotFound))), "the other finds it claimed");
    assert!(world.pending.account(&token).is_none());
}

#[tokio::test]
async fn a_step_cancelled_after_lookup_is_not_completed_and_its_proof_is_not_counted_twice() {
    let codes = [RecoveryCode::generate()];
    let world = World::new(Fake::with_codes(&codes));
    let token = world.pending.start(AccountId(7), "password");
    let code = codes[0].formatted();
    let result = world
        .step()
        .complete(&token, SecondFactorProof::Code(&code), false, |_| Claims(&world.fake), async {
            world.pending.end(&token);
        })
        .await;
    assert!(matches!(result, Err(SecondStepError::NotFound)));
    assert_eq!(world.fake.calls(), ["check", "lookup"], "no reset and no failure for a cancelled step");
}

#[tokio::test]
async fn wrong_factors_are_counted_and_the_step_ends_after_the_limit() {
    let world = World::new(Fake::default());
    let token = world.pending.start(AccountId(7), "password");
    for attempt in 1..=PENDING_SIGN_IN_ATTEMPTS {
        let result = world.step().complete(&token, SecondFactorProof::Code("000000"), false, |_| Claims(&world.fake), ready(())).await;
        assert!(matches!(result, Err(SecondStepError::WrongFactor { .. })), "attempt {attempt}");
    }
    assert!(world.pending.account(&token).is_none(), "the password must be entered again");
    let calls = world.fake.calls();
    assert_eq!(calls.iter().filter(|call| *call == "failure").count(), usize::from(PENDING_SIGN_IN_ATTEMPTS));
}

#[tokio::test]
async fn suspension_found_by_the_lookup_ends_the_step_and_receipted_entries_are_not_adopted() {
    let world = World::new(Fake::default());
    *world.fake.suspended.lock().unwrap() = true;
    let token = world.pending.start(AccountId(7), "password");
    let result = world.step().complete(&token, SecondFactorProof::Code("000000"), false, |_| Claims(&world.fake), ready(())).await;
    assert!(matches!(result, Err(SecondStepError::NotFound)));
    assert!(world.pending.account(&token).is_none());

    let legacy = world.pending.start(AccountId(7), "password");
    let as_original = world.step().complete(&legacy, SecondFactorProof::Code("000000"), true, |_| Claims(&world.fake), ready(())).await;
    assert!(matches!(as_original, Err(SecondStepError::NotFound)));
    assert_eq!(world.pending.account(&legacy), Some(AccountId(7)), "a mode mismatch leaves the entry alone");
}

#[tokio::test]
async fn completion_resets_the_throttle_and_reports_the_first_step_method() {
    let codes = [RecoveryCode::generate()];
    let world = World::new(Fake::with_codes(&codes));
    let token = world.pending.start(AccountId(7), "GitHub");
    let code = codes[0].formatted();
    let done = world.step().complete(&token, SecondFactorProof::Code(&code), false, |_| Claims(&world.fake), ready(())).await.unwrap();
    assert_eq!(done.method, "GitHub");
    assert_eq!(done.sign_in.user().id(), AccountId(7));
    assert_eq!(world.fake.calls(), ["check", "lookup", "reset"]);
    let again = SessionToken::parse(&token.encode()).unwrap();
    assert!(world.pending.account(&again).is_none(), "completed steps cannot be reused");
}

#[tokio::test]
async fn confirmations_need_the_password_before_any_proof_is_spent() {
    let codes = [RecoveryCode::generate()];
    let world = World::new(Fake::with_codes(&codes));
    let hash = world.work.begin().unwrap().hash("correct horse".into()).await.unwrap();
    *world.fake.password_hash.lock().unwrap() = Some(hash);
    let alice = user(AccountId(7));
    let code = codes[0].formatted();

    let wrong = world.step().confirm(&alice, "wrong".into(), SecondFactorProof::Code(&code), &Claims(&world.fake)).await;
    assert!(matches!(wrong, Err(SecondStepError::WrongFactor { .. })));
    assert!(!world.fake.codes.lock().unwrap()[0].1, "a wrong password leaves the recovery code unspent");
    assert_eq!(world.fake.calls(), ["check", "failure"]);

    world.step().disable_authenticator(&alice, "correct horse".into(), SecondFactorProof::Code(&code), &Claims(&world.fake)).await.unwrap();
    assert_eq!(world.fake.calls(), ["check", "reset", "disable"]);
    let reused = world.step().confirm(&alice, "correct horse".into(), SecondFactorProof::Code(&code), &Claims(&world.fake)).await;
    assert!(matches!(reused, Err(SecondStepError::WrongFactor { .. })), "the recovery code worked once");
}
