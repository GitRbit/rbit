//! Compatibility delegation to the identity-owned pending sign-in owner.
use super::{AccountId, SessionToken, TwoFactorService};
use crate::accounts::ProvenAccount;
#[cfg(test)]
use {
    super::PENDING_SIGN_IN_TTL,
    crate::accounts::OriginalProofReceipt,
    rbit_identity::{
        TokenDigest,
        second_factor::{PendingProof, PendingSignIn},
    },
    std::{collections::HashMap, sync::MutexGuard, time::Instant},
};

impl TwoFactorService {
    /// Opens a pending sign-in for `account`, whose first step was `method`; external
    /// sign-in continues here too.
    pub(crate) fn start_pending(&self, account: AccountId, method: &str) -> SessionToken {
        self.pending.start(account, method)
    }

    /// Inactive seam: starts local MFA with the SAME original password proof. Current
    /// sign-in routes retain legacy pending until the full fenced mode is activated.
    /// The sign-in orchestrator checks enrollment before calling, without another query here.
    pub fn start_original_password_pending(&self, proven: ProvenAccount) -> SessionToken {
        self.pending.start_original(proven, "password")
    }

    /// Inactive provider counterpart, retaining the same proof and existing configured
    /// provider name for the second-factor audit. No enrollment lookup or new map owner.
    pub(crate) fn start_original_provider_pending(&self, proven: ProvenAccount, method: &str) -> SessionToken {
        self.pending.start_original(proven, method)
    }

    /// The account waiting for its second factor under `token`, if still valid.
    pub fn pending_account(&self, token: &SessionToken) -> Option<AccountId> {
        self.pending.account(token)
    }

    /// Abandons a pending sign-in (completed, or cancelled by the user).
    pub fn end_pending(&self, token: &SessionToken) {
        self.pending.end(token);
    }
}

/// Owner seams the pending unit tests drive directly; production completes entries only
/// through the identity second step.
#[cfg(test)]
impl TwoFactorService {
    pub(super) fn lock_pending(&self) -> MutexGuard<'_, HashMap<TokenDigest, PendingSignIn>> {
        self.pending.lock()
    }

    pub(super) fn finish_pending(&self, token: &SessionToken) -> Option<String> {
        self.pending.finish(token)
    }

    pub(super) fn pending_proof(&self, token: &SessionToken) -> Option<PendingProof> {
        self.pending.proof(token)
    }

    pub(super) fn finish_original_pending(&self, token: &SessionToken, expected: &OriginalProofReceipt) -> Option<String> {
        self.pending.finish_original(token, expected)
    }
}

#[cfg(test)]
mod tests;
