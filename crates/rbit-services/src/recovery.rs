//! Password reset and email verification: the service adapter over the identity
//! recovery application.
//!
//! Both flows send a one-time link. The link carries 256 random bits; only their SHA-256
//! digest is stored, bound to a purpose, an expiry and the address it was sent to (see
//! [`rbit_identity::recovery`], which owns issuance, redemption and password policy).
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
//! keyed by client address and by recipient address. This adapter renders and queues mail,
//! logs, audits, publishes events and finalizes sessions.

use std::{collections::HashSet, fmt, sync::Arc};

use rbit_core::{AccountId, Email, Timestamp, User};
use rbit_identity::PasswordWork;
use rbit_identity::recovery::{RecoveryError, RecoveryLinks, RecoveryThrottle, ResetRequest};
use rbit_kernel::TaskLifecycle;
use rbit_mail::{
    Mail,
    messages::{PasswordReset, VerifyReason},
};
use rbit_storage::StorageError;
use rbit_storage::sqlite::recovery::{RecoveryAccount, RecoveryStore, TokenPurpose};

use crate::audit::{AuditAction, AuditEntry, AuditService};
use crate::{AccountService, InstanceService, MailService, ServiceContext, ServiceError, ThrottleScope, ThrottleService};

mod background;
mod builder;
mod email_events;
mod password_changes;
mod session_effects;
pub use builder::RecoveryServiceBuilder;

/// Link lifetimes and the unavailable-email text are identity-owned; these paths stay compatible.
pub use rbit_identity::recovery::{EMAIL_UNAVAILABLE, RESET_LIFETIME_SECONDS, VERIFY_LIFETIME_SECONDS};

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

/// Mail-request buckets of the shared throttle, keyed by client and by recipient.
struct Throttle<'a>(&'a dyn RequestThrottle);

impl RecoveryThrottle for Throttle<'_> {
    type Error = ServiceError;
    fn check_client(&self, client: &str) -> Result<(), ServiceError> {
        self.0.check_client(ThrottleScope::EmailRequest, client)
    }
    fn check_recipient(&self, key: &str) -> Result<(), ServiceError> {
        self.0.check(ThrottleScope::EmailRequest, key)
    }
}

impl RecoveryService {
    fn audit(&self) -> AuditService {
        AuditService::new(self.ctx.clone())
    }

    fn store(&self) -> RecoveryStore {
        self.ctx.database.recovery()
    }

    fn links<'a>(&self, store: &'a RecoveryStore, throttle: &'a Throttle<'a>) -> RecoveryLinks<'a, RecoveryStore, Throttle<'a>> {
        RecoveryLinks::new(store, throttle, self.email_available())
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
        let (store, throttle) = (self.store(), Throttle(&*self.throttle));
        match self.links(&store, &throttle).request_reset(address, client_key).await.map_err(map)? {
            ResetRequest::Send(account) => self.send_reset(&account).await?,
            ResetRequest::Suspended => tracing::info!("password reset requested for a suspended account; ignored"),
            ResetRequest::Unknown => tracing::debug!("password reset requested for an unknown address; ignored"),
        }
        Ok(())
    }

    /// The username a valid, unused reset link belongs to (shown on the reset form).
    ///
    /// [`ServiceError::NotFound`] for unknown, expired or used links,
    /// [`ServiceError::Forbidden`] if the account has been suspended since.
    pub async fn reset_target(&self, token: &str) -> Result<String, ServiceError> {
        let (store, throttle) = (self.store(), Throttle(&*self.throttle));
        let account = self.links(&store, &throttle).reset_account(token).await.map_err(map)?;
        Ok(account.name.to_string())
    }

    async fn send_reset(&self, account: &RecoveryAccount) -> Result<(), ServiceError> {
        let (store, throttle) = (self.store(), Throttle(&*self.throttle));
        let token = self.links(&store, &throttle).issue(account, TokenPurpose::Reset).await.map_err(map)?;
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
        let (store, throttle) = (self.store(), Throttle(&*self.throttle));
        let record = self.links(&store, &throttle).verify_email(token).await.map_err(map)?;
        tracing::info!(account = record.account_id.0, "email address verified");
        Ok(VerifiedEmail { account_id: record.account_id, address: record.email })
    }

    /// Sends the signed-in user a new verification link.
    pub async fn resend_verification(&self, user: &User) -> Result<(), ServiceError> {
        let (store, throttle) = (self.store(), Throttle(&*self.throttle));
        let account = self.links(&store, &throttle).resend_target(user).await.map_err(map)?;
        self.send_verification(&account, VerifyReason::Requested).await
    }

    /// The user's address and whether it is verified.
    pub async fn email_status(&self, user: &User) -> Result<EmailStatus, ServiceError> {
        let (store, throttle) = (self.store(), Throttle(&*self.throttle));
        let account = self.links(&store, &throttle).email_status(user).await.map_err(map)?;
        Ok(EmailStatus { address: account.email, verified_at: account.email_verified_at })
    }

    /// Which of `ids` have a verified address (for the administrators' user list).
    pub async fn verified_among(&self, actor: &User, ids: &[AccountId]) -> Result<HashSet<AccountId>, ServiceError> {
        let (store, throttle) = (self.store(), Throttle(&*self.throttle));
        self.links(&store, &throttle).verified_among(actor, ids).await.map_err(map)
    }

    async fn send_verification(&self, account: &RecoveryAccount, reason: VerifyReason) -> Result<(), ServiceError> {
        self.email_events().send_verification(account, reason).await
    }

    // ------------------------------------------------------------------ administration

    /// Emails a reset link to the user on an administrator's behalf. Returns the address.
    pub async fn admin_send_password_reset(&self, actor: &User, target: AccountId) -> Result<Email, ServiceError> {
        let (store, throttle) = (self.store(), Throttle(&*self.throttle));
        let account = self.links(&store, &throttle).admin_reset_target(actor, target).await.map_err(map)?;
        self.send_reset(&account).await?;
        tracing::info!(admin = %actor.name(), user = %account.name, "administrator sent a password reset");
        self.audit().record(AuditEntry::new(AuditAction::PasswordResetSent).by(actor).on(account.name.as_str())).await;
        Ok(account.email)
    }

    // ------------------------------------------------------------------ helpers

    fn site_name(&self) -> String {
        self.instance.as_ref().map_or_else(|| self.ctx.config.server.site_name.clone(), |instance| instance.current().site_name)
    }

    fn base_url(&self) -> &str {
        self.ctx.config.server.base_url()
    }
}

fn map(error: RecoveryError<StorageError, ServiceError>) -> ServiceError {
    convert(error, |error| error, |never| match never {})
}

/// Errors from the password preparations (no throttle) and the commits (neither).
fn map_passwords(error: RecoveryError<StorageError, std::convert::Infallible, ServiceError>) -> ServiceError {
    convert(error, |never| match never {}, |error| error)
}

fn map_commit(error: RecoveryError<StorageError>) -> ServiceError {
    convert(error, |never| match never {}, |never| match never {})
}

fn convert<T, R>(
    error: RecoveryError<StorageError, T, R>,
    throttle: impl FnOnce(T) -> ServiceError,
    reserve: impl FnOnce(R) -> ServiceError,
) -> ServiceError {
    match error {
        RecoveryError::Store(error) => error.into(),
        RecoveryError::Throttle(error) => throttle(error),
        RecoveryError::Reserve(error) => reserve(error),
        RecoveryError::Rejected(rejection) => ServiceError::Invalid(rejection.to_string()),
        RecoveryError::NotFound => ServiceError::NotFound,
        RecoveryError::Forbidden => ServiceError::Forbidden,
        RecoveryError::Admission(error) => error.into(),
        RecoveryError::Password(error) => error.into(),
    }
}
