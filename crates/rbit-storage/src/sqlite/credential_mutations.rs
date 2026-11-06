//! Internal transaction-bound glue for the dormant global credential writers.
use super::{
    credential_epochs::{self, EpochAllocationError},
    credential_revocations::RevocationIntent,
    pool::WriteTransaction,
};
use crate::StorageError;
use rbit_core::{AccountId, Timestamp};
use rbit_identity_models::credential::CredentialEpoch;

pub(super) async fn allocate(transaction: &mut WriteTransaction) -> Result<CredentialEpoch, StorageError> {
    credential_epochs::allocate_tracked(transaction).await.map_err(|error| match error {
        EpochAllocationError::Storage(error) => StorageError::Sqlite(error),
        EpochAllocationError::InvalidState | EpochAllocationError::Exhausted => StorageError::Corrupt(error.to_string()),
    })
}

/// Provisional only inside the owning transaction; callers return it after commit.
pub(super) async fn insert_intent(
    transaction: &mut WriteTransaction,
    account: AccountId,
    target: CredentialEpoch,
    now: Timestamp,
) -> Result<RevocationIntent, StorageError> {
    let id = sqlx::query("INSERT INTO credential_revocations (account_id, target_epoch, created_at) VALUES (?, ?, ?)")
        .bind(account.0)
        .bind(target.get())
        .bind(now.seconds())
        .execute(&mut **transaction)
        .await?
        .last_insert_rowid();
    if id <= 0 || target == CredentialEpoch::INITIAL {
        return Err(StorageError::Corrupt("credential revocation identity is invalid".into()));
    }
    Ok(RevocationIntent { id, account_id: account, target_epoch: target.get(), created_at: now })
}
