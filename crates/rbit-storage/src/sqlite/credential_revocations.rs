//! Inactive persistence foundation for credential/session fencing.
//!
//! These functions enlist in the caller's explicit SQL transaction; they never commit
//! the caller's credential/proof changes. Existing password writers and session readers
//! do not call them yet. An intent is a database fact, not authentication evidence.
//! No function here performs redb work or proves that an acknowledged effect occurred.

use std::num::NonZeroU8;

use rbit_core::{AccountId, Timestamp};
use sqlx::{Acquire, Sqlite, Transaction};

use crate::StorageError;

/// Provisional until the caller commits; not a receipt on rollback/error.
/// Committed IDs are never reused by normal insertion, even after completed-row deletion.
/// Do not modify `sqlite_sequence` or manually assign IDs; rolled-back IDs are not receipts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RevocationIntent {
    pub id: i64,
    pub account_id: AccountId,
    pub target_epoch: i64,
    pub created_at: Timestamp,
}

/// The required decision recorded by a future validated reconciliation adapter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RevocationOutcome {
    KeptCurrent,
    EndedAll,
}
impl RevocationOutcome {
    const fn as_str(self) -> &'static str {
        match self {
            Self::KeptCurrent => "kept_current",
            Self::EndedAll => "ended_all",
        }
    }
}

/// Completion never grants permission to replay cleanup or promote a session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Completion {
    Applied,
    AlreadyApplied,
    NotCurrent,
}

/// Advance the checked account epoch and insert its pending intent in the SAME caller
/// transaction as the password/proof writes. A savepoint also prevents a failed insert
/// leaving a staged epoch if the caller handles the error. On any error the caller
/// should abort its entire credential transaction. This is not a password-change API.
pub async fn stage(
    transaction: &mut Transaction<'_, Sqlite>,
    account_id: AccountId,
    created_at: Timestamp,
) -> Result<RevocationIntent, StorageError> {
    let mut savepoint = transaction.begin().await?;
    let epoch: Option<i64> = sqlx::query_scalar(
        "UPDATE user_logins SET authentication_epoch = authentication_epoch + 1 \
         WHERE account_id = ? AND authentication_epoch < 9223372036854775807 \
         RETURNING authentication_epoch",
    )
    .bind(account_id.0)
    .fetch_optional(&mut *savepoint)
    .await?;
    let epoch = epoch.ok_or_else(|| StorageError::Corrupt("credential epoch cannot advance".into()))?;
    let id = sqlx::query("INSERT INTO credential_revocations (account_id, target_epoch, created_at) VALUES (?, ?, ?)")
        .bind(account_id.0)
        .bind(epoch)
        .bind(created_at.seconds())
        .execute(&mut *savepoint)
        .await?
        .last_insert_rowid();
    savepoint.commit().await?;
    Ok(RevocationIntent { id, account_id, target_epoch: epoch, created_at })
}

/// Bounded, stable ID pagination. Keep a sweep cursor only for that sweep; restart at
/// zero on recovery/retry so unresolved earlier IDs are not silently skipped forever.
pub async fn pending(
    transaction: &mut Transaction<'_, Sqlite>,
    after_id: i64,
    limit: NonZeroU8,
) -> Result<Vec<RevocationIntent>, StorageError> {
    let rows: Vec<(i64, i64, i64, i64)> = sqlx::query_as(
        "SELECT id, account_id, target_epoch, created_at FROM credential_revocations \
         WHERE completed_at IS NULL AND id > ? ORDER BY id LIMIT ?",
    )
    .bind(after_id)
    .bind(i64::from(limit.get()))
    .fetch_all(&mut **transaction)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, account, target_epoch, at)| RevocationIntent {
            id,
            account_id: AccountId(account),
            target_epoch,
            created_at: Timestamp(at),
        })
        .collect())
}

/// Acknowledge only this exact pending intent while its fixed target epoch is current.
/// A later change is never acknowledged or cleared. The caller must first prove the
/// matching redb decision, retain tracked ownership and obey the full protocol. In
/// particular recovery after a partial keep requires an additional epoch advance;
/// this operation cannot implement recovery by simply clearing a pending barrier.
pub async fn complete(
    transaction: &mut Transaction<'_, Sqlite>,
    intent: &RevocationIntent,
    outcome: RevocationOutcome,
    completed_at: Timestamp,
) -> Result<Completion, StorageError> {
    let changed = sqlx::query(
        "UPDATE credential_revocations SET outcome = ?1, completed_at = ?2 \
         WHERE id = ?3 AND account_id = ?4 AND target_epoch = ?5 AND completed_at IS NULL \
         AND EXISTS (SELECT 1 FROM user_logins WHERE account_id = ?4 AND authentication_epoch = ?5)",
    )
    .bind(outcome.as_str())
    .bind(completed_at.seconds())
    .bind(intent.id)
    .bind(intent.account_id.0)
    .bind(intent.target_epoch)
    .execute(&mut **transaction)
    .await?
    .rows_affected();
    if changed != 0 {
        return Ok(Completion::Applied);
    }
    let applied: Option<String> = sqlx::query_scalar(
        "SELECT outcome FROM credential_revocations WHERE id = ? AND account_id = ? \
         AND target_epoch = ? AND completed_at IS NOT NULL",
    )
    .bind(intent.id)
    .bind(intent.account_id.0)
    .bind(intent.target_epoch)
    .fetch_optional(&mut **transaction)
    .await?;
    Ok(if applied.as_deref() == Some(outcome.as_str()) { Completion::AlreadyApplied } else { Completion::NotCurrent })
}
