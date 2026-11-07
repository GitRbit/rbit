//! Explicit stopped PREPARED-only foundation. No server/CLI activates fenced sessions.
mod layout;
pub(crate) mod model;
mod owned;
mod owner;
mod sql;
pub use model::{IdentityUpgradeError, IdentityUpgradeLimits, IdentityUpgradeSummary};
pub use owned::IdentityUpgradeFailure;
pub use owner::{PreparedIdentityPair, StoppedIdentityUpgrade};

/// Startup-only refusal BEFORE migrations/resealing; fresh/pre-0067 legacy stores
/// are allowed to migrate. No ordinary request invokes this query.
pub(crate) async fn require_legacy_sql(pool: &sqlx::SqlitePool) -> Result<(), crate::StorageError> {
    let mut connection = pool.acquire().await?;
    layout::legacy_mode(&mut connection).await.map_err(|error| crate::StorageError::Corrupt(error.to_string()))
}

#[cfg(test)]
mod tests;
