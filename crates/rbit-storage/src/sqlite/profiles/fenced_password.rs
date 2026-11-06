//! Complete dormant credential transactions; no session or finalization activation.
use super::ProfileStore;
use crate::{
    StorageError,
    sqlite::{credential_mutations, credential_revocations::RevocationIntent},
};
use rbit_core::{AccountId, Timestamp};
use rbit_identity_models::credential::CredentialEpoch;

impl ProfileStore {
    /// Administrator-authorized replacement, bound to the accepted target's original
    /// epoch. Caller authorization is still mandatory. A stale/deleted/recreated
    /// target returns None; this never adopts a fresh epoch for that target.
    pub async fn set_password_hash_with_intent(
        &self,
        account: AccountId,
        hash: &str,
        expected_epoch: CredentialEpoch,
        now: Timestamp,
    ) -> Result<Option<RevocationIntent>, StorageError> {
        self.replace_password_at_epoch(account, hash, None, expected_epoch, now).await
    }

    /// Original-password replacement: the same PHC, original epoch, active account
    /// and current pending fence must still match when the SQL writer owns the lock.
    /// A failed CAS consumes neither reset links nor a committed epoch/intent.
    pub async fn set_password_hash_if_current_with_intent(
        &self,
        account: AccountId,
        hash: &str,
        expected_hash: &str,
        expected_epoch: CredentialEpoch,
        now: Timestamp,
    ) -> Result<Option<RevocationIntent>, StorageError> {
        self.replace_password_at_epoch(account, hash, Some(expected_hash), expected_epoch, now).await
    }

    async fn replace_password_at_epoch(
        &self,
        account: AccountId,
        hash: &str,
        expected_hash: Option<&str>,
        expected_epoch: CredentialEpoch,
        now: Timestamp,
    ) -> Result<Option<RevocationIntent>, StorageError> {
        let mut tx = self.pool.begin().await?;
        // First statement takes the write lock. Provisional allocation rolls back
        // with a losing CAS or any later failure; no account epoch is refreshed.
        let target = credential_mutations::allocate(&mut tx).await?;
        if target <= expected_epoch {
            return Err(StorageError::Corrupt("credential epoch allocator is below the accepted original epoch".into()));
        }
        let changed = sqlx::query(
            "UPDATE user_logins SET password_hash=?1, password_change_required=0, authentication_epoch=?5 \
             WHERE account_id=?2 AND (?3 IS NULL OR password_hash=?3) AND authentication_epoch=?4 \
             AND (?3 IS NULL OR (suspended_at IS NULL AND NOT EXISTS \
                (SELECT 1 FROM credential_revocations r WHERE r.account_id=?2 AND r.target_epoch=?4 AND r.completed_at IS NULL)))",
        )
        .bind(hash)
        .bind(account.0)
        .bind(expected_hash)
        .bind(expected_epoch.get())
        .bind(target.get())
        .execute(&mut *tx)
        .await?
        .rows_affected()
            != 0;
        if !changed {
            return Ok(None);
        }
        let intent = credential_mutations::insert_intent(&mut tx, account, target, now).await?;
        sqlx::query("UPDATE email_tokens SET used_at = ? WHERE account_id = ? AND purpose = 'reset' AND used_at IS NULL")
            .bind(now.seconds())
            .bind(account.0)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(Some(intent))
    }
}
