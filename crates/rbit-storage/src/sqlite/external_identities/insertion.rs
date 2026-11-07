//! Shared identity insertion: legacy VALUES SQL and fixed-origin guarded SELECT.
use super::{ExternalIdentity, ExternalIdentityStore, IdentityRow, LinkConflict, NewExternalIdentity};
use crate::{StorageError, sqlite::pool::WriteTransaction};
use rbit_core::{AccountId, Timestamp};
use rbit_identity_models::credential::CredentialEpoch;

#[derive(Debug)]
pub enum GuardedIdentityLink {
    Linked(ExternalIdentity),
    Conflict(LinkConflict),
    Unavailable,
}
impl ExternalIdentityStore {
    /// Inactive fixed-origin credential addition. Epoch, pending and suspension
    /// are checked by the SAME INSERT while SQLite owns its write transaction.
    pub async fn link_at_epoch(
        &self,
        account: AccountId,
        identity: NewExternalIdentity<'_>,
        at: Timestamp,
        original: CredentialEpoch,
    ) -> Result<GuardedIdentityLink, StorageError> {
        if original == CredentialEpoch::INITIAL {
            return Ok(GuardedIdentityLink::Unavailable);
        }
        let mut tx = self.pool.begin().await?;
        match insert_inner(&mut tx, account, identity, at, Some(original)).await? {
            Some(Ok(linked)) => {
                tx.commit().await?;
                Ok(GuardedIdentityLink::Linked(linked))
            }
            Some(Err(conflict)) => Ok(GuardedIdentityLink::Conflict(conflict)),
            None => Ok(GuardedIdentityLink::Unavailable),
        }
    }
}

pub(super) async fn insert(
    tx: &mut WriteTransaction,
    account: AccountId,
    identity: NewExternalIdentity<'_>,
    at: Timestamp,
) -> Result<Result<ExternalIdentity, LinkConflict>, StorageError> {
    insert_inner(tx, account, identity, at, None).await?.ok_or(sqlx::Error::RowNotFound.into())
}

macro_rules! insertion {
    ($values:literal) => {
        concat!(
            "INSERT INTO external_identities (account_id, provider_id, subject, email, username, created_at, last_used_at) \
        ",
            $values,
            " RETURNING id, account_id, provider_id, subject, email, username, created_at, last_used_at"
        )
    };
}
async fn insert_inner(
    tx: &mut WriteTransaction,
    account: AccountId,
    identity: NewExternalIdentity<'_>,
    at: Timestamp,
    original: Option<CredentialEpoch>,
) -> Result<Option<Result<ExternalIdentity, LinkConflict>>, StorageError> {
    let query = sqlx::query_as(if original.is_some() {
        sqlx::SqlStr::from_static(insertion!(
            "SELECT ?1, ?2, ?3, ?4, ?5, ?6, ?7 WHERE EXISTS (SELECT 1 FROM user_logins l \
            WHERE l.account_id=?1 AND l.authentication_epoch=?8 AND l.suspended_at IS NULL \
            AND NOT EXISTS (SELECT 1 FROM credential_revocations r WHERE r.account_id=l.account_id \
            AND r.target_epoch=l.authentication_epoch AND r.completed_at IS NULL))"
        ))
    } else {
        sqlx::SqlStr::from_static(insertion!("VALUES (?, ?, ?, ?, ?, ?, ?)"))
    })
    .bind(account.0)
    .bind(identity.provider_id)
    .bind(identity.subject)
    .bind(identity.email)
    .bind(identity.username)
    .bind(at.0)
    .bind(at.0);
    let query = if let Some(original) = original { query.bind(original.get()) } else { query };
    let inserted: Result<Option<IdentityRow>, sqlx::Error> = query.fetch_optional(&mut **tx).await;
    match inserted {
        Ok(row) => Ok(row.map(|row| Ok(row.into()))),
        Err(sqlx::Error::Database(error)) if error.is_unique_violation() => {
            let taken: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM external_identities WHERE provider_id = ? AND subject = ?)")
                .bind(identity.provider_id)
                .bind(identity.subject)
                .fetch_one(&mut **tx)
                .await?;
            Ok(Some(Err(if taken { LinkConflict::IdentityTaken } else { LinkConflict::ProviderTaken })))
        }
        Err(error) => Err(error.into()),
    }
}
