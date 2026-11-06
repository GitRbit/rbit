//! Existing one-use proof claims and dormant original-epoch guarded counterparts.
use super::super::pool::WriteTransaction;
use super::{AccountId, StorageError, Timestamp, TwoFactorStore, step_value};
use rbit_identity_models::credential::{CredentialEpoch, CredentialProofWrite};

// ?1 account, ?2 immutable pre-verification epoch. The indexed current-target
// barrier and active-login condition are evaluated by the proof's write statement.
macro_rules! fenced {
    ($statement:literal) => {
        concat!(
            $statement,
            "EXISTS (SELECT 1 FROM user_logins l WHERE l.account_id = ?1 \
            AND l.authentication_epoch = ?2 AND l.suspended_at IS NULL \
            AND NOT EXISTS (SELECT 1 FROM credential_revocations r WHERE r.account_id = l.account_id \
            AND r.target_epoch = l.authentication_epoch AND r.completed_at IS NULL))"
        )
    };
}

impl TwoFactorStore {
    /// Accepts time step `step` for an enabled account, unless it (or a later one) was
    /// already accepted. `true` means the code may be honoured.
    pub async fn claim_step(&self, account: AccountId, secret: &[u8], step: u64) -> Result<bool, StorageError> {
        let Some(stored) = self.stored_secret(&self.pool, account, secret, true).await? else {
            return Ok(false);
        };
        let result = sqlx::query(
            "UPDATE two_factor SET last_used_step = ?1 WHERE account_id = ?2 AND secret = ?3 AND confirmed_at IS NOT NULL \
             AND (last_used_step IS NULL OR last_used_step < ?1)",
        )
        .bind(step_value(step))
        .bind(account.0)
        .bind(stored)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    /// Marks an unused recovery code as used. `true` if it existed and was unused.
    pub async fn spend_recovery_code(&self, account: AccountId, digest: &[u8; 32]) -> Result<bool, StorageError> {
        let result = sqlx::query("UPDATE recovery_codes SET used_at = ? WHERE account_id = ? AND code_digest = ? AND used_at IS NULL")
            .bind(Timestamp::now().seconds())
            .bind(account.0)
            .bind(digest.as_slice())
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() == 1)
    }

    /// Records a sign-in with passkey `id`: its counter moves from `seen_count` (the value
    /// the signature was checked against) to `new_count`. `false` if another sign-in got
    /// there first, so one response can't be honoured twice.
    pub async fn use_passkey(&self, account: AccountId, id: i64, seen_count: u32, new_count: u32) -> Result<bool, StorageError> {
        let result = sqlx::query("UPDATE passkeys SET sign_count = ?, last_used_at = ? WHERE id = ? AND account_id = ? AND sign_count = ?")
            .bind(i64::from(new_count))
            .bind(Timestamp::now().seconds())
            .bind(id)
            .bind(account.0)
            .bind(i64::from(seen_count))
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() == 1)
    }

    /// Dormant original-proof claim. Secret identity/replay semantics are unchanged;
    /// successful work remains the existing secret read plus one guarded write.
    pub async fn claim_step_at_epoch(
        &self,
        account: AccountId,
        secret: &[u8],
        step: u64,
        original: CredentialEpoch,
    ) -> Result<CredentialProofWrite, StorageError> {
        let stored = self.stored_secret(&self.pool, account, secret, true).await?;
        let mut tx = self.pool.begin_immediate().await?;
        let changed = if let Some(stored) = stored {
            sqlx::query(fenced!(
                "UPDATE two_factor SET last_used_step = ?3 WHERE account_id = ?1 \
                AND secret = ?4 AND confirmed_at IS NOT NULL AND (last_used_step IS NULL OR last_used_step < ?3) AND "
            ))
            .bind(account.0)
            .bind(original.get())
            .bind(step_value(step))
            .bind(stored)
            .execute(&mut *tx)
            .await?
            .rows_affected()
                == 1
        } else {
            false
        };
        finish_claim(tx, account, original, changed).await
    }

    /// The epoch/pending/suspension predicate and recovery-code consumption share
    /// one SQLite write transaction. No later fresh epoch can rehabilitate the proof.
    pub async fn spend_recovery_code_at_epoch(
        &self,
        account: AccountId,
        digest: &[u8; 32],
        original: CredentialEpoch,
    ) -> Result<CredentialProofWrite, StorageError> {
        let now = Timestamp::now();
        let mut tx = self.pool.begin_immediate().await?;
        let changed = sqlx::query(fenced!(
            "UPDATE recovery_codes SET used_at = ?3 WHERE account_id = ?1 \
            AND code_digest = ?4 AND used_at IS NULL AND "
        ))
        .bind(account.0)
        .bind(original.get())
        .bind(now.seconds())
        .bind(digest.as_slice())
        .execute(&mut *tx)
        .await?
        .rows_affected()
            == 1;
        finish_claim(tx, account, original, changed).await
    }

    /// Counter CAS additionally requires the captured account epoch to remain active
    /// and unblocked at the exact write. Existing seen/new counter policy is retained.
    pub async fn use_passkey_at_epoch(
        &self,
        account: AccountId,
        id: i64,
        seen_count: u32,
        new_count: u32,
        original: CredentialEpoch,
    ) -> Result<CredentialProofWrite, StorageError> {
        let now = Timestamp::now();
        let mut tx = self.pool.begin_immediate().await?;
        let changed = sqlx::query(fenced!(
            "UPDATE passkeys SET sign_count = ?3, last_used_at = ?4 \
            WHERE id = ?5 AND account_id = ?1 AND sign_count = ?6 AND "
        ))
        .bind(account.0)
        .bind(original.get())
        .bind(i64::from(new_count))
        .bind(now.seconds())
        .bind(id)
        .bind(i64::from(seen_count))
        .execute(&mut *tx)
        .await?
        .rows_affected()
            == 1;
        finish_claim(tx, account, original, changed).await
    }
}

async fn finish_claim(
    mut tx: WriteTransaction,
    account: AccountId,
    original: CredentialEpoch,
    changed: bool,
) -> Result<CredentialProofWrite, StorageError> {
    let outcome = if changed {
        CredentialProofWrite::Claimed
    } else {
        // Only the rejected-write path pays for classification. BEGIN IMMEDIATE
        // retains the writer lock through this query, so a reset cannot race it.
        let current: bool = sqlx::query_scalar(fenced!("SELECT ")).bind(account.0).bind(original.get()).fetch_one(&mut *tx).await?;
        if current { CredentialProofWrite::Rejected } else { CredentialProofWrite::Invalidated }
    };
    tx.commit().await?;
    Ok(outcome)
}
