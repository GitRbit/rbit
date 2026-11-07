//! Password reset and email verification.
//!
//! Milestone 1 workstream E owns this file.
//!
//! Both flows send a one-time link. The link carries 256 random bits; only their SHA-256
//! digest is stored, bound to a purpose, an expiry and the address it was sent to (see
//! [`rbit_storage::sqlite::recovery`]).
//!
//! * **Password reset** — [`RecoveryService::request_password_reset`] answers the same way
//!   whether or not an account uses the address, so it can't be used to discover who has an
//!   account. The link is valid for an hour and works once; completing it publishes
//!   [`crate::DomainEvent::PasswordChanged`] after revoking sessions and, since it proves
//!   control of the mailbox, also verifies the address. Suspended users can't reset.
//! * **Verification** — sent when an account is created ([`crate::DomainEvent::UserSignedUp`]) and
//!   when its address changes ([`crate::DomainEvent::EmailChanged`], which also notifies the previous
//!   address), or on request from the account settings.
//!
//! Requests that make the server send mail are throttled with [`ThrottleScope::EmailRequest`],
//! keyed by client address and by recipient address.

use std::{collections::HashSet, fmt, sync::Arc};

use rbit_auth::SessionToken;
use rbit_core::{AccountId, Email, Timestamp, User};
use rbit_identity::PasswordWork;
use rbit_kernel::TaskLifecycle;
use rbit_mail::{
    Mail,
    messages::{PasswordReset, VerifyReason},
};
use rbit_storage::sqlite::recovery::{NewEmailToken, RecoveryAccount, RecoveryStore, TokenPurpose};

use crate::audit::{AuditAction, AuditEntry, AuditService};
use crate::{AccountService, InstanceService, MailService, ServiceContext, ServiceError, ThrottleScope, ThrottleService};

mod background;
mod builder;
mod email_events;
mod password_changes;
mod session_effects;
pub use builder::RecoveryServiceBuilder;

/// How long a password-reset link works.
pub const RESET_LIFETIME_SECONDS: i64 = 60 * 60;
/// How long a verification link works.
pub const VERIFY_LIFETIME_SECONDS: i64 = 2 * 24 * 60 * 60;
/// Upper bound on password length: Argon2 accepts more, but unbounded input invites denial of service.
const MAX_PASSWORD_LEN: usize = 256;
/// Shown wherever a flow needs email on an instance without it.
pub const EMAIL_UNAVAILABLE: &str = "email is not available on this server";

/// The rate limiter as recovery uses it. Implemented by [`ThrottleService`]; tests substitute
/// a recording implementation to check what is throttled and with which keys.
pub trait RequestThrottle: fmt::Debug + Send + Sync {
    /// Fails with [`ServiceError::RateLimited`] when `key` has exhausted its budget in `scope`.
    fn check(&self, scope: ThrottleScope, key: &str) -> Result<(), ServiceError>;

    /// Like [`Self::check`], for a key that is a client address (a larger, shared budget).
    fn check_client(&self, scope: ThrottleScope, address: &str) -> Result<(), ServiceError> {
        self.check(scope, address)
    }
}

impl RequestThrottle for ThrottleService {
    fn check(&self, scope: ThrottleScope, key: &str) -> Result<(), ServiceError> {
        ThrottleService::check(self, scope, key)
    }

    fn check_client(&self, scope: ThrottleScope, address: &str) -> Result<(), ServiceError> {
        ThrottleService::check_client(self, scope, address)
    }
}

/// The verification state of a user's sign-in address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmailStatus {
    pub address: Email,
    /// When the address was last proven, or `None` if it never was (or changed since).
    pub verified_at: Option<Timestamp>,
}

impl EmailStatus {
    pub fn is_verified(&self) -> bool {
        self.verified_at.is_some()
    }
}

/// The address a verification link confirmed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedEmail {
    pub account_id: AccountId,
    pub address: Email,
}

#[derive(Debug, Clone)]
pub struct RecoveryService {
    background: TaskLifecycle,
    fallback_session_effects: Option<rbit_identity::sessions::effects::SessionEffectScope>,
    passwords: PasswordWork,
    ctx: ServiceContext,
    mail: MailService,
    throttle: Arc<dyn RequestThrottle>,
    instance: Option<InstanceService>,
    accounts: Option<AccountService>,
}

impl RecoveryService {
    fn audit(&self) -> AuditService {
        AuditService::new(self.ctx.clone())
    }

    fn store(&self) -> RecoveryStore {
        self.ctx.database.recovery()
    }

    /// Whether flows that send email can be offered at all.
    pub fn email_available(&self) -> bool {
        self.mail.is_enabled()
    }

    // ------------------------------------------------------------------ password reset

    /// Emails a reset link to `address` if a user who may reset uses it. The outcome is
    /// `Ok(())` either way, so the response never reveals whether the account exists.
    ///
    /// `client_key` identifies the requester for rate limiting (their IP address).
    pub async fn request_password_reset(&self, address: &str, client_key: &str) -> Result<(), ServiceError> {
        if !self.email_available() {
            return Err(ServiceError::Invalid(EMAIL_UNAVAILABLE.to_owned()));
        }
        self.throttle.check_client(ThrottleScope::EmailRequest, client_key)?;
        let email: Email = address.trim().parse().map_err(|_| ServiceError::Invalid("enter a valid email address".to_owned()))?;
        self.throttle.check(ThrottleScope::EmailRequest, &recipient_key(&email))?;

        match self.store().find_account_by_email(&email).await? {
            Some(account) if !account.suspended => self.send_reset(&account).await?,
            Some(_) => tracing::info!("password reset requested for a suspended account; ignored"),
            None => tracing::debug!("password reset requested for an unknown address; ignored"),
        }
        Ok(())
    }

    /// The username a valid, unused reset link belongs to (shown on the reset form).
    ///
    /// [`ServiceError::NotFound`] for unknown, expired or used links,
    /// [`ServiceError::Forbidden`] if the account has been suspended since.
    pub async fn reset_target(&self, token: &str) -> Result<String, ServiceError> {
        let account = self.reset_account(token).await?;
        Ok(account.name.to_string())
    }

    async fn reset_account(&self, token: &str) -> Result<RecoveryAccount, ServiceError> {
        let digest = parse_token(token)?;
        let store = self.store();
        let record = store.find_valid(&digest, TokenPurpose::Reset, Timestamp::now()).await?.ok_or(ServiceError::NotFound)?;
        let account = store.find_account(record.account_id).await?.ok_or(ServiceError::NotFound)?;
        if account.suspended {
            return Err(ServiceError::Forbidden);
        }
        Ok(account)
    }

    async fn send_reset(&self, account: &RecoveryAccount) -> Result<(), ServiceError> {
        let token = self.issue(account, TokenPurpose::Reset, RESET_LIFETIME_SECONDS).await?;
        let message = PasswordReset {
            site_name: self.site_name(),
            base_url: self.base_url().to_owned(),
            username: account.name.to_string(),
            link: format!("{}/password/reset/{token}", self.base_url()),
            valid_for: "1 hour".to_owned(),
        };
        self.mail.queue(Mail::render(account.email.clone(), &message).map_err(ServiceError::internal)?).await;
        Ok(())
    }

    // ------------------------------------------------------------------ verification

    /// Confirms the address a verification link was sent to. The link is used up.
    ///
    /// [`ServiceError::NotFound`] for unknown, expired or used links, and for links sent to
    /// an address the account no longer uses.
    pub async fn verify_email(&self, token: &str) -> Result<VerifiedEmail, ServiceError> {
        let digest = parse_token(token)?;
        let store = self.store();
        let record = store.consume(&digest, TokenPurpose::Verify, Timestamp::now()).await?.ok_or(ServiceError::NotFound)?;
        if !store.mark_verified(record.account_id, &record.email, Timestamp::now()).await? {
            return Err(ServiceError::NotFound);
        }
        store.revoke_all(record.account_id, TokenPurpose::Verify).await?;
        tracing::info!(account = record.account_id.0, "email address verified");
        Ok(VerifiedEmail { account_id: record.account_id, address: record.email })
    }

    /// Sends the signed-in user a new verification link.
    pub async fn resend_verification(&self, user: &User) -> Result<(), ServiceError> {
        if !self.email_available() {
            return Err(ServiceError::Invalid(EMAIL_UNAVAILABLE.to_owned()));
        }
        let account = self.store().find_account(user.id()).await?.ok_or(ServiceError::NotFound)?;
        if account.email_verified_at.is_some() {
            return Err(ServiceError::Invalid("your email address is already verified".to_owned()));
        }
        self.throttle.check(ThrottleScope::EmailRequest, &recipient_key(&account.email))?;
        self.send_verification(&account, VerifyReason::Requested).await
    }

    /// The user's address and whether it is verified.
    pub async fn email_status(&self, user: &User) -> Result<EmailStatus, ServiceError> {
        let account = self.store().find_account(user.id()).await?.ok_or(ServiceError::NotFound)?;
        Ok(EmailStatus { address: account.email, verified_at: account.email_verified_at })
    }

    /// Which of `ids` have a verified address (for the administrators' user list).
    pub async fn verified_among(&self, actor: &User, ids: &[AccountId]) -> Result<HashSet<AccountId>, ServiceError> {
        require_admin(actor)?;
        Ok(self.store().verified_among(ids).await?)
    }

    async fn send_verification(&self, account: &RecoveryAccount, reason: VerifyReason) -> Result<(), ServiceError> {
        self.email_events().send_verification(account, reason).await
    }

    // ------------------------------------------------------------------ administration

    /// Emails a reset link to the user on an administrator's behalf. Returns the address.
    pub async fn admin_send_password_reset(&self, actor: &User, target: AccountId) -> Result<Email, ServiceError> {
        require_admin(actor)?;
        if !self.email_available() {
            return Err(ServiceError::Invalid(EMAIL_UNAVAILABLE.to_owned()));
        }
        let account = self.store().find_account(target).await?.ok_or(ServiceError::NotFound)?;
        if account.suspended {
            return Err(ServiceError::Invalid(format!("{} is suspended; lift the suspension first", account.name)));
        }
        self.send_reset(&account).await?;
        tracing::info!(admin = %actor.name(), user = %account.name, "administrator sent a password reset");
        self.audit().record(AuditEntry::new(AuditAction::PasswordResetSent).by(actor).on(account.name.as_str())).await;
        Ok(account.email)
    }

    // ------------------------------------------------------------------ helpers

    /// Stores a new token for the current address and returns its link secret.
    async fn issue(&self, account: &RecoveryAccount, purpose: TokenPurpose, lifetime: i64) -> Result<String, ServiceError> {
        issue_token(&self.store(), account, purpose, lifetime).await
    }

    fn site_name(&self) -> String {
        self.instance.as_ref().map_or_else(|| self.ctx.config.server.site_name.clone(), |instance| instance.current().site_name)
    }

    fn base_url(&self) -> &str {
        self.ctx.config.server.base_url()
    }
}

/// The digest of a link's secret; anything malformed is simply not found.
fn parse_token(token: &str) -> Result<[u8; 32], ServiceError> {
    // Link secrets are drawn exactly like session tokens: 256 random bits, base64url.
    SessionToken::parse(token).map(|secret| secret.digest()).ok_or(ServiceError::NotFound)
}

/// The throttle key for messages sent to `email` (addresses never collide with IPs).
fn recipient_key(email: &Email) -> String {
    email.as_str().to_lowercase()
}

fn require_admin(actor: &User) -> Result<(), ServiceError> {
    if actor.is_admin { Ok(()) } else { Err(ServiceError::Forbidden) }
}

/// The sign-up password rules.
fn validate_password(min: usize, password: &str) -> Result<(), ServiceError> {
    let length = rbit_identity::classify_password_length(password, min, MAX_PASSWORD_LEN);
    if length == rbit_identity::PasswordLength::TooShort {
        return Err(ServiceError::Invalid(format!("password must be at least {min} characters")));
    }
    if length == rbit_identity::PasswordLength::TooLong {
        return Err(ServiceError::Invalid(format!("password must be at most {MAX_PASSWORD_LEN} characters")));
    }
    Ok(())
}

async fn issue_token(
    store: &RecoveryStore,
    account: &RecoveryAccount,
    purpose: TokenPurpose,
    lifetime: i64,
) -> Result<String, ServiceError> {
    let secret = SessionToken::generate();
    let token = NewEmailToken {
        digest: secret.digest(),
        account_id: account.id,
        purpose,
        email: account.email.clone(),
        expires_at: Timestamp(Timestamp::now().seconds() + lifetime),
    };
    store.insert(&token).await?;
    Ok(secret.encode())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_tokens_are_not_found() {
        for token in ["", "short", "!!!!", &"a".repeat(200)] {
            assert!(matches!(parse_token(token), Err(ServiceError::NotFound)), "{token:?}");
        }
        let secret = SessionToken::generate();
        assert_eq!(parse_token(&secret.encode()).unwrap(), secret.digest());
    }

    #[test]
    fn password_rules_match_sign_up() {
        assert!(validate_password(10, "short").is_err());
        assert!(validate_password(10, "long enough!").is_ok());
        assert!(validate_password(10, &"x".repeat(MAX_PASSWORD_LEN + 1)).is_err());
        assert!(validate_password(3, "ééé").is_ok(), "length counts characters, not bytes");
    }

    #[test]
    fn recipient_keys_ignore_case() {
        assert_eq!(recipient_key(&"Alice@Example.COM".parse().unwrap()), "alice@example.com");
    }
}
