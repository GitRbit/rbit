//! Private ACTIVE store acceptance, not a public activation switch or runtime API.
mod kv;
mod pool;
mod sql;
#[cfg(test)]
mod tests;
use super::{model::Checkpoint, owner::StoppedIdentityUpgrade};
use crate::identity_upgrade::IdentityUpgradeError as Error;
use sqlx::Connection;
use std::sync::Arc;

/// Construction requires real SQL/KV validation under the retained exclusive owner.
/// No caller-supplied boolean, parsed marker or User can construct this capability.
pub(crate) struct ActiveIdentityPair {
    owner: StoppedIdentityUpgrade,
    checkpoint: Checkpoint,
}
impl ActiveIdentityPair {
    pub(crate) fn runtime_parts(&self) -> (&Arc<redb::Database>, Checkpoint) {
        (&self.owner.db, self.checkpoint)
    }
    fn session_store(&self) -> crate::SessionStore {
        crate::kv::sessions::epoch::runtime::sessions(self)
    }
}
impl std::fmt::Debug for ActiveIdentityPair {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("ActiveIdentityPair").finish_non_exhaustive()
    }
}
impl StoppedIdentityUpgrade {
    /// Run only in the existing owned-operation task when production selects this
    /// seam. These inner functions do not themselves establish cancellation/drain
    /// ownership; the outer operation must move the actual owner before awaiting.
    async fn publish_active_inner(&mut self) -> Result<Checkpoint, Error> {
        let mut tx = self.connection.begin_with("BEGIN IMMEDIATE").await?;
        sql::layout(&mut tx).await?;
        let checkpoint = match sql::publication(&mut *tx).await? {
            None => {
                let super::sql::State::Prepared(checkpoint) = super::sql::state(&mut *tx).await? else {
                    return Err(Error::NotPrepared);
                };
                super::sql::within_limits(&mut tx, self.limits).await?;
                super::sql::verify_accounts(&mut tx, checkpoint).await?;
                let db = Arc::clone(&self.db);
                let limits = self.limits;
                tokio::task::spawn_blocking(move || kv::verify_prepared(&db, checkpoint, limits)).await??;
                pool::bind_writer(&mut tx, Arc::clone(&self.db)).await?;
                sql::publish(&mut tx, checkpoint).await?;
                checkpoint
            }
            Some(checkpoint) => {
                sql::match_prepared(&mut tx, checkpoint).await?;
                // Both active is restart, not an opportunity to check an old empty
                // state or to clear/allocate again after legitimate runtime writes.
                let db = Arc::clone(&self.db);
                let limits = self.limits;
                if tokio::task::spawn_blocking(move || kv::is_active(&db, checkpoint, limits)).await?? {
                    runtime_history(&mut tx, Arc::clone(&self.db), checkpoint, self.limits).await?;
                    tx.commit().await?;
                    return Ok(checkpoint);
                }
                super::sql::within_limits(&mut tx, self.limits).await?;
                super::sql::verify_accounts(&mut tx, checkpoint).await?;
                checkpoint
            }
        };
        tx.commit().await?;
        // Deliberately not cross-store atomic. This exact SQL-active/KV-prepared
        // cut is resumable; no public runtime capability has escaped.
        let db = Arc::clone(&self.db);
        let limits = self.limits;
        tokio::task::spawn_blocking(move || kv::publish(&db, checkpoint, limits)).await??;
        Ok(checkpoint)
    }
    async fn accept_active_inner(mut self) -> Result<ActiveIdentityPair, Error> {
        let mut tx = self.connection.begin_with("BEGIN IMMEDIATE").await?;
        sql::layout(&mut tx).await?;
        let checkpoint = sql::publication(&mut *tx).await?.ok_or(Error::Mismatch)?;
        sql::match_prepared(&mut tx, checkpoint).await?;
        let db = Arc::clone(&self.db);
        let limits = self.limits;
        if !tokio::task::spawn_blocking(move || kv::is_active(&db, checkpoint, limits)).await?? {
            return Err(Error::Mismatch);
        }
        runtime_history(&mut tx, Arc::clone(&self.db), checkpoint, self.limits).await?;
        tx.commit().await?;
        Ok(ActiveIdentityPair { owner: self, checkpoint })
    }
}
async fn runtime_history(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    db: Arc<redb::Database>,
    checkpoint: Checkpoint,
    limits: super::IdentityUpgradeLimits,
) -> Result<(), Error> {
    super::sql::within_limits(tx, limits).await?;
    let history = super::sql::history(&mut **tx).await?;
    let counter: i64 =
        sqlx::query_scalar("SELECT high_water FROM credential_epoch_allocator WHERE singleton=1").fetch_one(&mut **tx).await?;
    if counter < history.floor || counter < checkpoint.final_floor()? {
        return Err(Error::Mismatch);
    }
    let floor = tokio::task::spawn_blocking(move || crate::kv::sessions::epoch::stopped::active_floor(&db, limits)).await??;
    if floor > counter {
        return Err(Error::Mismatch);
    }
    Ok(())
}
