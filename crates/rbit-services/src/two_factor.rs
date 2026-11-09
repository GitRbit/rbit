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

use rbit_auth::{Challenge, RecoveryCode, RelyingParty, SessionToken, TokenDigest, TotpSecret};
use rbit_core::{AccountId, Timestamp, User};
use rbit_identity::PasswordWork;
use rbit_identity_models::sessions::ClientInfo;
use rbit_storage::{
    AccountStore,
    sqlite::two_factor::{NewPasskey, PasskeyAdded, PasskeyRecord, TwoFactorRecord, TwoFactorStore},
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

/// Recovery codes issued per enrolment (and per regeneration).
pub const RECOVERY_CODE_COUNT: usize = 10;
/// How long the second sign-in step may take.
pub const PENDING_SIGN_IN_TTL: Duration = Duration::from_mins(5);
/// Wrong codes allowed in one pending sign-in before the password must be entered again.
const PENDING_SIGN_IN_ATTEMPTS: u8 = 5;
/// Pending sign-ins kept at most (oldest beyond this are dropped).
const MAX_PENDING_SIGN_INS: usize = 10_000;
/// How long a passkey challenge issued on a settings page stays valid.
const CHALLENGE_TTL: Duration = Duration::from_mins(10);
/// Accounts with outstanding settings challenges kept at most (oldest beyond this are dropped).
const MAX_CHALLENGES: usize = 10_000;
/// Outstanding challenges per account and ceremony.
const CHALLENGES_PER_ACCOUNT: usize = 4;
/// Passkeys per account.
pub const MAX_PASSKEYS: usize = 20;
/// Longest passkey name, in characters.
pub const MAX_PASSKEY_NAME: usize = 64;
/// Size limit of a browser's `WebAuthn` answer (they are a few hundred bytes to a few KiB).
const MAX_RESPONSE_LEN: usize = 16 * 1024;

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

/// A pending enrolment: what the user puts into their authenticator app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Enrolment {
    /// Base32, for manual entry.
    pub secret: String,
    /// `otpauth://` URI, for the QR code.
    pub provisioning_uri: String,
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

/// The result of registering a passkey.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PasskeyRegistered {
    /// Added next to the account's other factors.
    Added,
    /// The account's first factor: these fresh recovery codes must be shown once.
    FirstFactor(Vec<String>),
}

/// What a settings-page challenge is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Ceremony {
    /// Registering a new passkey.
    Register,
    /// Confirming a sensitive change with an existing passkey.
    Confirm,
}

/// Outstanding settings-page challenges, newest last.
type Challenges = HashMap<(AccountId, Ceremony), Vec<(Challenge, Instant)>>;

#[derive(Debug, Clone)]
pub struct TwoFactorService {
    passwords: PasswordWork,
    store: TwoFactorStore,
    accounts: AccountStore,
    audit: AuditService,
    instance: InstanceService,
    public_url: String,
    pending: Arc<Mutex<HashMap<TokenDigest, PendingSignIn>>>,
    challenges: Arc<Mutex<Challenges>>,
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

    /// Starts (or resumes, so reloading the page keeps an already scanned secret) an
    /// enrolment. `issuer` names the site in the authenticator app.
    pub async fn begin_enrolment(&self, user: &User, issuer: &str) -> Result<Enrolment, ServiceError> {
        let already = || ServiceError::Invalid("an authenticator app is already set up".to_owned());
        let secret = match self.store.find(user.id()).await? {
            Some(record) if record.is_enabled() => return Err(already()),
            Some(record) => TotpSecret::from_bytes(&record.secret),
            None => None,
        };
        let secret = if let Some(secret) = secret {
            secret
        } else {
            let secret = TotpSecret::generate();
            if !self.store.begin_enrolment(user.id(), secret.as_bytes()).await? {
                return Err(already());
            }
            secret
        };
        Ok(Enrolment { secret: secret.to_base32(), provisioning_uri: secret.provisioning_uri(issuer, user.name().as_str()) })
    }

    /// Discards a pending (unconfirmed) enrolment.
    pub async fn cancel_enrolment(&self, user: &User) -> Result<(), ServiceError> {
        self.store.cancel_enrolment(user.id()).await?;
        Ok(())
    }

    /// Enables a pending enrolment if `code` matches; returns new recovery codes if this
    /// is the account's first factor.
    async fn confirm(&self, user: &User, code: &str, client: &ClientInfo) -> Result<Option<Vec<RecoveryCode>>, ServiceError> {
        let Some(record) = self.store.find(user.id()).await?.filter(|record| !record.is_enabled()) else {
            return Err(ServiceError::Invalid("there is no authenticator setup in progress; start again".to_owned()));
        };
        let secret = TotpSecret::from_bytes(&record.secret).ok_or_else(|| ServiceError::Invalid("start the setup again".to_owned()))?;
        let step = secret.verify(code, Timestamp::now().seconds()).ok_or(ServiceError::InvalidCredentials)?;
        let codes = new_recovery_codes();
        let Some(first_factor) = self.store.confirm(user.id(), &record.secret, step, &digests(user, &codes)).await? else {
            return Err(ServiceError::Invalid("the authenticator setup changed; start again".to_owned()));
        };
        self.audit.record(AuditEntry::new(AuditAction::TwoFactorEnabled).by(user).from_ip(client.ip).detail("authenticator app")).await;
        Ok(first_factor.then_some(codes))
    }

    // ------------------------------------------------------------------ passkeys

    /// The relying party for this site. Passkeys are bound to `server.public_url`, so
    /// they only work when the site is opened at that address.
    fn relying_party(&self) -> Result<RelyingParty, ServiceError> {
        RelyingParty::from_public_url(&self.public_url, &self.instance.current().site_name)
            .ok_or_else(|| ServiceError::Invalid("passkeys need `server.public_url` to be set to this site's address".to_owned()))
    }

    fn lock_challenges(&self) -> std::sync::MutexGuard<'_, Challenges> {
        self.challenges.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// A fresh challenge for `ceremony`. The account's few most recent ones stay valid, so
    /// a second tab (or a page fetched again) doesn't break the one already open.
    fn issue_challenge(&self, account: AccountId, ceremony: Ceremony) -> Challenge {
        let challenge = Challenge::generate();
        let now = Instant::now();
        let mut challenges = self.lock_challenges();
        challenges.retain(|_, issued| {
            issued.retain(|(_, at)| now.duration_since(*at) < CHALLENGE_TTL);
            !issued.is_empty()
        });
        if challenges.len() >= MAX_CHALLENGES
            && let Some(oldest) = challenges.iter().min_by_key(|(_, issued)| issued.first().map(|(_, at)| *at)).map(|(key, _)| *key)
        {
            challenges.remove(&oldest);
        }
        let issued = challenges.entry((account, ceremony)).or_default();
        if issued.len() >= CHALLENGES_PER_ACCOUNT {
            issued.remove(0);
        }
        issued.push((challenge.clone(), now));
        challenge
    }

    /// Takes (so it can't be used twice) the account's outstanding challenge for
    /// `ceremony` that `response` answers.
    fn take_challenge(&self, account: AccountId, ceremony: Ceremony, response: &str) -> Option<Challenge> {
        let answered = Challenge::answered_by(response).ok()?;
        let mut challenges = self.lock_challenges();
        let issued = challenges.get_mut(&(account, ceremony))?;
        let index = issued.iter().position(|(challenge, _)| *challenge == answered)?;
        let (challenge, at) = issued.remove(index);
        (at.elapsed() < CHALLENGE_TTL).then_some(challenge)
    }

    /// The options for `navigator.credentials.get` limited to the account's passkeys, or
    /// `None` if it has none.
    fn request_options(&self, passkeys: &[PasskeyRecord], challenge: &Challenge) -> Result<Option<String>, ServiceError> {
        if passkeys.is_empty() {
            return Ok(None);
        }
        let ids: Vec<Vec<u8>> = passkeys.iter().map(|passkey| passkey.credential_id.clone()).collect();
        Ok(Some(self.relying_party()?.request_options(challenge, &ids)))
    }

    /// Options for `navigator.credentials.create` to register a new passkey.
    pub async fn creation_options(&self, user: &User) -> Result<String, ServiceError> {
        let rp = self.relying_party()?;
        let existing: Vec<Vec<u8>> = self.store.passkeys(user.id()).await?.into_iter().map(|passkey| passkey.credential_id).collect();
        let challenge = self.issue_challenge(user.id(), Ceremony::Register);
        Ok(rp.creation_options(&challenge, &user.id().0.to_be_bytes(), user.name().as_str(), &existing))
    }

    /// Options for `navigator.credentials.get` to confirm a sensitive change with a
    /// passkey, or `None` if the account has none.
    pub async fn confirmation_options(&self, user: &User) -> Result<Option<String>, ServiceError> {
        let passkeys = self.store.passkeys(user.id()).await?;
        if passkeys.is_empty() {
            return Ok(None);
        }
        let challenge = self.issue_challenge(user.id(), Ceremony::Confirm);
        self.request_options(&passkeys, &challenge)
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
        self.request_options(&passkeys, &challenge)
    }
}

fn new_recovery_codes() -> Vec<RecoveryCode> {
    (0..RECOVERY_CODE_COUNT).map(|_| RecoveryCode::generate()).collect()
}

fn digests(user: &User, codes: &[RecoveryCode]) -> Vec<[u8; 32]> {
    codes.iter().map(|code| code.digest(user.id().0)).collect()
}

fn formatted(codes: &[RecoveryCode]) -> Vec<String> {
    codes.iter().map(RecoveryCode::formatted).collect()
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

/// A trimmed passkey name, `Passkey` when left empty.
fn passkey_name(name: &str) -> Result<String, ServiceError> {
    let name = name.trim();
    if name.chars().count() > MAX_PASSKEY_NAME {
        return Err(ServiceError::Invalid(format!("the name can be at most {MAX_PASSKEY_NAME} characters")));
    }
    if name.chars().any(char::is_control) {
        return Err(ServiceError::Invalid("the name can't contain control characters".to_owned()));
    }
    Ok(if name.is_empty() { "Passkey".to_owned() } else { name.to_owned() })
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
                Ok(codes.as_deref().map(formatted))
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
        let name = passkey_name(name)?;
        let challenge = self
            .two_factor
            .take_challenge(user.id(), Ceremony::Register, response)
            .ok_or_else(|| ServiceError::Invalid("the passkey request expired; try again".to_owned()))?;
        if self.two_factor.store.passkeys(user.id()).await?.len() >= MAX_PASSKEYS {
            return Err(ServiceError::Invalid(format!("you can register at most {MAX_PASSKEYS} passkeys")));
        }
        if response.len() > MAX_RESPONSE_LEN {
            return Err(ServiceError::Invalid("the passkey response is too large".to_owned()));
        }
        let credential = self.two_factor.relying_party()?.verify_registration(&challenge, response).map_err(|error| {
            tracing::info!(user = %user.name(), %error, "passkey registration refused");
            ServiceError::Invalid("the passkey couldn't be verified; try again".to_owned())
        })?;
        let codes = new_recovery_codes();
        let passkey = NewPasskey {
            credential_id: &credential.id,
            public_key: &credential.public_key,
            algorithm: credential.algorithm,
            sign_count: credential.sign_count,
            transports: &credential.transports,
            name: &name,
        };
        let registered = match self.two_factor.store.add_passkey(user.id(), &passkey, &digests(user, &codes)).await? {
            PasskeyAdded::Duplicate => return Err(ServiceError::Invalid("that passkey is already registered".to_owned())),
            PasskeyAdded::Added => PasskeyRegistered::Added,
            PasskeyAdded::FirstFactor => PasskeyRegistered::FirstFactor(formatted(&codes)),
        };
        self.audit.record(AuditEntry::new(AuditAction::PasskeyAdded).by(user).from_ip(client.ip).detail(&name)).await;
        Ok(registered)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passkey_names_are_trimmed_and_bounded() {
        assert_eq!(passkey_name("  YubiKey 5 ").unwrap(), "YubiKey 5");
        assert_eq!(passkey_name("").unwrap(), "Passkey");
        assert!(passkey_name(&"x".repeat(MAX_PASSKEY_NAME + 1)).is_err());
        assert!(passkey_name("a\u{0}b").is_err());
    }
}
