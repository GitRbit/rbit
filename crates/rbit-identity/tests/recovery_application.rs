//! Recovery application policy against an in-memory link store and real password work.

use rbit_contracts::{AccountId, Timestamp};
use rbit_identity::{
    Account, AccountKind, Email, PasswordWork, SessionToken, TokenDigest, User,
    recovery::{
        EmailTokenRecord, NewEmailToken, RESET_LIFETIME_SECONDS, RecoveryAccount, RecoveryCredentialRepository, RecoveryError,
        RecoveryLinks, RecoveryPasswords, RecoveryRepository, RecoveryThrottle, ResetRequest, TokenPurpose, commit_reset,
    },
};
use rbit_kernel::BlockingBudget;
use std::{
    collections::HashSet,
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

struct Link {
    token: NewEmailToken,
    used: bool,
}

#[derive(Default)]
struct State {
    links: Vec<Link>,
    accounts: Vec<RecoveryAccount>,
    hash: Option<String>,
    initial: bool,
    calls: Vec<String>,
}

#[derive(Default)]
struct Store(Mutex<State>);

impl Store {
    fn with_account(suspended: bool) -> Self {
        let account = RecoveryAccount {
            id: AccountId(7),
            name: "alice".parse().unwrap(),
            email: "alice@example.com".parse().unwrap(),
            suspended,
            email_verified_at: None,
        };
        Self(Mutex::new(State { accounts: vec![account], ..State::default() }))
    }
    fn log(&self, call: &str) {
        self.0.lock().unwrap().calls.push(call.to_owned());
    }
    fn calls(&self) -> Vec<String> {
        std::mem::take(&mut self.0.lock().unwrap().calls)
    }
    fn honoured(&self, digest: TokenDigest, purpose: TokenPurpose, now: Timestamp, consume: bool) -> Option<EmailTokenRecord> {
        let mut state = self.0.lock().unwrap();
        let current = state.accounts.first().map(|account| account.email.clone());
        let link = state.links.iter_mut().find(|link| {
            link.token.digest == digest
                && link.token.purpose == purpose
                && !link.used
                && link.token.expires_at > now
                && Some(&link.token.email) == current.as_ref()
        })?;
        link.used |= consume;
        Some(EmailTokenRecord { account_id: link.token.account_id, email: link.token.email.clone(), expires_at: link.token.expires_at })
    }
}

impl RecoveryRepository for Store {
    type Error = Failure;
    fn insert(&self, token: &NewEmailToken) -> impl Future<Output = Result<(), Failure>> + Send {
        self.log("insert");
        self.0.lock().unwrap().links.push(Link { token: token.clone(), used: false });
        ready(Ok(()))
    }
    fn find_valid(
        &self,
        digest: TokenDigest,
        purpose: TokenPurpose,
        now: Timestamp,
    ) -> impl Future<Output = Result<Option<EmailTokenRecord>, Failure>> + Send {
        self.log("find_valid");
        ready(Ok(self.honoured(digest, purpose, now, false)))
    }
    fn consume(
        &self,
        digest: TokenDigest,
        purpose: TokenPurpose,
        now: Timestamp,
    ) -> impl Future<Output = Result<Option<EmailTokenRecord>, Failure>> + Send {
        self.log("consume");
        ready(Ok(self.honoured(digest, purpose, now, true)))
    }
    fn revoke_all(&self, account: AccountId, purpose: TokenPurpose) -> impl Future<Output = Result<u64, Failure>> + Send {
        self.log("revoke_all");
        let mut state = self.0.lock().unwrap();
        let mut count = 0;
        for link in state.links.iter_mut().filter(|link| link.token.account_id == account && link.token.purpose == purpose && !link.used) {
            link.used = true;
            count += 1;
        }
        ready(Ok(count))
    }
    fn find_account_by_email(&self, email: &Email) -> impl Future<Output = Result<Option<RecoveryAccount>, Failure>> + Send {
        self.log("find_account_by_email");
        let state = self.0.lock().unwrap();
        ready(Ok(state.accounts.iter().find(|account| account.email.as_str().eq_ignore_ascii_case(email.as_str())).cloned()))
    }
    fn find_account(&self, account: AccountId) -> impl Future<Output = Result<Option<RecoveryAccount>, Failure>> + Send {
        ready(Ok(self.0.lock().unwrap().accounts.iter().find(|candidate| candidate.id == account).cloned()))
    }
    fn mark_verified(&self, _: AccountId, email: &Email, at: Timestamp) -> impl Future<Output = Result<bool, Failure>> + Send {
        let mut state = self.0.lock().unwrap();
        let mut marked = false;
        if let Some(account) = state.accounts.first_mut().filter(|account| &account.email == email) {
            account.email_verified_at = Some(at);
            marked = true;
        }
        ready(Ok(marked))
    }
    fn verified_among(&self, _: &[AccountId]) -> impl Future<Output = Result<HashSet<AccountId>, Failure>> + Send {
        ready(Ok(HashSet::new()))
    }
    fn reset_password(
        &self,
        digest: TokenDigest,
        hash: &str,
        now: Timestamp,
    ) -> impl Future<Output = Result<Option<EmailTokenRecord>, Failure>> + Send {
        let record = self.honoured(digest, TokenPurpose::Reset, now, true);
        if record.is_some() {
            self.0.lock().unwrap().hash = Some(hash.to_owned());
        }
        ready(Ok(record))
    }
}

impl RecoveryCredentialRepository for Store {
    type Error = Failure;
    fn password_change_required(&self, _: AccountId) -> impl Future<Output = Result<bool, Failure>> + Send {
        ready(Ok(self.0.lock().unwrap().initial))
    }
    fn password_hash(&self, _: AccountId) -> impl Future<Output = Result<Option<String>, Failure>> + Send {
        ready(Ok(self.0.lock().unwrap().hash.clone()))
    }
    fn set_password_hash(&self, _: AccountId, hash: &str) -> impl Future<Output = Result<(), Failure>> + Send {
        self.0.lock().unwrap().hash = Some(hash.to_owned());
        ready(Ok(()))
    }
}

impl RecoveryThrottle for Store {
    type Error = &'static str;
    fn check_client(&self, client: &str) -> Result<(), &'static str> {
        self.log(&format!("client {client}"));
        Ok(())
    }
    fn check_recipient(&self, key: &str) -> Result<(), &'static str> {
        self.log(&format!("recipient {key}"));
        Ok(())
    }
}

fn admin() -> User {
    User {
        account: Account {
            id: AccountId(1),
            kind: AccountKind::User,
            name: "root".parse().unwrap(),
            display_name: None,
            bio: None,
            created_at: Timestamp(0),
        },
        email: "root@example.com".parse().unwrap(),
        is_admin: true,
    }
}

fn work() -> PasswordWork {
    PasswordWork::new(BlockingBudget::new(NonZeroUsize::new(2).unwrap()))
}

fn rejection<T: fmt::Debug, E: fmt::Debug, V: fmt::Debug>(result: Result<T, RecoveryError<Failure, E, V>>) -> String {
    match result {
        Err(RecoveryError::Rejected(rejection)) => rejection.to_string(),
        other => panic!("expected a rejection, got {other:?}"),
    }
}

#[tokio::test]
async fn reset_requests_throttle_client_then_recipient_and_never_reveal_accounts() {
    let store = Store::with_account(false);
    let links = RecoveryLinks::new(&store, &store, true);
    assert_eq!(rejection(links.request_reset("not an address", "203.0.113.7").await), "enter a valid email address");
    assert_eq!(store.calls(), ["client 203.0.113.7"]);
    let request = links.request_reset(" Alice@Example.com ", "203.0.113.7").await.unwrap();
    assert!(matches!(request, ResetRequest::Send(account) if account.id == AccountId(7)));
    assert_eq!(store.calls(), ["client 203.0.113.7", "recipient alice@example.com", "find_account_by_email"]);
    assert_eq!(links.request_reset("nobody@example.com", "x").await.unwrap(), ResetRequest::Unknown);

    let suspended = Store::with_account(true);
    let links = RecoveryLinks::new(&suspended, &suspended, true);
    assert_eq!(links.request_reset("alice@example.com", "x").await.unwrap(), ResetRequest::Suspended);
    let unavailable = RecoveryLinks::new(&suspended, &suspended, false);
    assert_eq!(rejection(unavailable.request_reset("alice@example.com", "x").await), "email is not available on this server");
}

#[tokio::test]
async fn issued_links_store_only_a_digest_bound_to_purpose_address_and_expiry() {
    let store = Store::with_account(false);
    let links = RecoveryLinks::new(&store, &store, true);
    let account = store.0.lock().unwrap().accounts[0].clone();
    let secret = links.issue(&account, TokenPurpose::Reset).await.unwrap();
    let state = store.0.lock().unwrap();
    let stored = &state.links[0].token;
    assert_eq!(stored.digest, SessionToken::parse(&secret).unwrap().digest());
    assert_eq!((stored.purpose, &stored.email), (TokenPurpose::Reset, &account.email));
    let expected = Timestamp::now().seconds() + RESET_LIFETIME_SECONDS;
    assert!((expected - 5..=expected).contains(&stored.expires_at.seconds()));
}

#[tokio::test]
async fn verification_links_work_once() {
    let store = Store::with_account(false);
    let links = RecoveryLinks::new(&store, &store, true);
    let account = store.0.lock().unwrap().accounts[0].clone();
    let secret = links.issue(&account, TokenPurpose::Verify).await.unwrap();
    let other = links.issue(&account, TokenPurpose::Verify).await.unwrap();
    assert!(matches!(links.verify_email("malformed").await, Err(RecoveryError::NotFound)));
    let record = links.verify_email(&secret).await.unwrap();
    assert_eq!(record.account_id, AccountId(7));
    assert!(matches!(links.verify_email(&secret).await, Err(RecoveryError::NotFound)), "used up");
    assert!(matches!(links.verify_email(&other).await, Err(RecoveryError::NotFound)), "siblings are retired");
    assert_eq!(rejection(links.resend_target(&account_user()).await), "your email address is already verified");
}

fn account_user() -> User {
    User {
        account: Account { id: AccountId(7), name: "alice".parse().unwrap(), ..admin().account },
        email: "alice@example.com".parse().unwrap(),
        is_admin: false,
    }
}

#[tokio::test]
async fn resets_are_single_use_and_invalid_passwords_leave_the_link_usable() {
    let work = work();
    let store = Store::with_account(false);
    let links = RecoveryLinks::new(&store, &store, true);
    let account = store.0.lock().unwrap().accounts[0].clone();
    let secret = links.issue(&account, TokenPurpose::Reset).await.unwrap();
    let passwords = RecoveryPasswords::new(&store, &store, &work, 8);

    let short = passwords.prepare_reset(&secret, "short".into(), || Ok::<_, ()>(())).await;
    assert_eq!(rejection(short), "password must be at least 8 characters");
    assert_eq!(links.reset_account(&secret).await.unwrap().id, AccountId(7), "still usable");

    let closed = passwords.prepare_reset(&secret, "battery staple".into(), || Err::<(), _>("closed")).await;
    assert!(matches!(closed, Err(RecoveryError::Reserve("closed"))));

    let first = passwords.prepare_reset(&secret, "battery staple".into(), || Ok::<_, ()>(1)).await.unwrap();
    let second = passwords.prepare_reset(&secret, "another staple".into(), || Ok::<_, ()>(2)).await.unwrap();
    commit_reset(&store, first.digest, &first.hash).await.unwrap();
    assert!(matches!(commit_reset(&store, second.digest, &second.hash).await, Err(RecoveryError::NotFound)), "one redemption wins");
    assert!(matches!(links.reset_account(&secret).await, Err(RecoveryError::NotFound)));
    let stored = store.0.lock().unwrap().hash.clone().unwrap();
    assert!(work.begin().unwrap().verify("battery staple".into(), stored).await.unwrap());
}

#[tokio::test]
async fn initial_passwords_must_change_and_suspension_forbids_reset() {
    let work = work();
    let store = Store::with_account(false);
    let links = RecoveryLinks::new(&store, &store, true);
    let account = store.0.lock().unwrap().accounts[0].clone();
    let initial = work.begin().unwrap().hash("given by admin".into()).await.unwrap();
    {
        let mut state = store.0.lock().unwrap();
        state.hash = Some(initial);
        state.initial = true;
    }
    let secret = links.issue(&account, TokenPurpose::Reset).await.unwrap();
    let passwords = RecoveryPasswords::new(&store, &store, &work, 8);
    let same = passwords.prepare_reset(&secret, "given by admin".into(), || Ok::<_, ()>(())).await;
    assert_eq!(rejection(same), "choose a different password from your initial password");

    store.0.lock().unwrap().accounts[0].suspended = true;
    assert!(matches!(links.reset_account(&secret).await, Err(RecoveryError::Forbidden)));
    let suspended = links.admin_reset_target(&admin(), AccountId(7)).await;
    assert_eq!(rejection(suspended), "alice is suspended; lift the suspension first");
}

#[tokio::test]
async fn administrators_cannot_set_their_own_password_and_others_cannot_set_any() {
    let work = work();
    let store = Store::with_account(false);
    let alice = store.0.lock().unwrap().accounts[0].clone();
    store.0.lock().unwrap().accounts.push(RecoveryAccount { id: AccountId(1), name: "root".parse().unwrap(), ..alice });
    let passwords = RecoveryPasswords::new(&store, &store, &work, 8);
    let own = passwords.prepare_admin_password(&admin(), AccountId(1), "battery staple".into(), || Ok::<_, ()>(())).await;
    assert_eq!(rejection(own), "change your own password from your account settings");
    let user = account_user();
    let forbidden = passwords.prepare_admin_password(&user, AccountId(1), "battery staple".into(), || Ok::<_, ()>(())).await;
    assert!(matches!(forbidden, Err(RecoveryError::Forbidden)));
    let prepared = passwords.prepare_admin_password(&admin(), AccountId(7), "battery staple".into(), || Ok::<_, ()>(())).await.unwrap();
    assert_eq!(prepared.account.id, AccountId(7));
}
