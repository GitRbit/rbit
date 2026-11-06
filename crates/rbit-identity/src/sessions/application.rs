//! Session application policy, independent of config, SQL, redb and protocol responses.

use super::{ActiveSession, ClientInfo, SessionRecord, SessionRepository, SessionUser, SessionUserRepository, session_id};
use crate::{SessionToken, User};
use rbit_contracts::{AccountId, Timestamp};

const DAY: i64 = 24 * 60 * 60;
const LAST_SEEN_RESOLUTION: i64 = 5 * 60;

#[derive(Debug, Clone, Copy)]
pub struct SessionPolicy {
    pub lifetime_days: u32,
    pub max_age_days: u32,
    pub notifications: bool,
}

impl SessionPolicy {
    pub fn lifetime_seconds(self) -> i64 {
        i64::from(self.lifetime_days) * DAY
    }
    pub fn maximum_end(self, created: Timestamp) -> Option<Timestamp> {
        (self.max_age_days > 0).then(|| Timestamp(created.seconds() + i64::from(self.max_age_days) * DAY))
    }
    fn expiry(self, created: Timestamp, now: Timestamp) -> Timestamp {
        let idle = Timestamp(now.seconds() + self.lifetime_seconds());
        self.maximum_end(created).map_or(idle, |end| idle.min(end))
    }
}

pub trait SessionClock: Sync {
    fn now(&self) -> Timestamp;
}
#[derive(Debug, Clone, Copy)]
pub struct SystemSessionClock;
impl SessionClock for SystemSessionClock {
    fn now(&self) -> Timestamp {
        Timestamp::now()
    }
}

#[derive(Debug)]
pub enum SessionError<Session, Account> {
    Session(Session),
    Account(Account),
    NotFound,
}

pub type SessionResult<T, Session, Account> = Result<T, SessionError<Session, Account>>;

#[derive(Debug)]
pub struct SessionApplication<'a, S, A, C = SystemSessionClock> {
    sessions: &'a S,
    accounts: &'a A,
    policy: SessionPolicy,
    clock: C,
}

impl<'a, S: SessionRepository, A: SessionUserRepository> SessionApplication<'a, S, A> {
    pub fn new(sessions: &'a S, accounts: &'a A, policy: SessionPolicy) -> Self {
        Self::with_clock(sessions, accounts, policy, SystemSessionClock)
    }
}

impl<'a, S: SessionRepository, A: SessionUserRepository, C: SessionClock> SessionApplication<'a, S, A, C> {
    pub fn with_clock(sessions: &'a S, accounts: &'a A, policy: SessionPolicy, clock: C) -> Self {
        Self { sessions, accounts, policy, clock }
    }

    pub async fn start(&self, user: &User, client: &ClientInfo) -> SessionResult<SessionToken, S::Error, A::Error> {
        let token = SessionToken::generate();
        let ip = client.ip.map(|ip| ip.to_canonical().to_string());
        // Preserve legacy sample order rather than silently altering boundary semantics.
        let created_for_cap = self.clock.now();
        let expiry = self.policy.expiry(created_for_cap, self.clock.now());
        let record = SessionRecord::at(user.id(), expiry, ip, &client.user_agent, self.clock.now());
        self.sessions.put(token.digest(), record).await.map_err(SessionError::Session)?;
        Ok(token)
    }

    pub async fn resume(&self, token: &SessionToken) -> SessionResult<Option<SessionUser>, S::Error, A::Error> {
        let digest = token.digest();
        let Some(record) = self.sessions.get_live(digest).await.map_err(SessionError::Session)? else { return Ok(None) };
        let now = self.clock.now();
        if self.policy.maximum_end(record.created_at).is_some_and(|end| end <= now) {
            self.sessions.delete(digest).await.map_err(SessionError::Session)?;
            return Ok(None);
        }
        let Some(user) =
            self.accounts.find_session_user(record.account_id, self.policy.notifications).await.map_err(SessionError::Account)?
        else {
            self.sessions.delete(digest).await.map_err(SessionError::Session)?;
            return Ok(None);
        };
        let renewed = self.policy.expiry(record.created_at, self.clock.now());
        let renew = renewed.seconds() - record.expires_at.seconds() > DAY;
        let seen = now.seconds() - record.last_seen_at.seconds() >= LAST_SEEN_RESOLUTION;
        if (renew || seen) && !self.sessions.renew_live(digest, renewed, now).await.map_err(SessionError::Session)? {
            return Ok(None);
        }
        Ok(Some(user))
    }

    pub async fn end(&self, token: &SessionToken) -> SessionResult<(), S::Error, A::Error> {
        self.sessions.delete(token.digest()).await.map_err(SessionError::Session)
    }

    pub async fn list(&self, user: &User, current: Option<&SessionToken>) -> SessionResult<Vec<ActiveSession>, S::Error, A::Error> {
        let current = current.map(SessionToken::digest);
        let stored = self.sessions.list_for_account(user.id()).await.map_err(SessionError::Session)?;
        Ok(stored.into_iter().map(|stored| ActiveSession::from_stored(stored, current.as_ref())).collect())
    }

    /// Returns the removed metadata only after its deletion, for the infrastructure audit.
    pub async fn revoke(&self, user: &User, public_id: &str) -> SessionResult<SessionRecord, S::Error, A::Error> {
        let sessions = self.sessions.list_for_account(user.id()).await.map_err(SessionError::Session)?;
        let stored = sessions.into_iter().find(|stored| session_id(&stored.digest) == public_id).ok_or(SessionError::NotFound)?;
        self.sessions.delete(stored.digest).await.map_err(SessionError::Session)?;
        Ok(stored.record)
    }

    pub async fn revoke_others(&self, account: AccountId, current: &SessionToken) -> SessionResult<usize, S::Error, A::Error> {
        self.sessions.delete_for_account(account, Some(current.digest())).await.map_err(SessionError::Session)
    }
}
