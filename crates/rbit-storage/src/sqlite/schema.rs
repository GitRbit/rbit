//! Embedded schema upgrades and planner maintenance.
use super::Database;
use crate::StorageError;

impl Database {
    /// Applies migrations embedded in the binary at compile time.
    pub async fn migrate(&self) -> Result<(), StorageError> {
        sqlx::migrate!("./migrations").run(&self.pool).await?;
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
