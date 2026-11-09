//! Authenticator-app enrolment records and static ports for second-factor state.
//!
//! Every state change that must not race (confirming enrolment, accepting a time step,
//! spending a recovery code) is one guarded write in the implementation, so two
//! concurrent requests can never both succeed with the same code.

use std::future::Future;

use crate::credential::CredentialProofWrite;
use rbit_contracts::{AccountId, Timestamp};

/// A user's authenticator row: pending until `confirmed_at` is set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TwoFactorRecord {
    pub secret: Vec<u8>,
    pub created_at: Timestamp,
    pub confirmed_at: Option<Timestamp>,
    pub last_used_step: Option<u64>,
}

impl TwoFactorRecord {
    pub fn is_enabled(&self) -> bool {
        self.confirmed_at.is_some()
    }
}

/// A recovery code's digest, bound to its owner.
pub type RecoveryDigest = [u8; 32];

pub trait TwoFactorRepository: Sync {
    type Error: std::error::Error + Send + Sync + 'static;

    fn find(&self, account: AccountId) -> impl Future<Output = Result<Option<TwoFactorRecord>, Self::Error>> + Send;
    /// Stores a pending secret, replacing an earlier pending one; `false` (no change) when
    /// the authenticator is already enabled.
    fn begin_enrolment(&self, account: AccountId, secret: &[u8]) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    /// Discards only an unconfirmed enrolment.
    fn cancel_enrolment(&self, account: AccountId) -> impl Future<Output = Result<(), Self::Error>> + Send;
    /// Enables the pending enrolment for exactly `secret` and records `step`. Installs
    /// `recovery` when this is the account's first factor (`Some(true)`), keeps existing
    /// codes otherwise (`Some(false)`); `None` when there was no matching pending enrolment.
    fn confirm(
        &self,
        account: AccountId,
        secret: &[u8],
        step: u64,
        recovery: &[RecoveryDigest],
    ) -> impl Future<Output = Result<Option<bool>, Self::Error>> + Send;
    /// Replaces every recovery code of an account with a factor; `false` without one.
    fn replace_recovery_codes(
        &self,
        account: AccountId,
        digests: &[RecoveryDigest],
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    /// Removes the authenticator (confirmed or pending), and the recovery codes too if no
    /// factor remains. `true` if anything was removed.
    fn disable(&self, account: AccountId) -> impl Future<Output = Result<bool, Self::Error>> + Send;
}

/// One-use proof claims. Implementations decide the write mode (for example a guarded
/// claim at a credential epoch). `Claimed` means the proof may be honoured, `Rejected`
/// that it was already used, and `Invalidated` that the credential changed underneath.
pub trait SecondFactorClaims: Sync {
    type Error: Send;
    fn spend_recovery_code(
        &self,
        account: AccountId,
        digest: &RecoveryDigest,
    ) -> impl Future<Output = Result<CredentialProofWrite, Self::Error>> + Send;
    /// Accepts `step` unless it, or a later one, was already accepted.
    fn claim_step(
        &self,
        account: AccountId,
        secret: &[u8],
        step: u64,
    ) -> impl Future<Output = Result<CredentialProofWrite, Self::Error>> + Send;
}

/// Authoritative account lookups for the second sign-in step and for confirmations.
pub trait SecondStepAccounts: Sync {
    type Error: std::error::Error + Send + Sync + 'static;
    /// An active (not suspended) user.
    fn find_active_user(&self, account: AccountId) -> impl Future<Output = Result<Option<crate::User>, Self::Error>> + Send;
    /// The user only while it is active at exactly the `original` credential epoch.
    fn find_user_at_epoch(
        &self,
        account: AccountId,
        original: crate::credential::CredentialEpoch,
    ) -> impl Future<Output = Result<Option<crate::User>, Self::Error>> + Send;
    /// The stored password hash for a confirmation, or `None` without a login.
    fn password_hash(&self, account: AccountId) -> impl Future<Output = Result<Option<String>, Self::Error>> + Send;
}
