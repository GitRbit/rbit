//! Shared reset eligibility/consume transaction with an inactive global-intent mode.
use super::{EmailTokenRecord, RecoveryStore, TokenPurpose, TokenRow};
use crate::{
    StorageError,
    sqlite::{credential_mutations, credential_revocations::RevocationIntent},
};
use rbit_core::Timestamp;

/// Committed database facts, not proof of completed redb revocation or authentication.
#[derive(Debug)]
pub struct PasswordResetMutation {
    pub record: EmailTokenRecord,
    pub intent: RevocationIntent,
}
struct ResetCompletion {
    record: EmailTokenRecord,
    intent: Option<RevocationIntent>,
}

impl RecoveryStore {
    /// Redeems a reset link, replaces the password, verifies its address and revokes
    /// every other reset link in one transaction. Concurrent links cannot both win,
    /// and email changes or suspension before the write invalidate the reset.
    pub async fn reset_password(
        &self,
        digest: &[u8; 32],
        password_hash: &str,
        now: Timestamp,
    ) -> Result<Option<EmailTokenRecord>, StorageError> {
        Ok(self.reset_password_mode::<false>(digest, password_hash, now).await?.map(|completed| completed.record))
    }

    /// Dormant full transaction: reset consume, replacement, global epoch and pending
    /// intent commit together. No production service calls this before full protocol
    /// activation. The returned intent does NOT acknowledge session cleanup.
    pub async fn reset_password_with_intent(
        &self,
        digest: &[u8; 32],
        password_hash: &str,
        now: Timestamp,
    ) -> Result<Option<PasswordResetMutation>, StorageError> {
        self.reset_password_mode::<true>(digest, password_hash, now)
            .await?
            .map(|completed| {
                let intent = completed.intent.ok_or_else(|| StorageError::Corrupt("credential reset intent is missing".into()))?;
                Ok(PasswordResetMutation { record: completed.record, intent })
            })
            .transpose()
    }

    async fn reset_password_mode<const ORIGINAL: bool>(
        &self,
        digest: &[u8; 32],
        password_hash: &str,
        now: Timestamp,
    ) -> Result<Option<ResetCompletion>, StorageError> {
        let mut tx = self.pool.begin().await?;
        // SAME eligibility and one-use statement in both modes, before allocation.
        let row: Option<TokenRow> = sqlx::query_as(concat!(
            "UPDATE email_tokens AS t SET used_at = ?1 WHERE ",
            honoured!(),
            " AND EXISTS (SELECT 1 FROM user_logins l WHERE l.account_id = t.account_id AND l.suspended_at IS NULL) \
             RETURNING account_id, email, expires_at"
        ))
        .bind(now.seconds())
        .bind(digest.as_slice())
        .bind(TokenPurpose::Reset.as_str())
        .fetch_optional(&mut *tx)
        .await?;
        let Some(row) = row else { return Ok(None) };
        let record = EmailTokenRecord::try_from(row)?;
        let intent = if ORIGINAL {
            let target = credential_mutations::allocate(&mut tx).await?;
            let changed = sqlx::query(
                "UPDATE user_logins SET password_hash=?1, password_change_required=0, email_verified_at=?2, authentication_epoch=?4 \
                 WHERE account_id=?3 AND authentication_epoch<?4",
            )
            .bind(password_hash)
            .bind(now.seconds())
            .bind(record.account_id.0)
            .bind(target.get())
            .execute(&mut *tx)
            .await?
            .rows_affected();
            if changed != 1 {
                return Err(StorageError::Corrupt("credential reset epoch advance refused".into()));
            }
            Some(credential_mutations::insert_intent(&mut tx, record.account_id, target, now).await?)
        } else {
            sqlx::query(
                "UPDATE user_logins SET password_hash = ?, password_change_required = 0, email_verified_at = ? WHERE account_id = ?",
            )
            .bind(password_hash)
            .bind(now.seconds())
            .bind(record.account_id.0)
            .execute(&mut *tx)
            .await?;
            None
        };
        sqlx::query("UPDATE email_tokens SET used_at = ? WHERE account_id = ? AND purpose = 'reset' AND used_at IS NULL")
            .bind(now.seconds())
            .bind(record.account_id.0)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(Some(ResetCompletion { record, intent }))
    }
}
