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

use rbit_auth::{Challenge, Passwords, RecoveryCode, RelyingParty, SessionToken, TokenDigest, TotpSecret};
use rbit_core::{AccountId, Timestamp, User};
use rbit_storage::{
    AccountStore,
    sqlite::two_factor::{NewPasskey, PasskeyAdded, PasskeyRecord, TwoFactorRecord, TwoFactorStore},
};

use crate::{
    InstanceService, ServiceContext, ServiceError, Services, ThrottleScope,
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
/// How long a passkey challenge issued on a settings page stays valid.
const CHALLENGE_TTL: Duration = Duration::from_secs(10 * 60);
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

#[derive(Debug, Clone)]
struct PendingSignIn {
    account: AccountId,
    started: Instant,
    failures: u8,
    /// The passkey challenge last shown on the second-step page.
    challenge: Option<Challenge>,
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
    store: TwoFactorStore,
    accounts: AccountStore,
    audit: AuditService,
    instance: InstanceService,
    public_url: String,
    pending: Arc<Mutex<HashMap<TokenDigest, PendingSignIn>>>,
    challenges: Arc<Mutex<Challenges>>,
}

impl TwoFactorService {
    pub(crate) fn new(ctx: ServiceContext, instance: InstanceService) -> Self {
        let ServiceContext { config, database, .. } = ctx;
        Self {
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
        if self.store.find(user.id()).await?.is_some_and(|record| !record.is_enabled()) {
            self.store.disable(user.id()).await?;
        }
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
        let Some(first_factor) = self.store.confirm(user.id(), step, &digests(user, &codes)).await? else {
            return Err(ServiceError::Invalid("an authenticator app is already set up".to_owned()));
        };
        self.audit.record(AuditEntry::new(AuditAction::TwoFactorEnabled).by(user).from_ip(client.ip).detail("authenticator app")).await;
        Ok(first_factor.then_some(codes))
    }

    /// Checks a second factor for an enrolled account: a 6-digit code (each time step
    /// works once), an unused recovery code (spent) or a passkey's answer to `challenge`.
    /// [`ServiceError::InvalidCredentials`] otherwise.
    async fn verify(&self, user: &User, proof: SecondFactorProof<'_>, challenge: Option<Challenge>) -> Result<SecondFactor, ServiceError> {
        let code = match proof {
            SecondFactorProof::Code(code) => code,
            SecondFactorProof::Passkey(response) => {
                let challenge = challenge.ok_or(ServiceError::InvalidCredentials)?;
                return self.verify_passkey(user, &challenge, response).await.map(|()| SecondFactor::Passkey);
            }
        };
        if let Some(recovery) = RecoveryCode::parse(code) {
            return if self.store.spend_recovery_code(user.id(), &recovery.digest(user.id().0)).await? {
                Ok(SecondFactor::RecoveryCode)
            } else {
                Err(ServiceError::InvalidCredentials)
            };
        }
        let Some(record) = self.store.find(user.id()).await?.filter(TwoFactorRecord::is_enabled) else {
            return Err(ServiceError::InvalidCredentials);
        };
        let secret = TotpSecret::from_bytes(&record.secret).ok_or(ServiceError::InvalidCredentials)?;
        match secret.verify(code, Timestamp::now().seconds()) {
            Some(step) if self.store.claim_step(user.id(), step).await? => Ok(SecondFactor::Authenticator),
            _ => Err(ServiceError::InvalidCredentials),
        }
    }

    async fn verify_passkey(&self, user: &User, challenge: &Challenge, response: &str) -> Result<(), ServiceError> {
        if response.len() > MAX_RESPONSE_LEN {
            return Err(ServiceError::InvalidCredentials);
        }
        let rp = self.relying_party()?;
        let refused = |error: rbit_auth::WebauthnError| {
            tracing::info!(user = %user.name(), %error, "passkey sign-in refused");
            ServiceError::InvalidCredentials
        };
        let credential_id = RelyingParty::assertion_credential_id(response).map_err(refused)?;
        let Some(passkey) = self.store.find_passkey(user.id(), &credential_id).await? else {
            return Err(ServiceError::InvalidCredentials);
        };
        let count = rp.verify_assertion(challenge, response, &passkey.public_key, passkey.sign_count).map_err(refused)?;
        if self.store.use_passkey(user.id(), passkey.id, passkey.sign_count, count).await? {
            Ok(())
        } else {
            Err(ServiceError::InvalidCredentials)
        }
    }

    /// Re-checks the account password for sensitive changes.
    async fn verify_password(&self, user: &User, password: String) -> Result<(), ServiceError> {
        let Some(record) = self.accounts.find_login(user.name().as_str()).await? else {
            return Err(ServiceError::InvalidCredentials);
        };
        if Passwords::verify(password, record.password_hash).await? { Ok(()) } else { Err(ServiceError::InvalidCredentials) }
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
        pending.insert(token.digest(), PendingSignIn { account, started: now, failures: 0, challenge: None });
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

    /// Takes the pending sign-in's passkey challenge, so each works once.
    fn take_pending_challenge(&self, token: &SessionToken) -> Option<Challenge> {
        self.lock_pending().get_mut(&token.digest()).and_then(|entry| entry.challenge.take())
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

fn formatted(codes: &[RecoveryCode]) -> Vec<String> {
    codes.iter().map(RecoveryCode::formatted).collect()
}

/// The throttle key for a client address.
fn ip_key(client: &ClientInfo) -> String {
    client.ip.map_or_else(|| "unknown".to_owned(), |ip| ip.to_canonical().to_string())
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
    /// Checks a password sign-in, throttled per client address and per login, and
    /// audited. Enrolled users get a pending second step instead of a session.
    pub async fn sign_in(&self, login: &str, password: String, client: &ClientInfo) -> Result<SignInOutcome, ServiceError> {
        let (ip, login) = (ip_key(client), login.trim());
        self.throttle.check_client(ThrottleScope::SignIn, &ip)?;
        self.throttle.check(ThrottleScope::SignIn, login)?;

        let user = match self.accounts.authenticate(login, password).await {
            Ok(user) => user,
            Err(error) => {
                let reason = match &error {
                    ServiceError::InvalidCredentials => {
                        self.throttle.record_client_failure(ThrottleScope::SignIn, &ip);
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
        self.throttle.reset_client(ThrottleScope::SignIn, &ip);
        self.throttle.reset(ThrottleScope::SignIn, login);

        if self.two_factor.is_enabled(user.id()).await? {
            return Ok(SignInOutcome::SecondFactorRequired(self.two_factor.start_pending(user.id())));
        }
        self.audit.record(AuditEntry::new(AuditAction::SignIn).by(&user).from_ip(client.ip).detail("password")).await;
        Ok(SignInOutcome::SignedIn(user))
    }

    /// Finishes a pending sign-in with an authenticator code, a recovery code or a passkey
    /// (answering [`TwoFactorService::sign_in_options`]). [`ServiceError::NotFound`] when
    /// the pending sign-in expired or was used up (start over),
    /// [`ServiceError::InvalidCredentials`] for a wrong proof.
    pub async fn complete_sign_in<'a>(
        &self,
        pending: &SessionToken,
        proof: impl Into<SecondFactorProof<'a>>,
        client: &ClientInfo,
    ) -> Result<User, ServiceError> {
        let proof = proof.into();
        let account = self.two_factor.pending_account(pending).ok_or(ServiceError::NotFound)?;
        let key = account.0.to_string();
        self.throttle.check(ThrottleScope::TwoFactor, &key)?;
        let Some(user) = self.two_factor.accounts.find_active_user(account).await? else {
            self.two_factor.end_pending(pending);
            return Err(ServiceError::NotFound);
        };

        let challenge = matches!(proof, SecondFactorProof::Passkey(_)).then(|| self.two_factor.take_pending_challenge(pending)).flatten();
        match self.two_factor.verify(&user, proof, challenge).await {
            Ok(factor) => {
                self.throttle.reset(ThrottleScope::TwoFactor, &key);
                self.two_factor.end_pending(pending);
                let entry = match factor {
                    SecondFactor::Authenticator => AuditEntry::new(AuditAction::SignIn).detail("password and authenticator code"),
                    SecondFactor::Passkey => AuditEntry::new(AuditAction::SignIn).detail("password and passkey"),
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

    /// Removes passkey `id`; needs the password and a proof.
    pub async fn remove_passkey<'a>(
        &self,
        user: &User,
        id: i64,
        password: String,
        proof: impl Into<SecondFactorProof<'a>>,
        client: &ClientInfo,
    ) -> Result<(), ServiceError> {
        let Some(passkey) = self.two_factor.store.passkeys(user.id()).await?.into_iter().find(|passkey| passkey.id == id) else {
            return Err(ServiceError::NotFound);
        };
        self.check_password_and_proof(user, password, proof.into(), client).await?;
        self.two_factor.store.remove_passkey(user.id(), id).await?;
        self.audit.record(AuditEntry::new(AuditAction::PasskeyRemoved).by(user).from_ip(client.ip).detail(&passkey.name)).await;
        Ok(())
    }

    /// Removes the authenticator app; needs the password and a proof. Two-factor
    /// authentication stays on if the account has passkeys.
    pub async fn disable_two_factor<'a>(
        &self,
        user: &User,
        password: String,
        proof: impl Into<SecondFactorProof<'a>>,
        client: &ClientInfo,
    ) -> Result<(), ServiceError> {
        self.check_password_and_proof(user, password, proof.into(), client).await?;
        self.two_factor.store.disable(user.id()).await?;
        self.audit.record(AuditEntry::new(AuditAction::TwoFactorDisabled).by(user).from_ip(client.ip).detail("authenticator app")).await;
        Ok(())
    }

    /// Replaces the recovery codes (invalidating the old ones); needs the password and a proof.
    pub async fn regenerate_recovery_codes<'a>(
        &self,
        user: &User,
        password: String,
        proof: impl Into<SecondFactorProof<'a>>,
        client: &ClientInfo,
    ) -> Result<Vec<String>, ServiceError> {
        self.check_password_and_proof(user, password, proof.into(), client).await?;
        let codes = new_recovery_codes();
        if !self.two_factor.store.replace_recovery_codes(user.id(), &digests(user, &codes)).await? {
            return Err(ServiceError::Invalid("two-factor authentication is not enabled".to_owned()));
        }
        self.audit.record(AuditEntry::new(AuditAction::RecoveryCodesRegenerated).by(user).from_ip(client.ip)).await;
        Ok(formatted(&codes))
    }

    async fn check_password_and_proof(
        &self,
        user: &User,
        password: String,
        proof: SecondFactorProof<'_>,
        client: &ClientInfo,
    ) -> Result<(), ServiceError> {
        let key = user.id().0.to_string();
        self.throttle.check(ThrottleScope::TwoFactor, &key)?;
        // Taken before anything is checked, so a failed attempt can't be retried with it.
        let challenge = match proof {
            SecondFactorProof::Passkey(response) => self.two_factor.take_challenge(user.id(), Ceremony::Confirm, response),
            SecondFactorProof::Code(_) => None,
        };
        let verified = match self.two_factor.verify_password(user, password).await {
            Ok(()) => self.two_factor.verify(user, proof, challenge).await.map(drop),
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
        self.throttle.check_client(ThrottleScope::GitAuth, &ip)?;
        self.throttle.check(ThrottleScope::GitAuth, login)?;
        let user = match self.accounts.authenticate(login, password).await {
            Ok(user) => user,
            Err(error) => {
                if matches!(error, ServiceError::InvalidCredentials) {
                    self.throttle.record_client_failure(ThrottleScope::GitAuth, &ip);
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
