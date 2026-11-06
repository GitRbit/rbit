//! Dormant global allocation primitive. Production writers/readers are NOT activated.
//!
//! Returned epochs are provisional until the caller commits. Global nonreuse for user
//! credentials additionally requires accepted startup floors and exclusive epoch-aware
//! writers; legacy stage/creation APIs intentionally remain unchanged in this atom.
use rbit_identity_models::credential::CredentialEpoch;
use sqlx::{Sqlite, Transaction};

#[derive(Debug, thiserror::Error)]
pub enum EpochAllocationError {
    #[error("credential epoch allocator state is invalid; preserve both stores and reconcile offline, never reset the counter")]
    InvalidState,
    #[error("credential epoch space is exhausted; stop activation and preserve both stores, never wrap or reset the counter")]
    Exhausted,
    #[error("credential epoch allocation storage failure: {0}")]
    Storage(#[from] sqlx::Error),
}

/// Exactly one statement on valid state, in the SAME outer transaction as the future
/// creation/password/intent writes. Never commits, queries accounts, resets state or
/// treats a bare allocated fact as authentication/issuance authority.
///
/// Supported composition must first reconcile SQL and all observed redb epoch floors
/// under exclusive writers. Do not mix global values with legacy local increments.
pub async fn allocate_in(transaction: &mut Transaction<'_, Sqlite>) -> Result<CredentialEpoch, EpochAllocationError> {
    let allocated: Option<i64> = sqlx::query_scalar(
        "UPDATE credential_epoch_allocator SET high_water = high_water + 1 \
         WHERE rowid = 1 AND singleton = 1 AND typeof(singleton) = 'integer' \
         AND typeof(high_water) = 'integer' AND high_water >= 0 AND high_water < 9223372036854775807 \
         AND (SELECT COUNT(*) FROM (SELECT 1 FROM credential_epoch_allocator LIMIT 2)) = 1 \
         RETURNING high_water",
    )
    .fetch_optional(&mut **transaction)
    .await?;
    if let Some(allocated) = allocated {
        return CredentialEpoch::new(allocated).ok_or(EpochAllocationError::InvalidState);
    }
    Err(state_failure(transaction).await?)
}

/// Startup/restore primitive: only raises an accepted observed floor, never lowers it.
/// The argument is a DATA fact, not proof of a complete scan or authority to activate.
/// The private startup owner must supply the maximum of ALL accepted SQL/redb evidence
/// under exclusive writers. Passing one sample or editable JSON is not readiness.
pub async fn raise_floor_in(
    transaction: &mut Transaction<'_, Sqlite>,
    observed: CredentialEpoch,
) -> Result<CredentialEpoch, EpochAllocationError> {
    if observed.get() == i64::MAX {
        return Err(EpochAllocationError::Exhausted);
    }
    let raised: Option<i64> = sqlx::query_scalar(
        "UPDATE credential_epoch_allocator SET high_water = MAX(high_water, ?1) \
         WHERE rowid = 1 AND singleton = 1 AND typeof(singleton) = 'integer' \
         AND typeof(high_water) = 'integer' AND high_water >= 0 AND high_water < 9223372036854775807 \
         AND (SELECT COUNT(*) FROM (SELECT 1 FROM credential_epoch_allocator LIMIT 2)) = 1 \
         RETURNING high_water",
    )
    .bind(observed.get())
    .fetch_optional(&mut **transaction)
    .await?;
    match raised {
        Some(value) => CredentialEpoch::new(value).ok_or(EpochAllocationError::InvalidState),
        None => Err(state_failure(transaction).await?),
    }
}

async fn state_failure(transaction: &mut Transaction<'_, Sqlite>) -> Result<EpochAllocationError, sqlx::Error> {
    // Bounded diagnostic path only; malformed/missing state is never auto-created.
    let rows: Vec<i64> = sqlx::query_scalar(
        "SELECT CASE WHEN typeof(singleton) = 'integer' AND singleton = 1 \
         AND typeof(high_water) = 'integer' AND high_water = 9223372036854775807 \
         THEN 1 ELSE 0 END FROM credential_epoch_allocator LIMIT 2",
    )
    .fetch_all(&mut **transaction)
    .await?;
    Ok(match rows.as_slice() {
        [1] => EpochAllocationError::Exhausted,
        _ => EpochAllocationError::InvalidState,
    })
}
