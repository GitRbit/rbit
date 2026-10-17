//! Sign-up, sign-in and session management.
//!
//! Suspended users are refused at every door this service guards: sign-in returns
//! [`ServiceError::Forbidden`] (only after the password has been verified, so suspension
//! is never revealed to someone who doesn't know the password) and sessions stop resolving.
//!
//! Sessions are indexed by account, so users can list them, revoke one or sign out
//! everywhere else. When a password changes ([`DomainEvent::PasswordChanged`]) every
//! session of that user ends — except the one that made the change, which the web layer
//! brackets with a [`PasswordChangeGuard`].

use std::{
    collections::HashMap,
    fmt::Write as _,
    net::IpAddr,
    sync::{Arc, Mutex, PoisonError},
};

use rbit_auth::{Passwords, SessionToken, TokenDigest};
use rbit_core::{Account, AccountId, AccountName, Config, Email, RegistrationMode, Timestamp, User};
use rbit_storage::{AccountStore, NewUser, SessionRecord, SessionStore, kv::StoredSession};
use tokio::sync::broadcast::{self, error::TryRecvError};

use crate::{
    DomainEvent, EventBus, InstanceService, ServiceError,
    audit::{AuditAction, AuditEntry, AuditService},
};

/// Upper bound on password length: Argon2 accepts more, but unbounded input invites denial of service.
const MAX_PASSWORD_LEN: usize = 256;
/// `last_seen_at` is rewritten at most this often, to keep most requests write-free.
const LAST_SEEN_RESOLUTION_SECONDS: i64 = 5 * 60;
/// Bytes of the token digest used as a session's public identifier.
const SESSION_ID_BYTES: usize = 12;

/// Raw sign-up input, exactly as submitted.
#[derive(Debug, Clone, Default)]
pub struct SignUp {
    pub username: String,
    pub email: String,
    pub password: String,
}

/// Where a request comes from, recorded with new sessions and audit entries.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClientInfo {
    pub ip: Option<IpAddr>,
    /// The `User-Agent` header, empty when absent.
    pub user_agent: String,
}

/// A session as shown to its owner. Never carries the token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveSession {
    /// Public identifier for revocation: a prefix of the token's digest, hex-encoded.
    pub id: String,
    pub created_at: Timestamp,
    pub last_seen_at: Timestamp,
    pub expires_at: Timestamp,
    pub ip_address: Option<String>,
    pub user_agent: String,
    /// The session making the request.
    pub is_current: bool,
}

impl ActiveSession {
    fn new(stored: StoredSession, current: Option<&TokenDigest>) -> Self {
        let StoredSession { digest, record } = stored;
        Self {
            id: session_id(&digest),
            created_at: record.created_at,
            last_seen_at: record.last_seen_at,
            expires_at: record.expires_at,
            ip_address: record.ip_address,
            user_agent: record.user_agent,
            is_current: current == Some(&digest),
        }
    }
}

fn session_id(digest: &TokenDigest) -> String {
    digest[..SESSION_ID_BYTES].iter().fold(String::with_capacity(SESSION_ID_BYTES * 2), |mut hex, byte| {
        let _ = write!(hex, "{byte:02x}");
        hex
    })
}

#[derive(Debug, Clone)]
pub struct AccountService {
    config: Arc<Config>,
    accounts: AccountStore,
    sessions: SessionStore,
    instance: InstanceService,
    events: EventBus,
    audit: AuditService,
    password_changes: Arc<PasswordChangeLedger>,
}

impl AccountService {
    /// Also starts the subscriber that ends sessions after password changes (when called
    /// inside a Tokio runtime, as `Services::new` always is).
    pub(crate) fn new(
        config: Arc<Config>,
        accounts: AccountStore,
        sessions: SessionStore,
        instance: InstanceService,
        events: EventBus,
    ) -> Self {
        let audit = AuditService::from_store(accounts.audit_log());
        let service = Self { config, accounts, sessions, instance, events, audit, password_changes: Arc::default() };
        service.spawn_password_change_subscriber();
        service
    }

    /// Whether the public sign-up form is available (an instance setting administrators
    /// change at runtime). The very first account can always be created so that a fresh
    /// instance can be bootstrapped.
    pub async fn registration_open(&self) -> Result<bool, ServiceError> {
        Ok(self.instance.current().registration == RegistrationMode::Open || self.accounts.count_users().await? == 0)
    }

    /// Registers a user. The first user of an instance becomes its administrator.
    pub async fn sign_up(&self, input: SignUp) -> Result<User, ServiceError> {
        if !self.registration_open().await? {
            return Err(ServiceError::RegistrationClosed);
        }
        self.create_user(input, None).await
    }

    /// Creates a user regardless of the registration setting (admin CLI, admin panel).
    /// `is_admin = None` applies the "first user is admin" rule.
    pub async fn create_user(&self, input: SignUp, is_admin: Option<bool>) -> Result<User, ServiceError> {
        let user = create_user(&self.config, &self.accounts, input, is_admin).await?;
        self.events.publish(DomainEvent::UserSignedUp { user: user.id() });
        Ok(user)
    }

    /// Verifies credentials. Takes the same time whether or not the login exists.
    /// A suspended user with the right password gets [`ServiceError::Forbidden`].
    pub async fn authenticate(&self, login: &str, password: String) -> Result<User, ServiceError> {
        if password.len() > MAX_PASSWORD_LEN * 4 {
            return Err(ServiceError::InvalidCredentials);
        }
        let Some(record) = self.accounts.find_login(login.trim()).await? else {
            Passwords::verify_dummy(password).await;
            return Err(ServiceError::InvalidCredentials);
        };
        if !Passwords::verify(password, record.password_hash).await? {
            return Err(ServiceError::InvalidCredentials);
        }
        if record.suspended {
            tracing::info!(user = %record.user.name(), "suspended user refused sign-in");
            return Err(ServiceError::Forbidden);
        }
        Ok(record.user)
    }

    /// Whether an administrator has suspended the user. Credentials resolved elsewhere
    /// (personal access tokens) must be checked against this before they are honoured.
    pub async fn is_suspended(&self, id: AccountId) -> Result<bool, ServiceError> {
        Ok(self.accounts.is_suspended(id).await?)
    }

    // ------------------------------------------------------------------ sessions

    /// Opens a new session and returns the secret token for the cookie.
    pub async fn start_session(&self, user: &User) -> Result<SessionToken, ServiceError> {
        self.start_session_for(user, &ClientInfo::default()).await
    }

    /// Opens a new session, remembering where it was opened from.
    pub async fn start_session_for(&self, user: &User, client: &ClientInfo) -> Result<SessionToken, ServiceError> {
        let token = SessionToken::generate();
        let ip = client.ip.map(|ip| ip.to_canonical().to_string());
        let record = SessionRecord::new(user.id(), self.session_expiry(), ip, &client.user_agent);
        self.sessions.put(token.digest(), record).await?;
        Ok(token)
    }

    /// Resolves a cookie token to its user, sliding the expiry forward on use. Sessions of
    /// deleted or suspended users are discarded.
    pub async fn resume_session(&self, token: &SessionToken) -> Result<Option<User>, ServiceError> {
        let digest = token.digest();
        let Some(mut record) = self.sessions.get(digest).await? else {
            return Ok(None);
        };
        let Some(user) = self.accounts.find_active_user(record.account_id).await? else {
            self.sessions.delete(digest).await?;
            return Ok(None);
        };

        // Rewrite only when the expiry moves by a day or "last seen" is stale.
        let now = Timestamp::now();
        let renewed = self.session_expiry();
        let renew = renewed.seconds() - record.expires_at.seconds() > 24 * 60 * 60;
        let seen = now.seconds() - record.last_seen_at.seconds() >= LAST_SEEN_RESOLUTION_SECONDS;
        if renew || seen {
            record.expires_at = record.expires_at.max(renewed);
            record.last_seen_at = now;
            self.sessions.put(digest, record).await?;
        }
        Ok(Some(user))
    }

    pub async fn end_session(&self, token: &SessionToken) -> Result<(), ServiceError> {
        Ok(self.sessions.delete(token.digest()).await?)
    }

    /// The user's active sessions, most recently used first; `current` is marked.
    pub async fn sessions(&self, user: &User, current: Option<&SessionToken>) -> Result<Vec<ActiveSession>, ServiceError> {
        let current = current.map(SessionToken::digest);
        let sessions = self.sessions.list_for_account(user.id()).await?;
        Ok(sessions.into_iter().map(|stored| ActiveSession::new(stored, current.as_ref())).collect())
    }

    /// Ends one of the user's sessions by its public id. [`ServiceError::NotFound`] if the
    /// user has no such session.
    pub async fn revoke_session(&self, user: &User, id: &str, client: &ClientInfo) -> Result<(), ServiceError> {
        let sessions = self.sessions.list_for_account(user.id()).await?;
        let stored = sessions.into_iter().find(|stored| session_id(&stored.digest) == id).ok_or(ServiceError::NotFound)?;
        self.sessions.delete(stored.digest).await?;
        let detail = describe_session(&stored.record);
        self.audit.record(AuditEntry::new(AuditAction::SessionRevoked).by(user).from_ip(client.ip).detail(detail)).await;
        Ok(())
    }

    /// Ends every session of the user except `current`; returns how many ended.
    pub async fn revoke_other_sessions(&self, user: &User, current: &SessionToken, client: &ClientInfo) -> Result<usize, ServiceError> {
        let removed = self.sessions.delete_for_account(user.id(), Some(current.digest())).await?;
        let detail = format!("{removed} session{} ended", if removed == 1 { "" } else { "s" });
        self.audit.record(AuditEntry::new(AuditAction::OtherSessionsRevoked).by(user).from_ip(client.ip).detail(detail)).await;
        Ok(removed)
    }

    pub fn session_lifetime_seconds(&self) -> i64 {
        i64::from(self.config.security.session_lifetime_days) * 24 * 60 * 60
    }

    fn session_expiry(&self) -> Timestamp {
        Timestamp(Timestamp::now().seconds() + self.session_lifetime_seconds())
    }

    pub async fn find_account(&self, name: &str) -> Result<Account, ServiceError> {
        self.accounts.find_account_by_name(name).await?.ok_or(ServiceError::NotFound)
    }

    // ------------------------------------------------------------------ password changes

    /// Brackets a request that may change `user`'s own password while signed in with
    /// `current`. Call [`PasswordChangeGuard::finish`] once the response is known: if the
    /// request changed the password, every *other* session ends and `current` survives;
    /// password changes made elsewhere meanwhile still end every session.
    pub fn guard_password_change(&self, user: AccountId, current: &SessionToken) -> PasswordChangeGuard {
        self.password_changes.begin(user);
        PasswordChangeGuard { service: self.clone(), user, current: current.digest(), receiver: Some(self.events.subscribe()) }
    }

    fn spawn_password_change_subscriber(&self) {
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let mut receiver = self.events.subscribe();
        // Hold the stores, not `self`: the subscriber must not keep the event bus alive.
        let enforcer = SessionEnforcer { accounts: self.accounts.clone(), sessions: self.sessions.clone(), audit: self.audit.clone() };
        let ledger = Arc::clone(&self.password_changes);
        runtime.spawn(async move {
            loop {
                match receiver.recv().await {
                    Ok(DomainEvent::PasswordChanged { user }) => {
                        if ledger.on_event(user) {
                            enforcer.end_all_sessions(user).await;
                        }
                    }
                    Ok(_) => {}
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        tracing::warn!(skipped, "session revocation fell behind the event bus");
                        ledger.forget_consumed();
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });
    }
}

/// What the password-change subscriber needs, without the event bus.
#[derive(Debug, Clone)]
struct SessionEnforcer {
    accounts: AccountStore,
    sessions: SessionStore,
    audit: AuditService,
}

impl SessionEnforcer {
    /// Ends the user's sessions (all of them, or all but `keep`) and records it.
    async fn end_sessions(&self, user: AccountId, keep: Option<TokenDigest>) {
        let removed = match self.sessions.delete_for_account(user, keep).await {
            Ok(removed) => removed,
            Err(error) => {
                tracing::error!(%error, user = user.0, "failed to end sessions after a password change");
                return;
            }
        };
        let mut entry = AuditEntry::new(AuditAction::SessionsRevokedAfterPasswordChange)
            .detail(format!("{removed} session{} ended", if removed == 1 { "" } else { "s" }));
        if let Ok(Some(found)) = self.accounts.find_user(user).await {
            entry = entry.by(&found);
        }
        self.audit.record(entry).await;
    }

    async fn end_all_sessions(&self, user: AccountId) {
        self.end_sessions(user, None).await;
    }
}

/// Returned by [`AccountService::guard_password_change`]; see there.
#[derive(Debug)]
pub struct PasswordChangeGuard {
    service: AccountService,
    user: AccountId,
    current: TokenDigest,
    /// `None` once finished.
    receiver: Option<broadcast::Receiver<DomainEvent>>,
}

impl PasswordChangeGuard {
    /// `succeeded`: the request reported success. The change only counts as this
    /// request's own when it also published `PasswordChanged` for the user.
    pub async fn finish(mut self, succeeded: bool) {
        let Some(mut receiver) = self.receiver.take() else { return };
        let mut changed_here = false;
        loop {
            match receiver.try_recv() {
                Ok(DomainEvent::PasswordChanged { user }) if user == self.user => changed_here = true,
                Ok(_) => {}
                // Events were missed: trust the response.
                Err(TryRecvError::Lagged(_)) => changed_here = true,
                Err(TryRecvError::Empty | TryRecvError::Closed) => break,
            }
        }
        let changed_here = changed_here && succeeded;
        let outcome = self.service.password_changes.finish(self.user, changed_here);
        let enforcer = self.service.enforcer();
        if outcome.end_all {
            enforcer.end_all_sessions(self.user).await;
        } else if outcome.end_others {
            enforcer.end_sessions(self.user, Some(self.current)).await;
        }
    }
}

impl Drop for PasswordChangeGuard {
    /// A request abandoned midway (client went away) never keeps its session.
    fn drop(&mut self) {
        if self.receiver.take().is_none() {
            return;
        }
        let outcome = self.service.password_changes.finish(self.user, false);
        if outcome.end_all
            && let Ok(runtime) = tokio::runtime::Handle::try_current()
        {
            let enforcer = self.service.enforcer();
            let user = self.user;
            runtime.spawn(async move { enforcer.end_all_sessions(user).await });
        }
    }
}

impl AccountService {
    fn enforcer(&self) -> SessionEnforcer {
        SessionEnforcer { accounts: self.accounts.clone(), sessions: self.sessions.clone(), audit: self.audit.clone() }
    }
}

/// Decides, per account, whether a `PasswordChanged` event ends every session or was
/// made by a guarded request that keeps its own. The subscriber and the guards may see
/// an event in either order; the counters make both orders reach the same result.
#[derive(Debug, Default)]
struct PasswordChangeLedger {
    accounts: Mutex<HashMap<AccountId, LedgerEntry>>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct LedgerEntry {
    /// Guarded requests still running.
    in_flight: u32,
    /// Events the subscriber saw while requests were in flight, not yet attributed.
    deferred: u32,
    /// Events a finished request claimed before the subscriber saw them.
    consumed: u32,
}

/// What a finishing guard must do.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct FinishOutcome {
    end_others: bool,
    end_all: bool,
}

impl PasswordChangeLedger {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<AccountId, LedgerEntry>> {
        self.accounts.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn begin(&self, user: AccountId) {
        self.lock().entry(user).or_default().in_flight += 1;
    }

    fn finish(&self, user: AccountId, changed_here: bool) -> FinishOutcome {
        let mut accounts = self.lock();
        let entry = accounts.entry(user).or_default();
        entry.in_flight = entry.in_flight.saturating_sub(1);
        if changed_here {
            if entry.deferred > 0 {
                entry.deferred -= 1;
            } else {
                entry.consumed += 1;
            }
        }
        let end_all = entry.in_flight == 0 && entry.deferred > 0;
        if end_all {
            entry.deferred = 0;
        }
        if *entry == LedgerEntry::default() {
            accounts.remove(&user);
        }
        FinishOutcome { end_others: changed_here, end_all }
    }

    /// The subscriber saw a password change; `true` means "end every session now".
    fn on_event(&self, user: AccountId) -> bool {
        let mut accounts = self.lock();
        let Some(entry) = accounts.get_mut(&user) else {
            return true;
        };
        let end_all = if entry.consumed > 0 {
            entry.consumed -= 1;
            false
        } else if entry.in_flight > 0 {
            entry.deferred += 1;
            false
        } else {
            true
        };
        if *entry == LedgerEntry::default() {
            accounts.remove(&user);
        }
        end_all
    }

    /// After the subscriber missed events, claims made for them can never be matched.
    fn forget_consumed(&self) {
        let mut accounts = self.lock();
        for entry in accounts.values_mut() {
            entry.consumed = 0;
        }
        accounts.retain(|_, entry| *entry != LedgerEntry::default());
    }
}

/// e.g. `Mozilla/5.0 … from 192.0.2.1`, for audit details.
fn describe_session(record: &SessionRecord) -> String {
    let agent: String = record.user_agent.chars().take(80).collect();
    match &record.ip_address {
        Some(ip) => format!("{agent} from {ip}").trim().to_owned(),
        None => agent,
    }
}

/// Creates a user with only the relational database, for the `rbit user create`
/// command: it needs neither the key/value store (which a running server holds
/// exclusively) nor background services. Nothing in that process listens for
/// events, so none is published. `is_admin = None` applies the "first user is
/// admin" rule.
pub async fn create_user_offline(
    config: &Config,
    accounts: &AccountStore,
    input: SignUp,
    is_admin: Option<bool>,
) -> Result<User, ServiceError> {
    create_user(config, accounts, input, is_admin).await
}

/// Validates and stores a new user. Shared by sign-up, the CLI and the admin panel.
pub(crate) async fn create_user(
    config: &Config,
    accounts: &AccountStore,
    input: SignUp,
    is_admin: Option<bool>,
) -> Result<User, ServiceError> {
    let name: AccountName = input.username.trim().parse()?;
    let email: Email = input.email.trim().parse()?;
    validate_password(config, &input.password)?;

    let is_admin = match is_admin {
        Some(flag) => flag,
        None => accounts.count_users().await? == 0,
    };
    let password_hash = Passwords::hash(input.password).await?;
    let user = accounts.create_user(NewUser { name, email, display_name: None, password_hash, is_admin }).await?;

    tracing::info!(user = %user.name(), is_admin, "account created");
    Ok(user)
}

fn validate_password(config: &Config, password: &str) -> Result<(), ServiceError> {
    let min = config.security.min_password_length;
    let length = password.chars().count();
    if length < min {
        return Err(ServiceError::Invalid(format!("password must be at least {min} characters")));
    }
    if length > MAX_PASSWORD_LEN {
        return Err(ServiceError::Invalid(format!("password must be at most {MAX_PASSWORD_LEN} characters")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALICE: AccountId = AccountId(1);

    #[test]
    fn unguarded_changes_end_every_session() {
        let ledger = PasswordChangeLedger::default();
        assert!(ledger.on_event(ALICE));
        assert!(ledger.lock().is_empty());
    }

    #[test]
    fn own_change_seen_by_the_guard_first() {
        let ledger = PasswordChangeLedger::default();
        ledger.begin(ALICE);
        assert_eq!(ledger.finish(ALICE, true), FinishOutcome { end_others: true, end_all: false });
        assert!(!ledger.on_event(ALICE), "the subscriber skips the event the guard handled");
        assert!(ledger.lock().is_empty());
    }

    #[test]
    fn own_change_seen_by_the_subscriber_first() {
        let ledger = PasswordChangeLedger::default();
        ledger.begin(ALICE);
        assert!(!ledger.on_event(ALICE), "deferred while the request runs");
        assert_eq!(ledger.finish(ALICE, true), FinishOutcome { end_others: true, end_all: false });
        assert!(ledger.lock().is_empty());
    }

    #[test]
    fn change_elsewhere_during_a_guarded_request_still_ends_everything() {
        let ledger = PasswordChangeLedger::default();
        ledger.begin(ALICE);
        assert!(!ledger.on_event(ALICE));
        assert_eq!(ledger.finish(ALICE, false), FinishOutcome { end_others: false, end_all: true });
        assert!(ledger.on_event(ALICE), "later changes are unaffected");
    }

    #[test]
    fn two_changes_one_of_them_ours() {
        let ledger = PasswordChangeLedger::default();
        ledger.begin(ALICE);
        assert!(!ledger.on_event(ALICE));
        assert!(!ledger.on_event(ALICE));
        assert_eq!(ledger.finish(ALICE, true), FinishOutcome { end_others: true, end_all: true });
        assert!(ledger.lock().is_empty());
    }

    #[test]
    fn missed_events_drop_stale_claims() {
        let ledger = PasswordChangeLedger::default();
        ledger.begin(ALICE);
        ledger.finish(ALICE, true);
        ledger.forget_consumed();
        assert!(ledger.on_event(ALICE));
    }

    #[test]
    fn session_ids_are_short_hex_prefixes() {
        let id = session_id(&[0xab; 32]);
        assert_eq!(id.len(), SESSION_ID_BYTES * 2);
        assert!(id.chars().all(|c| c == 'a' || c == 'b'));
    }
}
