//! Password reset and email verification: one-use links, expiry, enumeration resistance.
//!
//! A link carries 256 random bits drawn and encoded exactly like a session token; only
//! the SHA-256 digest is stored, bound to a purpose, an expiry and the address it was
//! sent to. [`RecoveryLinks`] decides who receives a link and redeems verification links.
//! [`RecoveryPasswords`] validates and hashes replacement passwords from a reset link or an
//! administrator. Redeeming is single-use because the repository consumes atomically.
//! Callers keep mail rendering and delivery, logging, audit, events and session
//! finalization.

mod links;
mod passwords;
mod rejection;

pub use links::{RecoveryLinks, ResetRequest};
pub use passwords::{PreparedAdminPassword, PreparedReset, RecoveryPasswords, commit_admin_password, commit_reset};
pub use rbit_identity_models::recovery::{
    EmailTokenRecord, NewEmailToken, RecoveryAccount, RecoveryCredentialRepository, RecoveryRepository, TokenPurpose,
};
pub use rejection::RecoveryRejection;

use crate::{Email, PasswordError, SessionToken, TokenDigest, User};
use rbit_kernel::AdmissionError;

/// How long a password-reset link works.
pub const RESET_LIFETIME_SECONDS: i64 = 60 * 60;
/// How long a verification link works.
pub const VERIFY_LIFETIME_SECONDS: i64 = 2 * 24 * 60 * 60;
/// Upper bound on password length: Argon2 accepts more, but unbounded input invites denial of service.
pub const MAX_PASSWORD_LEN: usize = 256;
/// Shown wherever a flow needs email on an instance without it.
pub const EMAIL_UNAVAILABLE: &str = "email is not available on this server";

/// Mail-request limits. Implementations own buckets; the application decides the order.
pub trait RecoveryThrottle: Sync {
    type Error: Send;
    /// The requester's client address (a larger, shared budget).
    fn check_client(&self, client: &str) -> Result<(), Self::Error>;
    /// The recipient, keyed by [`recipient_key`].
    fn check_recipient(&self, key: &str) -> Result<(), Self::Error>;
}

#[derive(Debug)]
pub enum RecoveryError<Store, Throttle = std::convert::Infallible, Reserve = std::convert::Infallible> {
    Store(Store),
    Throttle(Throttle),
    /// The caller's session-finalization reservation was refused.
    Reserve(Reserve),
    /// The rejection's text is user-facing.
    Rejected(RecoveryRejection),
    /// Unknown, expired, used or malformed links, and missing accounts.
    NotFound,
    /// Not an administrator, or the link's account has been suspended since.
    Forbidden,
    Admission(AdmissionError),
    Password(PasswordError),
}

pub type RecoveryResult<T, Store, Throttle = std::convert::Infallible, Reserve = std::convert::Infallible> =
    Result<T, RecoveryError<Store, Throttle, Reserve>>;

/// The digest of a link's secret; anything malformed is simply not found.
pub fn link_digest(token: &str) -> Option<TokenDigest> {
    SessionToken::parse(token).map(|secret| secret.digest())
}

/// The throttle key for messages sent to `email` (addresses never collide with IPs).
pub fn recipient_key(email: &Email) -> String {
    email.as_str().to_lowercase()
}

fn require_admin<S, T, R>(actor: &User) -> RecoveryResult<(), S, T, R> {
    if actor.is_admin { Ok(()) } else { Err(RecoveryError::Forbidden) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_tokens_are_not_found() {
        for token in ["", "short", "!!!!", &"a".repeat(200)] {
            assert!(link_digest(token).is_none(), "{token:?}");
        }
        let secret = SessionToken::generate();
        assert_eq!(link_digest(&secret.encode()).unwrap(), secret.digest());
    }

    #[test]
    fn recipient_keys_ignore_case() {
        assert_eq!(recipient_key(&"Alice@Example.COM".parse().unwrap()), "alice@example.com");
    }
}
