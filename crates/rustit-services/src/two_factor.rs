//! Two-factor authentication (TOTP) and recovery codes.
//!
//! Milestone 1 workstream S owns this file.
//!
//! * Enrolment is two-step: [`TwoFactorService::begin_enrolment`] stores a pending secret
//!   (shown as a QR code and as text); [`Services::confirm_two_factor`] enables it once a
//!   code from the app matches, and returns ten single-use recovery codes, shown once and
//!   stored only as digests.
//! * Sign-in is two-step too: [`Services::sign_in`] checks the password and, for enrolled
//!   users, opens a short-lived pending sign-in instead of a session;
//!   [`Services::complete_sign_in`] finishes it with an authenticator or recovery code.
//! * A code works once: the matched time step is claimed atomically and older or equal
//!   steps are refused afterwards.
//! * With 2FA enabled the account password no longer works for Git over HTTP
//!   ([`Services::authenticate_git_password`]); a personal access token is required.
//!
//! Code attempts are throttled per account ([`ThrottleScope::TwoFactor`]) and audited.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, PoisonError},
    time::{Duration, Instant},
};

use rustit_auth::{Passwords, RecoveryCode, SessionToken, TokenDigest, TotpSecret};
use rustit_core::{AccountId, Timestamp, User};
use rustit_storage::{
    AccountStore,
    sqlite::two_factor::{TwoFactorRecord, TwoFactorStore},
};

use crate::{
    ServiceContext, ServiceError, Services, ThrottleScope,
    accounts::ClientInfo,
    audit::{AuditAction, AuditEntry, AuditService},
};

/// Recovery codes issued per enrolment (and per regeneration).
pub const RECOVERY_CODE_COUNT: usize = 10;
/// How long the second sign-in step may take.
pub const PENDING_SIGN_IN_TTL: Duration = Duration::from_secs(5 * 60);
/// Wrong codes allowed in one pending sign-in before the password must be entered again.
const PENDING_SIGN_IN_ATTEMPTS: u8 = 5;
/// Pending sign-ins kept at most (oldest beyond this are dropped).
const MAX_PENDING_SIGN_INS: usize = 10_000;

/// Whether a user has two-factor authentication, for the settings page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TwoFactorStatus {
    pub enabled_at: Option<Timestamp>,
    pub recovery_codes_left: i64,
}

impl TwoFactorStatus {
    pub fn is_enabled(&self) -> bool {
        self.enabled_at.is_some()
    }
}

/// A pending enrolment: what the user puts into their authenticator app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Enrolment {
    /// Base32, for manual entry.
    pub secret: String,
    /// `otpauth://` URI, for the QR code.
    pub provisioning_uri: String,
}

/// Which second factor was presented.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecondFactor {
    Authenticator,
    RecoveryCode,
}

/// Result of a correct password.
#[derive(Debug)]
pub enum SignInOutcome {
    /// No second factor needed: open a session.
    SignedIn(User),
    /// Ask for a code; the token identifies the pending sign-in (keep it in a short-lived cookie).
    SecondFactorRequired(SessionToken),
}

/// Result of correct Git HTTP credentials made of a login and the account password.
#[derive(Debug)]
pub enum GitPasswordOutcome {
    Authenticated(User),
    /// The password was right, but the account has 2FA, so only tokens are accepted.
    TokenRequired,
}

#[derive(Debug, Clone, Copy)]
struct PendingSignIn {
    account: AccountId,
    started: Instant,
    failures: u8,
}

#[derive(Debug, Clone)]
pub struct TwoFactorService {
    store: TwoFactorStore,
    accounts: AccountStore,
    audit: AuditService,
    pending: Arc<Mutex<HashMap<TokenDigest, PendingSignIn>>>,
}

impl TwoFactorService {
    pub(crate) fn new(ctx: ServiceContext) -> Self {
        let ServiceContext { database, .. } = ctx;
        Self {
            store: database.two_factor(),
            accounts: database.accounts(),
            audit: AuditService::from_store(database.audit()),
            pending: Arc::default(),
        }
    }

    pub async fn status(&self, user: &User) -> Result<TwoFactorStatus, ServiceError> {
        let enabled_at = self.store.find(user.id()).await?.and_then(|record| record.confirmed_at);
        let recovery_codes_left = if enabled_at.is_some() { self.store.remaining_recovery_codes(user.id()).await? } else { 0 };
        Ok(TwoFactorStatus { enabled_at, recovery_codes_left })
    }

    pub async fn is_enabled(&self, account: AccountId) -> Result<bool, ServiceError> {
        Ok(self.store.is_enabled(account).await?)
    }

    /// Starts (or resumes, so reloading the page keeps an already scanned secret) an
    /// enrolment. `issuer` names the site in the authenticator app.
    pub async fn begin_enrolment(&self, user: &User, issuer: &str) -> Result<Enrolment, ServiceError> {
        let secret = match self.store.find(user.id()).await? {
            Some(record) if record.is_enabled() => {
                return Err(ServiceError::Invalid("two-factor authentication is already enabled".to_owned()));
            }
            Some(record) => TotpSecret::from_bytes(&record.secret),
            None => None,
        };
        let secret = if let Some(secret) = secret {
            secret
        } else {
            let secret = TotpSecret::generate();
            if !self.store.begin_enrolment(user.id(), secret.as_bytes()).await? {
                return Err(ServiceError::Invalid("two-factor authentication is already enabled".to_owned()));
            }
            secret
        };
        Ok(Enrolment { secret: secret.to_base32(), provisioning_uri: secret.provisioning_uri(issuer, user.name().as_str()) })
    }

    /// Discards a pending (unconfirmed) enrolment.
    pub async fn cancel_enrolment(&self, user: &User) -> Result<(), ServiceError> {
        if self.store.find(user.id()).await?.is_some_and(|record| !record.is_enabled()) {
            self.store.disable(user.id()).await?;
        }
        Ok(())
    }

    /// Enables a pending enrolment if `code` matches; returns the new recovery codes.
    async fn confirm(&self, user: &User, code: &str, client: &ClientInfo) -> Result<Vec<RecoveryCode>, ServiceError> {
        let Some(record) = self.store.find(user.id()).await?.filter(|record| !record.is_enabled()) else {
            return Err(ServiceError::Invalid("there is no two-factor setup in progress; start again".to_owned()));
        };
        let secret = TotpSecret::from_bytes(&record.secret).ok_or_else(|| ServiceError::Invalid("start the setup again".to_owned()))?;
        let step = secret.verify(code, Timestamp::now().seconds()).ok_or(ServiceError::InvalidCredentials)?;
        let codes = new_recovery_codes();
        if !self.store.confirm(user.id(), step, &digests(user, &codes)).await? {
            return Err(ServiceError::Invalid("two-factor authentication is already enabled".to_owned()));
        }
        self.audit.record(AuditEntry::new(AuditAction::TwoFactorEnabled).by(user).from_ip(client.ip)).await;
        Ok(codes)
    }

    /// Checks a second factor for an enrolled account: a 6-digit code (each time step
    /// works once) or an unused recovery code (spent). [`ServiceError::InvalidCredentials`]
    /// otherwise.
    async fn verify(&self, user: &User, code: &str) -> Result<SecondFactor, ServiceError> {
        let Some(record) = self.store.find(user.id()).await?.filter(TwoFactorRecord::is_enabled) else {
            return Err(ServiceError::InvalidCredentials);
        };
        if let Some(recovery) = RecoveryCode::parse(code) {
            return if self.store.spend_recovery_code(user.id(), &recovery.digest(user.id().0)).await? {
                Ok(SecondFactor::RecoveryCode)
            } else {
                Err(ServiceError::InvalidCredentials)
            };
        }
        let secret = TotpSecret::from_bytes(&record.secret).ok_or(ServiceError::InvalidCredentials)?;
        match secret.verify(code, Timestamp::now().seconds()) {
            Some(step) if self.store.claim_step(user.id(), step).await? => Ok(SecondFactor::Authenticator),
            _ => Err(ServiceError::InvalidCredentials),
        }
    }

    /// Re-checks the account password for sensitive changes.
    async fn verify_password(&self, user: &User, password: String) -> Result<(), ServiceError> {
        let Some(record) = self.accounts.find_login(user.name().as_str()).await? else {
            return Err(ServiceError::InvalidCredentials);
        };
        if Passwords::verify(password, record.password_hash).await? { Ok(()) } else { Err(ServiceError::InvalidCredentials) }
    }

    // ------------------------------------------------------------------ pending sign-ins

    fn lock_pending(&self) -> std::sync::MutexGuard<'_, HashMap<TokenDigest, PendingSignIn>> {
        self.pending.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn start_pending(&self, account: AccountId) -> SessionToken {
        let token = SessionToken::generate();
        let now = Instant::now();
        let mut pending = self.lock_pending();
        pending.retain(|_, entry| now.duration_since(entry.started) < PENDING_SIGN_IN_TTL);
        if pending.len() >= MAX_PENDING_SIGN_INS
            && let Some(oldest) = pending.iter().min_by_key(|(_, entry)| entry.started).map(|(digest, _)| *digest)
        {
            pending.remove(&oldest);
        }
        pending.insert(token.digest(), PendingSignIn { account, started: now, failures: 0 });
        token
    }

    /// The account waiting for its second factor under `token`, if still valid.
    pub fn pending_account(&self, token: &SessionToken) -> Option<AccountId> {
        let mut pending = self.lock_pending();
        let digest = token.digest();
        match pending.get(&digest) {
            Some(entry) if entry.started.elapsed() < PENDING_SIGN_IN_TTL => Some(entry.account),
            Some(_) => {
                pending.remove(&digest);
                None
            }
            None => None,
        }
    }

    /// Counts a wrong code; after too many the pending sign-in is dropped.
    fn pending_failed(&self, token: &SessionToken) {
        let mut pending = self.lock_pending();
        let digest = token.digest();
        if let Some(entry) = pending.get_mut(&digest) {
            entry.failures += 1;
            if entry.failures >= PENDING_SIGN_IN_ATTEMPTS {
                pending.remove(&digest);
            }
        }
    }

    /// Abandons a pending sign-in (completed, or cancelled by the user).
    pub fn end_pending(&self, token: &SessionToken) {
        self.lock_pending().remove(&token.digest());
    }
}

fn new_recovery_codes() -> Vec<RecoveryCode> {
    (0..RECOVERY_CODE_COUNT).map(|_| RecoveryCode::generate()).collect()
}

fn digests(user: &User, codes: &[RecoveryCode]) -> Vec<[u8; 32]> {
    codes.iter().map(|code| code.digest(user.id().0)).collect()
}

/// The throttle key for a client address.
fn ip_key(client: &ClientInfo) -> String {
    client.ip.map_or_else(|| "unknown".to_owned(), |ip| ip.to_canonical().to_string())
}

// ---------------------------------------------------------------------- flows across services

impl Services {
    /// Checks a password sign-in, throttled per client address and per login, and
    /// audited. Enrolled users get a pending second step instead of a session.
    pub async fn sign_in(&self, login: &str, password: String, client: &ClientInfo) -> Result<SignInOutcome, ServiceError> {
        let (ip, login) = (ip_key(client), login.trim());
        self.throttle.check(ThrottleScope::SignIn, &ip)?;
        self.throttle.check(ThrottleScope::SignIn, login)?;

        let user = match self.accounts.authenticate(login, password).await {
            Ok(user) => user,
            Err(error) => {
                let reason = match &error {
                    ServiceError::InvalidCredentials => {
                        self.throttle.record_failure(ThrottleScope::SignIn, &ip);
                        self.throttle.record_failure(ThrottleScope::SignIn, login);
                        "wrong username or password"
                    }
                    ServiceError::Forbidden => "account suspended",
                    _ => return Err(error),
                };
                self.audit.record(AuditEntry::new(AuditAction::SignInFailed).by_login(login).from_ip(client.ip).detail(reason)).await;
                return Err(error);
            }
        };
        self.throttle.reset(ThrottleScope::SignIn, &ip);
        self.throttle.reset(ThrottleScope::SignIn, login);

        if self.two_factor.is_enabled(user.id()).await? {
            return Ok(SignInOutcome::SecondFactorRequired(self.two_factor.start_pending(user.id())));
        }
        self.audit.record(AuditEntry::new(AuditAction::SignIn).by(&user).from_ip(client.ip).detail("password")).await;
        Ok(SignInOutcome::SignedIn(user))
    }

    /// Finishes a pending sign-in with an authenticator or recovery code.
    /// [`ServiceError::NotFound`] when the pending sign-in expired or was used up (start
    /// over), [`ServiceError::InvalidCredentials`] for a wrong code.
    pub async fn complete_sign_in(&self, pending: &SessionToken, code: &str, client: &ClientInfo) -> Result<User, ServiceError> {
        let account = self.two_factor.pending_account(pending).ok_or(ServiceError::NotFound)?;
        let key = account.0.to_string();
        self.throttle.check(ThrottleScope::TwoFactor, &key)?;
        let Some(user) = self.two_factor.accounts.find_active_user(account).await? else {
            self.two_factor.end_pending(pending);
            return Err(ServiceError::NotFound);
        };

        match self.two_factor.verify(&user, code).await {
            Ok(factor) => {
                self.throttle.reset(ThrottleScope::TwoFactor, &key);
                self.two_factor.end_pending(pending);
                let entry = match factor {
                    SecondFactor::Authenticator => AuditEntry::new(AuditAction::SignIn).detail("password and authenticator code"),
                    SecondFactor::RecoveryCode => AuditEntry::new(AuditAction::RecoveryCodeUsed).detail("password and recovery code"),
                };
                self.audit.record(entry.by(&user).from_ip(client.ip)).await;
                Ok(user)
            }
            Err(ServiceError::InvalidCredentials) => {
                self.throttle.record_failure(ThrottleScope::TwoFactor, &key);
                self.two_factor.pending_failed(pending);
                self.audit
                    .record(AuditEntry::new(AuditAction::TwoFactorFailed).by(&user).from_ip(client.ip).detail("during sign-in"))
                    .await;
                Err(ServiceError::InvalidCredentials)
            }
            Err(other) => Err(other),
        }
    }

    /// Enables a pending enrolment with a code from the app (throttled per account).
    /// Returns the recovery codes to show once.
    pub async fn confirm_two_factor(&self, user: &User, code: &str, client: &ClientInfo) -> Result<Vec<String>, ServiceError> {
        let key = user.id().0.to_string();
        self.throttle.check(ThrottleScope::TwoFactor, &key)?;
        match self.two_factor.confirm(user, code, client).await {
            Ok(codes) => {
                self.throttle.reset(ThrottleScope::TwoFactor, &key);
                Ok(codes.iter().map(RecoveryCode::formatted).collect())
            }
            Err(error) => {
                if matches!(error, ServiceError::InvalidCredentials) {
                    self.throttle.record_failure(ThrottleScope::TwoFactor, &key);
                }
                Err(error)
            }
        }
    }

    /// Turns two-factor authentication off; needs the password and a current code (or a
    /// recovery code).
    pub async fn disable_two_factor(&self, user: &User, password: String, code: &str, client: &ClientInfo) -> Result<(), ServiceError> {
        self.check_password_and_code(user, password, code, client).await?;
        self.two_factor.store.disable(user.id()).await?;
        self.audit.record(AuditEntry::new(AuditAction::TwoFactorDisabled).by(user).from_ip(client.ip)).await;
        Ok(())
    }

    /// Replaces the recovery codes (invalidating the old ones); needs the password and a code.
    pub async fn regenerate_recovery_codes(
        &self,
        user: &User,
        password: String,
        code: &str,
        client: &ClientInfo,
    ) -> Result<Vec<String>, ServiceError> {
        self.check_password_and_code(user, password, code, client).await?;
        let codes = new_recovery_codes();
        if !self.two_factor.store.replace_recovery_codes(user.id(), &digests(user, &codes)).await? {
            return Err(ServiceError::Invalid("two-factor authentication is not enabled".to_owned()));
        }
        self.audit.record(AuditEntry::new(AuditAction::RecoveryCodesRegenerated).by(user).from_ip(client.ip)).await;
        Ok(codes.iter().map(RecoveryCode::formatted).collect())
    }

    async fn check_password_and_code(&self, user: &User, password: String, code: &str, client: &ClientInfo) -> Result<(), ServiceError> {
        let key = user.id().0.to_string();
        self.throttle.check(ThrottleScope::TwoFactor, &key)?;
        let verified = match self.two_factor.verify_password(user, password).await {
            Ok(()) => self.two_factor.verify(user, code).await.map(drop),
            Err(error) => Err(error),
        };
        match verified {
            Ok(()) => {
                self.throttle.reset(ThrottleScope::TwoFactor, &key);
                Ok(())
            }
            Err(ServiceError::InvalidCredentials) => {
                self.throttle.record_failure(ThrottleScope::TwoFactor, &key);
                self.audit.record(AuditEntry::new(AuditAction::TwoFactorFailed).by(user).from_ip(client.ip).detail("in settings")).await;
                Err(ServiceError::InvalidCredentials)
            }
            Err(other) => Err(other),
        }
    }

    /// Checks Git HTTP Basic credentials made of a login and the account password,
    /// throttled per client address and per login. Accounts with 2FA must use tokens.
    pub async fn authenticate_git_password(
        &self,
        login: &str,
        password: String,
        client: &ClientInfo,
    ) -> Result<GitPasswordOutcome, ServiceError> {
        let (ip, login) = (ip_key(client), login.trim());
        self.throttle.check(ThrottleScope::GitAuth, &ip)?;
        self.throttle.check(ThrottleScope::GitAuth, login)?;
        let user = match self.accounts.authenticate(login, password).await {
            Ok(user) => user,
            Err(error) => {
                if matches!(error, ServiceError::InvalidCredentials) {
                    self.throttle.record_failure(ThrottleScope::GitAuth, &ip);
                    self.throttle.record_failure(ThrottleScope::GitAuth, login);
                }
                return Err(error);
            }
        };
        self.throttle.reset(ThrottleScope::GitAuth, login);
        if self.two_factor.is_enabled(user.id()).await? {
            return Ok(GitPasswordOutcome::TokenRequired);
        }
        Ok(GitPasswordOutcome::Authenticated(user))
    }
}
