//! Passkey registration policy and assertion guards against an in-memory store. The
//! cryptographic ceremonies are covered by the `webauthn` unit tests and the service suites.

use rbit_contracts::{AccountId, Timestamp};
use rbit_identity::{
    Account, AccountKind, User,
    passkeys::{
        ChallengeBook, MAX_PASSKEYS, MAX_RESPONSE_LEN, NewPasskey, PasskeyAdded, PasskeyApplication, PasskeyClaims, PasskeyError,
        PasskeyRecord, PasskeyRepository,
        webauthn::{Challenge, RelyingParty},
    },
    two_factor::{CredentialProofWrite, RecoveryDigest},
};
use std::{
    fmt,
    future::{Future, ready},
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
struct Store(Vec<PasskeyRecord>);

impl PasskeyRepository for Store {
    type Error = Failure;
    fn passkeys(&self, _: AccountId) -> impl Future<Output = Result<Vec<PasskeyRecord>, Failure>> + Send {
        ready(Ok(self.0.clone()))
    }
    fn find_passkey(&self, _: AccountId, credential_id: &[u8]) -> impl Future<Output = Result<Option<PasskeyRecord>, Failure>> + Send {
        ready(Ok(self.0.iter().find(|passkey| passkey.credential_id == credential_id).cloned()))
    }
    fn add_passkey(
        &self,
        _: AccountId,
        _: &NewPasskey<'_>,
        _: &[RecoveryDigest],
    ) -> impl Future<Output = Result<PasskeyAdded, Failure>> + Send {
        ready(Ok(PasskeyAdded::Added))
    }
    fn remove_passkey(&self, _: AccountId, id: i64) -> impl Future<Output = Result<bool, Failure>> + Send {
        ready(Ok(self.0.iter().any(|passkey| passkey.id == id)))
    }
}

struct NoClaims;
impl PasskeyClaims for NoClaims {
    type Error = &'static str;
    fn use_passkey(&self, _: AccountId, _: i64, _: u32, _: u32) -> impl Future<Output = Result<CredentialProofWrite, &'static str>> + Send {
        ready(Err("must not be reached"))
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

fn passkey(id: i64) -> PasskeyRecord {
    PasskeyRecord {
        id,
        credential_id: vec![u8::try_from(id).unwrap()],
        public_key: Vec::new(),
        algorithm: -7,
        sign_count: 0,
        transports: Vec::new(),
        name: "key".into(),
        created_at: Timestamp(0),
        last_used_at: None,
    }
}

fn rejected<T: fmt::Debug, C: fmt::Debug>(result: Result<T, PasskeyError<Failure, C>>) -> String {
    match result {
        Err(PasskeyError::Rejected(rejection)) => rejection.to_string(),
        other => panic!("expected a rejection, got {other:?}"),
    }
}

fn rp() -> Option<RelyingParty> {
    RelyingParty::from_public_url("https://git.example.com", "rbit")
}

#[tokio::test]
async fn passkeys_need_a_public_url() {
    let (store, book) = (Store::default(), ChallengeBook::default());
    let app = PasskeyApplication::new(&store, &book, None);
    assert_eq!(rejected(app.creation_options(&user()).await), "passkeys need `server.public_url` to be set to this site's address");
    let with_url = PasskeyApplication::new(&store, &book, rp());
    assert!(with_url.creation_options(&user()).await.unwrap().contains("git.example.com"));
}

#[tokio::test]
async fn registration_checks_name_then_challenge_before_anything_else() {
    let (store, book) = (Store::default(), ChallengeBook::default());
    let app = PasskeyApplication::new(&store, &book, rp());
    assert_eq!(rejected(app.register(&user(), &"x".repeat(65), "{}").await), "the name can be at most 64 characters");
    assert_eq!(rejected(app.register(&user(), "a\u{7}", "{}").await), "the name can't contain control characters");
    assert_eq!(rejected(app.register(&user(), "laptop", "{}").await), "the passkey request expired; try again");
}

#[tokio::test]
async fn confirmation_options_exist_only_with_passkeys_and_limit_to_them() {
    let book = ChallengeBook::default();
    let empty = Store::default();
    assert!(PasskeyApplication::new(&empty, &book, rp()).confirmation_options(&user()).await.unwrap().is_none());
    let full = Store((1..=i64::try_from(MAX_PASSKEYS).unwrap()).map(passkey).collect());
    let options = PasskeyApplication::new(&full, &book, rp()).confirmation_options(&user()).await.unwrap().unwrap();
    assert!(options.contains("allowCredentials"));
}

#[tokio::test]
async fn oversized_or_malformed_assertions_never_reach_the_claim() {
    let (store, book) = (Store(vec![passkey(1)]), ChallengeBook::default());
    let app = PasskeyApplication::new(&store, &book, rp());
    let challenge = Challenge::generate();
    let huge = "x".repeat(MAX_RESPONSE_LEN + 1);
    assert!(matches!(app.verify_assertion(&user(), &challenge, &huge, &NoClaims).await, Err(PasskeyError::InvalidCredentials)));
    assert!(matches!(app.verify_assertion(&user(), &challenge, "{}", &NoClaims).await, Err(PasskeyError::AssertionRefused(_))));
}
