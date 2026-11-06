use rbit_contracts::{AccountId, Timestamp};
use rbit_identity::{
    Account, AccountKind, TokenDigest, User,
    sessions::{SessionClock, SessionPolicy, SessionRecord, SessionRepository, SessionUser, SessionUserRepository, StoredSession},
};
use std::{
    collections::VecDeque,
    convert::Infallible,
    future::{Future, ready},
    sync::Mutex,
};

#[derive(Default)]
pub struct State {
    pub record: Option<SessionRecord>,
    pub user: Option<SessionUser>,
    pub listed: Vec<StoredSession>,
    pub calls: Vec<String>,
    pub renewed: Option<(Timestamp, Timestamp)>,
    pub renewal_survives: bool,
    pub kept: Option<TokenDigest>,
}
#[derive(Default)]
pub struct Store(pub Mutex<State>);
pub struct Clock(pub Mutex<VecDeque<Timestamp>>);
impl Clock {
    pub fn at(seconds: &[i64]) -> Self {
        Self(Mutex::new(seconds.iter().copied().map(Timestamp).collect()))
    }
}
impl SessionClock for Clock {
    fn now(&self) -> Timestamp {
        self.0.lock().unwrap().pop_front().expect("unexpected clock sample")
    }
}
pub fn user() -> User {
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
pub fn record() -> SessionRecord {
    SessionRecord {
        account_id: AccountId(7),
        expires_at: Timestamp(200_000),
        created_at: Timestamp(0),
        last_seen_at: Timestamp(1_000),
        ip_address: None,
        user_agent: "browser".into(),
    }
}
pub fn policy() -> SessionPolicy {
    SessionPolicy { lifetime_days: 2, max_age_days: 0, notifications: true }
}
pub fn store() -> Store {
    Store(Mutex::new(State {
        record: Some(record()),
        user: Some(SessionUser { user: user(), password_change_required: true, has_unread_notifications: true }),
        renewal_survives: true,
        ..State::default()
    }))
}
impl SessionRepository for Store {
    type Error = Infallible;
    fn put(&self, _: TokenDigest, record: SessionRecord) -> impl Future<Output = Result<(), Self::Error>> + Send {
        let mut s = self.0.lock().unwrap();
        s.calls.push("put".into());
        s.record = Some(record);
        ready(Ok(()))
    }
    fn get_live(&self, _: TokenDigest) -> impl Future<Output = Result<Option<SessionRecord>, Self::Error>> + Send {
        let mut s = self.0.lock().unwrap();
        s.calls.push("get".into());
        ready(Ok(s.record.clone()))
    }
    fn renew_live(&self, _: TokenDigest, expiry: Timestamp, seen: Timestamp) -> impl Future<Output = Result<bool, Self::Error>> + Send {
        let mut s = self.0.lock().unwrap();
        s.calls.push("renew".into());
        s.renewed = Some((expiry, seen));
        ready(Ok(s.renewal_survives))
    }
    fn delete(&self, _: TokenDigest) -> impl Future<Output = Result<(), Self::Error>> + Send {
        let mut s = self.0.lock().unwrap();
        s.calls.push("delete".into());
        s.record = None;
        ready(Ok(()))
    }
    fn list_for_account(&self, account: AccountId) -> impl Future<Output = Result<Vec<StoredSession>, Self::Error>> + Send {
        let mut s = self.0.lock().unwrap();
        s.calls.push(format!("list:{}", account.0));
        ready(Ok(s.listed.clone()))
    }
    fn delete_for_account(&self, account: AccountId, keep: Option<TokenDigest>) -> impl Future<Output = Result<usize, Self::Error>> + Send {
        let mut s = self.0.lock().unwrap();
        s.calls.push(format!("delete-account:{}", account.0));
        s.kept = keep;
        ready(Ok(3))
    }
}
impl SessionUserRepository for Store {
    type Error = Infallible;
    fn find_session_user(
        &self,
        account: AccountId,
        notifications: bool,
    ) -> impl Future<Output = Result<Option<SessionUser>, Self::Error>> + Send {
        let mut s = self.0.lock().unwrap();
        s.calls.push(format!("user:{}:{notifications}", account.0));
        ready(Ok(s.user.clone()))
    }
}
