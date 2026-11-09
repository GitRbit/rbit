//! Two-factor authentication: authenticator apps (TOTP), passkeys and recovery codes.
//!
//! Milestone 1 workstream S owns this file.
//!
//! * An account has two-factor authentication while it has a confirmed authenticator app,
//!   at least one passkey (or security key), or both. Its recovery codes are shared by
//!   every factor: issued with the first one, removed with the last.
//! * Authenticator enrolment is two-step: [`TwoFactorService::begin_enrolment`] stores a
//!   pending secret (shown as a QR code and as text); [`Services::confirm_two_factor`]
//!   enables it once a code from the app matches.
//! * Passkeys are registered with [`Services::passkey_creation_options`] (a single-use
//!   challenge for the browser) and [`Services::add_passkey`] (checks the browser's answer).
//! * Sign-in is two-step too: [`Services::sign_in`] checks the password and, for enrolled
//!   users, opens a short-lived pending sign-in instead of a session;
//!   [`Services::complete_sign_in`] finishes it with any [`SecondFactorProof`].
//! * Every proof works once: a TOTP time step is claimed atomically, a recovery code is
//!   spent, a passkey challenge is taken out of memory before it is checked and the
//!   passkey's counter is advanced with a compare-and-set.
//! * Sensitive changes (removing a factor, new recovery codes) need the password and a
//!   proof as well.
//! * With 2FA enabled the account password no longer works for Git over HTTP
//!   ([`Services::authenticate_git_password`]); a personal access token is required.
//!
//! Proof attempts are throttled per account ([`ThrottleScope::TwoFactor`]) and audited.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, PoisonError},
    time::{Duration, Instant},
};

use rbit_auth::{Challenge, RelyingParty, SessionToken, TokenDigest};
use rbit_core::{AccountId, Timestamp, User};
use rbit_identity::PasswordWork;
use rbit_identity::passkeys::{ChallengeBook, PasskeyApplication, PasskeyError, PasskeyRejection};
use rbit_identity::two_factor::{TwoFactorApplication, TwoFactorError, formatted};
use rbit_identity_models::sessions::ClientInfo;
use rbit_storage::{
    AccountStore, StorageError,
    sqlite::two_factor::{PasskeyRecord, TwoFactorStore},
};

use crate::{
    InstanceService, ServiceContext, ServiceError, Services, ThrottleScope,
    audit::{AuditAction, AuditEntry, AuditService},
};

mod pending;
use pending::PendingSignIn;
mod password_checks;
mod password_operations;
mod password_sign_in;
pub use password_sign_in::OriginalSignInOutcome;
mod sign_in;
mod verification;

/// Enrolment and the recovery-code count are identity-owned; these paths stay compatible.
pub use rbit_identity::two_factor::{Enrolment, RECOVERY_CODE_COUNT};
/// How long the second sign-in step may take.
pub const PENDING_SIGN_IN_TTL: Duration = Duration::from_mins(5);
/// Wrong codes allowed in one pending sign-in before the password must be entered again.
const PENDING_SIGN_IN_ATTEMPTS: u8 = 5;
/// Pending sign-ins kept at most (oldest beyond this are dropped).
const MAX_PENDING_SIGN_INS: usize = 10_000;
/// Passkey limits, ceremonies and registration outcomes are identity-owned; these paths stay compatible.
pub use rbit_identity::passkeys::{Ceremony, MAX_PASSKEY_NAME, MAX_PASSKEYS, PasskeyRegistered};

/// A user's second factors, for the settings page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TwoFactorStatus {
    /// When the authenticator app was confirmed, if it is set up.
    pub enabled_at: Option<Timestamp>,
    pub passkeys: Vec<Passkey>,
    pub recovery_codes_left: i64,
}

impl TwoFactorStatus {
    /// Whether any factor protects the account.
    pub fn is_enabled(&self) -> bool {
        self.has_authenticator() || !self.passkeys.is_empty()
    }

    pub fn has_authenticator(&self) -> bool {
        self.enabled_at.is_some()
    }
}

/// A registered passkey, without its key material.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Passkey {
    pub id: i64,
    pub name: String,
    pub created_at: Timestamp,
    pub last_used_at: Option<Timestamp>,
}

impl From<PasskeyRecord> for Passkey {
    fn from(record: PasskeyRecord) -> Self {
        Self { id: record.id, name: record.name, created_at: record.created_at, last_used_at: record.last_used_at }
    }
}

/// What the user presented as their second factor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecondFactorProof<'a> {
    /// A 6-digit authenticator code or a recovery code, as typed.
    Code(&'a str),
    /// The browser's JSON answer to the passkey challenge on the page.
    Passkey(&'a str),
}

impl<'a> From<&'a str> for SecondFactorProof<'a> {
    fn from(code: &'a str) -> Self {
        Self::Code(code)
    }
}

impl<'a> From<&'a String> for SecondFactorProof<'a> {
    fn from(code: &'a String) -> Self {
        Self::Code(code)
    }
}

/// Which second factor was presented.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecondFactor {
    Authenticator,
    RecoveryCode,
    Passkey,
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

#[derive(Debug, Clone)]
pub struct TwoFactorService {
    passwords: PasswordWork,
    store: TwoFactorStore,
    accounts: AccountStore,
    audit: AuditService,
    instance: InstanceService,
    public_url: String,
    pending: Arc<Mutex<HashMap<TokenDigest, PendingSignIn>>>,
    challenges: Arc<ChallengeBook>,
}

impl TwoFactorService {
    pub(crate) fn new(ctx: ServiceContext, instance: InstanceService, passwords: PasswordWork) -> Self {
        let ServiceContext { config, database, .. } = ctx;
        Self {
            passwords,
            store: database.two_factor(),
            accounts: database.accounts(),
            audit: AuditService::from_store(database.audit()),
            instance,
            public_url: config.server.public_url.clone(),
            pending: Arc::default(),
            challenges: Arc::default(),
        }
    }

    pub async fn status(&self, user: &User) -> Result<TwoFactorStatus, ServiceError> {
        let enabled_at = self.store.find(user.id()).await?.and_then(|record| record.confirmed_at);
        let passkeys: Vec<Passkey> = self.store.passkeys(user.id()).await?.into_iter().map(Passkey::from).collect();
        let recovery_codes_left =
            if enabled_at.is_some() || !passkeys.is_empty() { self.store.remaining_recovery_codes(user.id()).await? } else { 0 };
        Ok(TwoFactorStatus { enabled_at, passkeys, recovery_codes_left })
    }

    /// Whether any factor protects the account.
    pub async fn is_enabled(&self, account: AccountId) -> Result<bool, ServiceError> {
        Ok(self.store.is_enabled(account).await?)
    }

    /// Whether the authenticator app is set up (passkeys aside).
    pub async fn has_authenticator(&self, account: AccountId) -> Result<bool, ServiceError> {
        Ok(self.store.find(account).await?.is_some_and(|record| record.is_enabled()))
    }

    pub(super) fn authenticator(&self) -> TwoFactorApplication<'_, TwoFactorStore> {
        TwoFactorApplication::new(&self.store)
    }

    /// Starts (or resumes, so reloading the page keeps an already scanned secret) an
    /// enrolment. `issuer` names the site in the authenticator app.
    pub async fn begin_enrolment(&self, user: &User, issuer: &str) -> Result<Enrolment, ServiceError> {
        self.authenticator().begin_enrolment(user, issuer).await.map_err(map)
    }

    /// Discards a pending (unconfirmed) enrolment.
    pub async fn cancel_enrolment(&self, user: &User) -> Result<(), ServiceError> {
        self.authenticator().cancel_enrolment(user).await.map_err(map)
    }

    /// Enables a pending enrolment if `code` matches; returns new recovery codes if this
    /// is the account's first factor.
    async fn confirm(&self, user: &User, code: &str, client: &ClientInfo) -> Result<Option<Vec<String>>, ServiceError> {
        let codes = self.authenticator().confirm(user, code).await.map_err(map)?;
        self.audit.record(AuditEntry::new(AuditAction::TwoFactorEnabled).by(user).from_ip(client.ip).detail("authenticator app")).await;
        Ok(codes.as_deref().map(formatted))
    }

    // ------------------------------------------------------------------ passkeys

    /// The passkey application, bound to `server.public_url`: passkeys only work when the
    /// site is opened at that address.
    pub(super) fn passkeys(&self) -> PasskeyApplication<'_, TwoFactorStore> {
        let relying_party = RelyingParty::from_public_url(&self.public_url, &self.instance.current().site_name);
        PasskeyApplication::new(&self.store, &self.challenges, relying_party)
    }

    /// Takes the account's outstanding settings challenge for `ceremony` that `response` answers.
    pub(super) fn take_challenge(&self, account: AccountId, ceremony: Ceremony, response: &str) -> Option<Challenge> {
        self.challenges.take(account, ceremony, response)
    }

    /// Options for `navigator.credentials.create` to register a new passkey.
    pub async fn creation_options(&self, user: &User) -> Result<String, ServiceError> {
        self.passkeys().creation_options(user).await.map_err(|error| map_passkey(error, user, |never| match never {}))
    }

    /// Options for `navigator.credentials.get` to confirm a sensitive change with a
    /// passkey, or `None` if the account has none.
    pub async fn confirmation_options(&self, user: &User) -> Result<Option<String>, ServiceError> {
        self.passkeys().confirmation_options(user).await.map_err(|error| map_passkey(error, user, |never| match never {}))
    }

    /// Options for `navigator.credentials.get` on the second sign-in step, or `None` if
    /// the account has no passkeys (or the pending sign-in is gone).
    pub async fn sign_in_options(&self, pending: &SessionToken) -> Result<Option<String>, ServiceError> {
        let Some(account) = self.pending_account(pending) else {
            return Ok(None);
        };
        let passkeys = self.store.passkeys(account).await?;
        if passkeys.is_empty() {
            return Ok(None);
        }
        let challenge = Challenge::generate();
        if let Some(entry) = self.lock_pending().get_mut(&pending.digest()) {
            entry.challenge = Some(challenge.clone());
        }
        self.passkeys().request_options(&passkeys, &challenge).map_err(|error| match error {
            PasskeyError::Rejected(rejection) => ServiceError::Invalid(rejection.to_string()),
            PasskeyError::Store(error) => error.into(),
            _ => ServiceError::InvalidCredentials,
        })
    }
}

/// Maps identity outcomes; the claim error is the adapter's own [`ServiceError`].
fn map(error: TwoFactorError<StorageError>) -> ServiceError {
    convert(error, |never| match never {})
}

fn convert<C>(error: TwoFactorError<StorageError, C>, claim: impl FnOnce(C) -> ServiceError) -> ServiceError {
    match error {
        TwoFactorError::Store(error) => error.into(),
        TwoFactorError::Claim(error) => claim(error),
        TwoFactorError::Rejected(rejection) => ServiceError::Invalid(rejection.to_string()),
        TwoFactorError::InvalidCredentials => ServiceError::InvalidCredentials,
    }
}

/// The throttle key for a client address.
fn ip_key(client: &ClientInfo) -> String {
    client.ip.map_or_else(|| "unknown".to_owned(), |ip| ip.to_canonical().to_string())
}

/// The sign-in throttle key for a login as typed. Prefixed so that no typed login can
/// share a bucket with an [`account_key`].
fn login_key(login: &str) -> String {
    format!("login:{login}")
}

/// The sign-in throttle key for a resolved account.
fn account_key(account: AccountId) -> String {
    format!("account:{}", account.0)
}

/// Maps passkey outcomes; refusal reasons are logged and never shown.
fn map_passkey<C>(error: PasskeyError<StorageError, C>, user: &User, claim: impl FnOnce(C) -> ServiceError) -> ServiceError {
    match error {
        PasskeyError::Store(error) => error.into(),
        PasskeyError::Claim(error) => claim(error),
        PasskeyError::Rejected(rejection) => ServiceError::Invalid(rejection.to_string()),
        PasskeyError::RegistrationRefused(error) => {
            tracing::info!(user = %user.name(), %error, "passkey registration refused");
            ServiceError::Invalid(PasskeyRejection::NotVerified.to_string())
        }
        PasskeyError::AssertionRefused(error) => {
            tracing::info!(user = %user.name(), %error, "passkey sign-in refused");
            ServiceError::InvalidCredentials
        }
        PasskeyError::InvalidCredentials => ServiceError::InvalidCredentials,
    }
}

// ---------------------------------------------------------------------- flows across services

impl Services {
    /// Enables a pending authenticator enrolment with a code from the app (throttled per
    /// account). Returns the recovery codes to show once if this is the account's first
    /// factor, `None` if it keeps its existing codes.
    pub async fn confirm_two_factor(&self, user: &User, code: &str, client: &ClientInfo) -> Result<Option<Vec<String>>, ServiceError> {
        let key = user.id().0.to_string();
        self.throttle.check(ThrottleScope::TwoFactor, &key)?;
        match self.two_factor.confirm(user, code, client).await {
            Ok(codes) => {
                self.throttle.reset(ThrottleScope::TwoFactor, &key);
                Ok(codes)
            }
            Err(error) => {
                if matches!(error, ServiceError::InvalidCredentials) {
                    self.throttle.record_failure(ThrottleScope::TwoFactor, &key);
                }
                Err(error)
            }
        }
    }

    /// Registers a passkey from the browser's answer to
    /// [`TwoFactorService::creation_options`], named `name`.
    pub async fn add_passkey(
        &self,
        user: &User,
        name: &str,
        response: &str,
        client: &ClientInfo,
    ) -> Result<PasskeyRegistered, ServiceError> {
        let (name, registered) = self
            .two_factor
            .passkeys()
            .register(user, name, response)
            .await
            .map_err(|error| map_passkey(error, user, |never| match never {}))?;
        self.audit.record(AuditEntry::new(AuditAction::PasskeyAdded).by(user).from_ip(client.ip).detail(&name)).await;
        Ok(registered)
    }
}
