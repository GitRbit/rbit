//! Claim write adapters: legacy claims, and the inactive original-epoch claims selected
//! only for receipted pending sign-ins.
use super::{AccountId, TwoFactorStore};
use crate::accounts::OriginalProofReceipt;
use rbit_identity::passkeys::PasskeyClaims;
use rbit_identity::two_factor::{CredentialProofWrite, RecoveryDigest, SecondFactorClaims};
use rbit_identity_models::credential::CredentialEpoch;
use rbit_storage::StorageError;

/// Which write a proof claim uses.
#[derive(Debug, Clone, Copy)]
enum Mode {
    Legacy,
    /// Guarded by the receipt's immutable pre-verification epoch.
    Original(CredentialEpoch),
}

/// The identity claim ports over the two-factor store in one write mode.
pub(super) struct Claims<'a> {
    store: &'a TwoFactorStore,
    mode: Mode,
}

impl<'a> Claims<'a> {
    pub(super) fn legacy(store: &'a TwoFactorStore) -> Self {
        Self { store, mode: Mode::Legacy }
    }

    pub(super) fn for_receipt(store: &'a TwoFactorStore, original: Option<&OriginalProofReceipt>) -> Self {
        let mode = original.map_or(Mode::Legacy, |receipt| Mode::Original(receipt.original_epoch()));
        Self { store, mode }
    }
}

fn legacy(claimed: bool) -> CredentialProofWrite {
    if claimed { CredentialProofWrite::Claimed } else { CredentialProofWrite::Rejected }
}

impl SecondFactorClaims for Claims<'_> {
    type Error = StorageError;
    async fn spend_recovery_code(&self, account: AccountId, digest: &RecoveryDigest) -> Result<CredentialProofWrite, StorageError> {
        match self.mode {
            Mode::Legacy => Ok(legacy(self.store.spend_recovery_code(account, digest).await?)),
            Mode::Original(epoch) => self.store.spend_recovery_code_at_epoch(account, digest, epoch).await,
        }
    }
    async fn claim_step(&self, account: AccountId, secret: &[u8], step: u64) -> Result<CredentialProofWrite, StorageError> {
        match self.mode {
            Mode::Legacy => Ok(legacy(self.store.claim_step(account, secret, step).await?)),
            Mode::Original(epoch) => self.store.claim_step_at_epoch(account, secret, step, epoch).await,
        }
    }
}

impl PasskeyClaims for Claims<'_> {
    type Error = StorageError;
    async fn use_passkey(&self, account: AccountId, id: i64, seen: u32, next: u32) -> Result<CredentialProofWrite, StorageError> {
        match self.mode {
            Mode::Legacy => Ok(legacy(self.store.use_passkey(account, id, seen, next).await?)),
            Mode::Original(epoch) => self.store.use_passkey_at_epoch(account, id, seen, next, epoch).await,
        }
    }
}
