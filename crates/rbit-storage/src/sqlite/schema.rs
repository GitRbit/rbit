//! Embedded schema upgrades and planner maintenance.
use super::Database;
use crate::StorageError;

impl Database {
    /// Applies migrations embedded in the binary at compile time.
    pub async fn migrate(&self) -> Result<(), StorageError> {
        #[cfg(test)]
        self.protocol.require_legacy()?;
        crate::identity_upgrade::require_legacy_sql(&self.pool).await?;
        refuse_newer(&self.pool).await?;
        sqlx::migrate!("./migrations").run(&self.pool).await?;
        crate::identity_upgrade::require_legacy_sql(&self.pool).await?;
        Ok(())
    }

    /// Refreshes the query planner's table statistics where they are missing or out of
    /// date (`PRAGMA optimize`), so it can tell a selective index from a poor one. Run
    /// after migrations (new indexes) and from periodic maintenance: `SQLite` only
    /// analyses tables whose size changed a lot since their last analysis, with a bounded
    /// sample, so it usually does nothing. `0x10002` considers every table, not only those
    /// the pooled connection it happens to run on has queried.
    pub async fn optimize(&self) -> Result<(), StorageError> {
        sqlx::query("PRAGMA optimize = 0x10002").execute(&self.pool).await?;
        Ok(())
    }
}

/// Fails with [`StorageError::NewerSchema`], before anything is written, when the database
/// records migrations this binary does not embed (a newer rbit migrated it). sqlx would
/// refuse too, but only as "migration N was previously applied but is missing".
pub(super) async fn refuse_newer(pool: &sqlx::SqlitePool) -> Result<(), StorageError> {
    let has_table: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = '_sqlx_migrations')")
            .fetch_one(pool)
            .await?;
    if !has_table {
        return Ok(());
    }
    let recorded: Vec<i64> = sqlx::query_scalar("SELECT version FROM _sqlx_migrations ORDER BY version").fetch_all(pool).await?;
    let known = super::snapshot::embedded_migrations();
    let unknown: Vec<i64> = recorded.into_iter().filter(|version| !known.contains(version)).collect();
    if unknown.is_empty() { Ok(()) } else { Err(StorageError::NewerSchema(unknown)) }
}
