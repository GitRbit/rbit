//! Access token application policy against an in-memory store.

use rbit_contracts::{AccountId, Timestamp};
use rbit_identity::{
    AccessTokenSecret, Account, AccountKind, TokenDigest, User,
    access_tokens::{
        AccessTokenApplication, AccessTokenError, AccessTokenId, AccessTokenOwnerRepository, AccessTokenRecord, AccessTokenRejection,
        AccessTokenRepository, EXPIRY_CHOICES, MAX_TOKENS_PER_ACCOUNT, NewAccessTokenRecord, ResolvedAccessToken, TokenAuthentication,
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
    tokens: Vec<(TokenDigest, AccessTokenRecord<u8>)>,
    password_change_required: bool,
    suspended: bool,
    touch_fails: bool,
    calls: Vec<&'static str>,
}

#[derive(Default)]
struct Store(Mutex<State>);

impl Store {
    fn calls(&self) -> Vec<&'static str> {
        self.0.lock().unwrap().calls.clone()
    }
}

impl AccessTokenRepository for Store {
    type Error = Failure;
    type Scope = u8;
    fn create_with_limit(
        &self,
        new: NewAccessTokenRecord<u8>,
        limit: i64,
    ) -> impl Future<Output = Result<Option<AccessTokenRecord<u8>>, Failure>> + Send {
        let mut state = self.0.lock().unwrap();
        state.calls.push("create");
        let owned = state.tokens.iter().filter(|(_, token)| token.account_id == new.account_id).count();
        let created = (i64::try_from(owned).unwrap() < limit).then(|| {
            let token = AccessTokenRecord {
                id: AccessTokenId(i64::try_from(state.tokens.len()).unwrap() + 1),
                account_id: new.account_id,
                name: new.name,
                hint: new.hint,
                scope: new.scope,
                created_at: Timestamp::now(),
                last_used_at: None,
                expires_at: new.expires_at,
            };
            state.tokens.push((new.digest, token.clone()));
            token
        });
        ready(Ok(created))
    }
    fn list_for_account(&self, account: AccountId) -> impl Future<Output = Result<Vec<AccessTokenRecord<u8>>, Failure>> + Send {
        let mut state = self.0.lock().unwrap();
        state.calls.push("list");
        ready(Ok(state.tokens.iter().filter(|(_, token)| token.account_id == account).map(|(_, token)| token.clone()).collect()))
    }
    fn delete(&self, account: AccountId, id: AccessTokenId) -> impl Future<Output = Result<bool, Failure>> + Send {
        let mut state = self.0.lock().unwrap();
        state.calls.push("delete");
        let before = state.tokens.len();
        state.tokens.retain(|(_, token)| !(token.id == id && token.account_id == account));
        ready(Ok(state.tokens.len() < before))
    }
    fn find_by_digest(&self, digest: TokenDigest) -> impl Future<Output = Result<Option<ResolvedAccessToken<u8>>, Failure>> + Send {
        let mut state = self.0.lock().unwrap();
        state.calls.push("find");
        ready(Ok(state.tokens.iter().find(|(stored, _)| *stored == digest).map(|(_, token)| (token.clone(), user(token.account_id.0)))))
    }
    fn touch(&self, id: AccessTokenId, now: Timestamp) -> impl Future<Output = Result<(), Failure>> + Send {
        let mut state = self.0.lock().unwrap();
        state.calls.push("touch");
        if state.touch_fails {
            return ready(Err(Failure));
        }
        if let Some((_, token)) = state.tokens.iter_mut().find(|(_, token)| token.id == id) {
            token.last_used_at = Some(now);
        }
        ready(Ok(()))
    }
}

impl AccessTokenOwnerRepository for Store {
    type Error = Failure;
    fn password_change_required(&self, _: AccountId) -> impl Future<Output = Result<bool, Failure>> + Send {
        let mut state = self.0.lock().unwrap();
        state.calls.push("password_change_required");
        ready(Ok(state.password_change_required))
    }
    fn is_suspended(&self, _: AccountId) -> impl Future<Output = Result<bool, Failure>> + Send {
        let mut state = self.0.lock().unwrap();
        state.calls.push("is_suspended");
        ready(Ok(state.suspended))
    }
}

fn user(id: i64) -> User {
    User {
        account: Account {
            id: AccountId(id),
            kind: AccountKind::User,
            name: format!("user{id}").parse().unwrap(),
            display_name: None,
            bio: None,
            created_at: Timestamp(0),
        },
        email: format!("user{id}@example.com").parse().unwrap(),
        is_admin: false,
    }
}

fn rejection<T: fmt::Debug>(result: Result<T, AccessTokenError<Failure, Failure>>) -> AccessTokenRejection {
    match result {
        Err(AccessTokenError::Rejected(rejection)) => rejection,
        other => panic!("expected a rejection, got {other:?}"),
    }
}

#[tokio::test]
async fn create_stores_only_the_digest_and_hint_of_a_prefixed_secret() {
    let store = Store::default();
    let app = AccessTokenApplication::new(&store, &store);
    let (token, secret) = app.create(&user(7), "  laptop  ", 2, Some(30)).await.unwrap();
    assert!(secret.starts_with(AccessTokenSecret::PREFIX) && secret.ends_with(&token.hint));
    assert_eq!((token.name.as_str(), token.scope, token.account_id), ("laptop", 2, AccountId(7)));
    let expected = Timestamp::now().seconds() + 30 * 86_400;
    assert!((expected - 5..=expected).contains(&token.expires_at.unwrap().seconds()));
    let digest = AccessTokenSecret::parse(&secret).unwrap().digest();
    assert_eq!(store.0.lock().unwrap().tokens[0].0, digest);
    assert_eq!(store.calls(), ["create"]);
}

#[tokio::test]
async fn invalid_requests_are_rejected_before_storage_with_the_legacy_text() {
    let store = Store::default();
    let app = AccessTokenApplication::new(&store, &store);
    let empty = rejection(app.create(&user(7), "   ", 0, None).await);
    assert_eq!(empty.to_string(), "token name must not be empty");
    let long = rejection(app.create(&user(7), &"x".repeat(65), 0, None).await);
    assert_eq!(long.to_string(), "token name must be at most 64 characters");
    let odd = rejection(app.create(&user(7), "odd", 0, Some(8)).await);
    assert_eq!(odd.to_string(), "choose one of the offered expiration periods");
    assert_eq!(store.calls(), Vec::<&str>::new(), "rejected before storage");
    assert!(app.create(&user(7), &"é".repeat(64), 0, None).await.is_ok());
    for &days in EXPIRY_CHOICES {
        let (token, _) = app.create(&user(7), "choice", 0, days).await.unwrap();
        assert_eq!(token.expires_at.is_none(), days.is_none());
    }
}

#[tokio::test]
async fn the_per_account_limit_is_enforced_by_the_store_and_reported() {
    let store = Store::default();
    let app = AccessTokenApplication::new(&store, &store);
    for n in 0..MAX_TOKENS_PER_ACCOUNT {
        app.create(&user(7), &format!("token {n}"), 0, None).await.unwrap();
    }
    let over = rejection(app.create(&user(7), "one too many", 0, None).await);
    assert_eq!(over.to_string(), "you can have at most 50 access tokens; revoke one you no longer use");
    assert!(app.create(&user(8), "unaffected", 0, None).await.is_ok());
}

#[tokio::test]
async fn revoke_is_owner_scoped_and_returns_the_name_for_audit() {
    let store = Store::default();
    let app = AccessTokenApplication::new(&store, &store);
    let (token, _) = app.create(&user(7), "laptop", 0, None).await.unwrap();
    assert!(matches!(app.revoke(&user(8), token.id).await, Err(AccessTokenError::NotFound)));
    assert_eq!(app.revoke(&user(7), token.id).await.unwrap().as_deref(), Some("laptop"));
    assert!(matches!(app.revoke(&user(7), token.id).await, Err(AccessTokenError::NotFound)));
    assert!(app.list(&user(7)).await.unwrap().is_empty());
}

#[tokio::test]
async fn authentication_accepts_both_prefixes_and_records_use() {
    let store = Store::default();
    let app = AccessTokenApplication::new(&store, &store);
    let (_, secret) = app.create(&user(7), "ci", 1, Some(7)).await.unwrap();
    let legacy = secret.replacen(AccessTokenSecret::PREFIX, AccessTokenSecret::LEGACY_PREFIX, 1);
    for presented in [&secret, &legacy] {
        let TokenAuthentication::Accepted { user, scope, bookkeeping, .. } = app.authenticate(presented).await.unwrap() else {
            panic!("a live token authenticates");
        };
        assert_eq!((user.id(), scope), (AccountId(7), 1));
        assert!(bookkeeping.is_ok());
    }
    assert!(app.list(&user(7)).await.unwrap()[0].last_used_at.is_some());
}

#[tokio::test]
async fn refusals_follow_the_fixed_check_order() {
    let store = Store::default();
    let app = AccessTokenApplication::new(&store, &store);
    for malformed in ["", "hunter2", "rbit_", "rit_tooshort", &AccessTokenSecret::generate().encode()] {
        assert!(matches!(app.authenticate(malformed).await.unwrap(), TokenAuthentication::Refused), "{malformed:?}");
    }
    let (_, secret) = app.create(&user(7), "ci", 1, None).await.unwrap();
    store.0.lock().unwrap().calls.clear();

    store.0.lock().unwrap().password_change_required = true;
    store.0.lock().unwrap().suspended = true;
    assert!(matches!(app.authenticate(&secret).await.unwrap(), TokenAuthentication::Refused));
    assert_eq!(store.calls(), ["find", "password_change_required"]);

    store.0.lock().unwrap().password_change_required = false;
    store.0.lock().unwrap().calls.clear();
    let outcome = app.authenticate(&secret).await.unwrap();
    assert!(matches!(outcome, TokenAuthentication::SuspendedOwner { user } if user.id() == AccountId(7)));
    assert_eq!(store.calls(), ["find", "password_change_required", "is_suspended"], "no use is recorded");
}

#[tokio::test]
async fn expired_tokens_are_refused_and_bookkeeping_failure_still_accepts() {
    let store = Store::default();
    let app = AccessTokenApplication::new(&store, &store);
    let (_, secret) = app.create(&user(7), "ci", 1, None).await.unwrap();
    store.0.lock().unwrap().touch_fails = true;
    let TokenAuthentication::Accepted { bookkeeping, .. } = app.authenticate(&secret).await.unwrap() else {
        panic!("a failed last-use write must not refuse the credential");
    };
    assert!(bookkeeping.is_err());

    store.0.lock().unwrap().tokens[0].1.expires_at = Some(Timestamp(Timestamp::now().seconds() - 1));
    store.0.lock().unwrap().calls.clear();
    assert!(matches!(app.authenticate(&secret).await.unwrap(), TokenAuthentication::Refused));
    assert_eq!(store.calls(), ["find"]);
}
