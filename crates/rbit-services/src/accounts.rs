//! Sign-up, sign-in and session management.
//!
//! Suspended users are refused at every door this service guards: sign-in returns
//! [`ServiceError::Forbidden`] (only after the password has been verified, so suspension
//! is never revealed to someone who doesn't know the password) and sessions stop resolving.
//!
//! Sessions are indexed by account, so users can list them, revoke one or sign out
//! everywhere else. Credential finalization awaits session revocation before returning,
//! so enforcement cannot be lost on the event bus. Recovery and administrator resets end
//! every session. Self-service changes preserve only the initiating session, which the web
//! layer brackets with a [`PasswordChangeGuard`]. However active, a session ends
//! `session_max_age_days` after sign-in.
//!
//! The first account of an instance becomes its administrator, so signing it up needs a
//! setup token: the configured `setup_token`, or a random one generated (and logged by the
//! server) when the instance starts without any user.

use std::{
    collections::HashMap,
    fmt::Write as _,
    net::IpAddr,
    sync::{Arc, Mutex, PoisonError},
};

use rbit_auth::{SessionToken, TokenDigest};
use rbit_core::{Account, AccountId, Config, RegistrationMode, Timestamp, User};
use rbit_identity::PasswordWork;
pub use rbit_storage::SessionUser;
use rbit_storage::{AccountStore, SessionRecord, SessionStore, kv::StoredSession};
use tokio::sync::broadcast::{self, error::TryRecvError};

use crate::{
    DomainEvent, EventBus, InstanceService, ServiceError,
    audit::{AuditAction, AuditEntry, AuditService},
};

pub(crate) mod application_error;
mod password_authentication;
mod registration;
pub use registration::create_user_offline;

/// `last_seen_at` is rewritten at most this often, to keep most requests write-free.
const LAST_SEEN_RESOLUTION_SECONDS: i64 = 5 * 60;
/// Bytes of the token digest used as a session's public identifier.
const SESSION_ID_BYTES: usize = 12;
pub use rbit_identity::accounts::SignUp;

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
    passwords: PasswordWork,
    config: Arc<Config>,
    accounts: AccountStore,
    sessions: SessionStore,
    instance: InstanceService,
    events: EventBus,
    audit: AuditService,
    password_changes: Arc<PasswordChangeLedger>,
    /// Required to sign up the first account; `None` when the instance started with users.
    setup_token: Option<Arc<str>>,
}

impl AccountService {
    /// Shares the session enforcement state with the services that change passwords.
    pub(crate) fn new(
        config: Arc<Config>,
        accounts: AccountStore,
        sessions: SessionStore,
        instance: InstanceService,
        events: EventBus,
        setup_token: Option<String>,
        passwords: PasswordWork,
    ) -> Self {
        let audit = AuditService::from_store(accounts.audit_log());
        let setup_token = setup_token.map(Arc::from);
        Self { config, accounts, sessions, instance, events, audit, password_changes: Arc::default(), setup_token, passwords }
    }

    pub(crate) fn password_work(&self) -> &PasswordWork {
        &self.passwords
    }

    /// Whether the public sign-up form is available (an instance setting administrators
    /// change at runtime). The very first account can always be created so that a fresh
    /// instance can be bootstrapped.
    pub async fn registration_open(&self) -> Result<bool, ServiceError> {
        self.registration()
            .registration_open(self.instance.current().registration == RegistrationMode::Open)
            .await
            .map_err(application_error::registration)
    }

    /// Whether the instance has no user yet, so the next sign-up creates its administrator
    /// and needs the [setup token](Self::setup_token).
    pub async fn setup_required(&self) -> Result<bool, ServiceError> {
        self.registration().setup_required().await.map_err(application_error::registration)
    }

    /// The token the first account's sign-up needs, while the instance started without
    /// users (the server logs it). Also lets tests bootstrap an instance.
    pub fn setup_token(&self) -> Option<&str> {
        self.setup_token.as_deref()
    }

    /// Registers a user. The first user of an instance becomes its administrator, and
    /// needs `setup_token` (ignored for every later sign-up).
    pub fn sign_up<'a>(&'a self, input: SignUp, setup_token: &'a str) -> impl Future<Output = Result<User, ServiceError>> + Send + 'a {
        let operation =
            self.registration().sign_up(input, setup_token, move || self.instance.current().registration == RegistrationMode::Open);
        async move {
            let user = operation.await.map_err(application_error::registration)?;
            self.events.publish(DomainEvent::UserSignedUp { user: user.id() });
            Ok(user)
        }
    }

    fn registration(&self) -> rbit_identity::accounts::AccountRegistration<'_, AccountStore> {
        rbit_identity::accounts::AccountRegistration::new(
            &self.accounts,
            &self.passwords,
            self.config.security.min_password_length,
            self.setup_token.as_deref(),
        )
    }

    /// Creates a user regardless of the registration setting (admin CLI, admin panel).
    /// `is_admin = None` applies the "first user is admin" rule.
    pub fn create_user(&self, input: SignUp, is_admin: Option<bool>) -> impl Future<Output = Result<User, ServiceError>> + Send + '_ {
        let operation = registration::create_user(&self.config, &self.accounts, &self.passwords, input, is_admin);
        async move {
            let user = operation.await?;
            self.events.publish(DomainEvent::UserSignedUp { user: user.id() });
            Ok(user)
        }
    }

    /// Whether an administrator has suspended the user. Credentials resolved elsewhere
    /// (personal access tokens) must be checked against this before they are honoured.
    pub async fn is_suspended(&self, id: AccountId) -> Result<bool, ServiceError> {
        Ok(self.accounts.is_suspended(id).await?)
    }

    /// Restricts administrator-provisioned accounts until their initial password is replaced.
    pub async fn password_change_required(&self, id: AccountId) -> Result<bool, ServiceError> {
        Ok(self.accounts.password_change_required(id).await?)
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
        let record = SessionRecord::new(user.id(), self.session_expiry(Timestamp::now()), ip, &client.user_agent);
        self.sessions.put(token.digest(), record).await?;
        Ok(token)
    }

    /// Resolves a cookie token to its user, sliding expiry forward up to the maximum
    /// session age. Deleted or suspended users and sessions past that age are discarded.
    ///
    /// The user comes with what every page of the request needs to know about them —
    /// whether they must replace their password first and whether they have unread
    /// notifications (always `false` with `[issues] notifications = false`) — read in the
    /// same single query.
    pub async fn resume_session(&self, token: &SessionToken) -> Result<Option<SessionUser>, ServiceError> {
        let digest = token.digest();
        let Some(record) = self.sessions.get(digest).await? else {
            return Ok(None);
        };
        let now = Timestamp::now();
        if self.max_session_end(record.created_at).is_some_and(|end| end <= now) {
            self.sessions.delete(digest).await?;
            return Ok(None);
        }
        let Some(user) = self.accounts.find_session_user(record.account_id, self.config.issues.notifications).await? else {
            self.sessions.delete(digest).await?;
            return Ok(None);
        };

        // Rewrite only when the expiry moves by a day or "last seen" is stale.
        let renewed = self.session_expiry(record.created_at);
        let renew = renewed.seconds() - record.expires_at.seconds() > 24 * 60 * 60;
        let seen = now.seconds() - record.last_seen_at.seconds() >= LAST_SEEN_RESOLUTION_SECONDS;
        if (renew || seen) && !self.sessions.renew(digest, renewed, now).await? {
            return Ok(None);
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

    /// A full idle lifetime from now, capped at the maximum age of a session created at `created_at`.
    fn session_expiry(&self, created_at: Timestamp) -> Timestamp {
        let idle = Timestamp(Timestamp::now().seconds() + self.session_lifetime_seconds());
        self.max_session_end(created_at).map_or(idle, |end| idle.min(end))
    }

    /// When a session created at `created_at` ends however active; `None` without a limit.
    fn max_session_end(&self, created_at: Timestamp) -> Option<Timestamp> {
        let days = self.config.security.session_max_age_days;
        (days > 0).then(|| Timestamp(created_at.seconds() + i64::from(days) * 24 * 60 * 60))
    }

    pub async fn find_account(&self, name: &str) -> Result<Account, ServiceError> {
        self.accounts.find_account_by_name(name).await?.ok_or(ServiceError::NotFound)
    }

    /// Current identity behind an old public profile name, if it has not been retaken.
    pub async fn moved_to(&self, name: &str) -> Result<Option<Account>, ServiceError> {
        Ok(self.accounts.find_by_old_name(name).await?)
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

    /// Recovery and administrative changes always revoke immediately, even while a
    /// self-service request is in flight. A slow request must never postpone a reset.
    pub(crate) async fn password_changed(&self, user: AccountId) -> Result<(), ServiceError> {
        self.enforcer().end_sessions(user, None).await
    }

    /// A self-service change can retain its guarded current session. All other sessions
    /// end when that request completes; without a guard they end before returning.
    pub(crate) async fn password_changed_preserving_current(&self, user: AccountId) -> Result<(), ServiceError> {
        if self.password_changes.on_event(user) {
            self.enforcer().end_sessions(user, None).await?;
        }
        Ok(())
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
    async fn end_sessions(&self, user: AccountId, keep: Option<TokenDigest>) -> Result<(), ServiceError> {
        let removed = self.sessions.delete_for_account(user, keep).await?;
        let mut entry = AuditEntry::new(AuditAction::SessionsRevokedAfterPasswordChange)
            .detail(format!("{removed} session{} ended", if removed == 1 { "" } else { "s" }));
        if let Ok(Some(found)) = self.accounts.find_user(user).await {
            entry = entry.by(&found);
        }
        self.audit.record(entry).await;
        Ok(())
    }

    async fn end_all_sessions(&self, user: AccountId) {
        if let Err(error) = self.end_sessions(user, None).await {
            tracing::error!(%error, user = user.0, "failed to end sessions after a password change");
        }
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
        } else if outcome.end_others
            && let Err(error) = enforcer.end_sessions(self.user, Some(self.current)).await
        {
            tracing::error!(%error, user = self.user.0, "failed to end other sessions after a password change");
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

/// Tracks committed password changes while a self-service request is in flight.
/// Notification is synchronous: a guard never claims an event that might arrive later.
#[derive(Debug, Default)]
struct PasswordChangeLedger {
    accounts: Mutex<HashMap<AccountId, LedgerEntry>>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct LedgerEntry {
    /// Guarded requests still running.
    in_flight: u32,
    /// Committed changes while requests were in flight, not yet attributed.
    deferred: u32,
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
        let changed_here = changed_here && entry.deferred > 0;
        if changed_here {
            entry.deferred -= 1;
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

    /// A password change committed; `true` means "end every session now".
    fn on_event(&self, user: AccountId) -> bool {
        let mut accounts = self.lock();
        let Some(entry) = accounts.get_mut(&user) else {
            return true;
        };
        let end_all = if entry.in_flight > 0 {
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
}

/// Resolves the exact feature-owned bootstrap policy using only the relational store.
pub(crate) async fn bootstrap_setup_token(config: &Config, accounts: &AccountStore) -> Result<Option<String>, ServiceError> {
    rbit_identity::accounts::bootstrap_setup_token(accounts, config.security.setup_token.as_deref())
        .await
        .map_err(application_error::registration)
}

/// e.g. `Mozilla/5.0 … from 192.0.2.1`, for audit details.
fn describe_session(record: &SessionRecord) -> String {
    let agent: String = record.user_agent.chars().take(80).collect();
    match &record.ip_address {
        Some(ip) => format!("{agent} from {ip}").trim().to_owned(),
        None => agent,
    }
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
    fn a_guard_cannot_claim_a_future_change() {
        let ledger = PasswordChangeLedger::default();
        ledger.begin(ALICE);
        assert_eq!(ledger.finish(ALICE, true), FinishOutcome::default());
        assert!(ledger.on_event(ALICE), "a later change must still revoke sessions");
        assert!(ledger.lock().is_empty());
    }

    #[test]
    fn own_change_defers_until_the_guard_finishes() {
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
    fn session_ids_are_short_hex_prefixes() {
        let id = session_id(&[0xab; 32]);
        assert_eq!(id.len(), SESSION_ID_BYTES * 2);
        assert!(id.chars().all(|c| c == 'a' || c == 'b'));
    }
}
