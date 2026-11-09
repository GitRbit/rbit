//! The second sign-in step and password-plus-proof confirmations.
//!
//! [`PendingSignIns`] owns the short-lived pending step that follows a correct first
//! factor. [`SecondFactorStep`] finishes it with an authenticator code, a recovery code or a
//! passkey answer, and confirms sensitive changes with the password plus a proof. It
//! decides the order of throttling, the authoritative account lookup, single-use challenge
//! and proof consumption, the one-time claim of the pending entry, and when a failure
//! counts against the account. Callers supply the throttle, the claim write mode and the
//! account lookups, and keep audit and logging.
//!
//! Receipted (original-proof) entries remain an inactive seam: they are completed only
//! through their own entry point, are never adopted by the legacy path and are not
//! selected by any route.

mod changes;
mod pending;
mod step;

pub use pending::{PendingProof, PendingSignIn, PendingSignIns};
pub use rbit_identity_models::two_factor::SecondStepAccounts;

use std::time::Duration;

use crate::accounts::ProvenAccount;
use crate::passkeys::{PasskeyRejection, webauthn::WebauthnError};
use crate::{PasswordError, User};
use rbit_contracts::AccountId;
use rbit_kernel::AdmissionError;

/// How long the second sign-in step may take.
pub const PENDING_SIGN_IN_TTL: Duration = Duration::from_mins(5);
/// Wrong factors allowed in one pending sign-in before the password must be entered again.
pub const PENDING_SIGN_IN_ATTEMPTS: u8 = 5;
/// Pending sign-ins kept at most (oldest beyond this are dropped).
pub const MAX_PENDING_SIGN_INS: usize = 10_000;

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

/// Which second factor was accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecondFactor {
    Authenticator,
    RecoveryCode,
    Passkey,
}

/// Per-account second-factor attempt limits; the step decides when each call happens.
pub trait SecondFactorThrottle: Sync {
    type Error: Send;
    fn check(&self, account: AccountId) -> Result<(), Self::Error>;
    fn record_failure(&self, account: AccountId);
    fn reset(&self, account: AccountId);
}

/// A finished second step.
#[derive(Debug)]
pub enum CompletedSignIn {
    Legacy(User),
    /// Inactive original-proof result; it must go to the fenced issuer, never the legacy one.
    Original(ProvenAccount),
}

impl CompletedSignIn {
    pub fn user(&self) -> &User {
        match self {
            Self::Legacy(user) => user,
            Self::Original(proven) => proven.user(),
        }
    }
}

/// A second step that succeeded, with what the audit needs.
#[derive(Debug)]
pub struct CompletedSecondFactor {
    pub sign_in: CompletedSignIn,
    pub factor: SecondFactor,
    /// How the first step was passed.
    pub method: String,
}

#[derive(Debug)]
pub enum SecondStepError<Store, Throttle, Claim> {
    Store(Store),
    Throttle(Throttle),
    Claim(Claim),
    /// The rejection's text is user-facing (passkeys without a public URL).
    Rejected(PasskeyRejection),
    /// A wrong, reused or refused factor. It was counted against the account; the caller
    /// audits it. `refused` carries a passkey refusal reason for logs only.
    WrongFactor {
        user: Box<User>,
        refused: Option<WebauthnError>,
    },
    /// Unknown, expired, used, cancelled or invalidated pending step.
    NotFound,
    Admission(AdmissionError),
    Password(PasswordError),
}

pub type SecondStepResult<T, Store, Throttle, Claim> = Result<T, SecondStepError<Store, Throttle, Claim>>;

pub use step::SecondFactorStep;
