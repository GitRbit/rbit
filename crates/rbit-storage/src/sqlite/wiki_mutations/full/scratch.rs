//! Bounded scratch rows under the actual preparing owner; never persistent proof.
mod cleanup;
mod edges;
mod records;
mod state;
pub use edges::{WikiScratchEdgeBatch, WikiScratchEdgeCursor};
#[cfg(test)]
mod tests;
use super::{WikiWriteError, fencing::FencedWikiRecovery, jobs};
use rbit_core::RepoId;
use rbit_wikis::{
    WikiId,
    ownership::WikiObjectOwnership,
    startup::{
        WikiStartupBatchSize,
        objects::{MAX_SCAN_RECORDS, WikiScanRecord},
        scratch::{SCOPE_CHARGE, WikiScratchLimits, WikiScratchUsage},
    },
};
use std::{fmt, future::Future, sync::Arc};

struct ScratchOwner<K> {
    recovery: FencedWikiRecovery<K>,
}
/// Consumes its recovery controller: one fresh scan per actual preparing owner.
/// Persisted phase/rows cannot reopen or reconstruct this owner after restart.
pub struct WikiScratchLedger<K> {
    owner: Arc<ScratchOwner<K>>,
}
impl<K> fmt::Debug for WikiScratchLedger<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("WikiScratchLedger(..)")
    }
}
/// A claimed scratch scope retains the real catalog and object guard.
pub struct WikiScratchScope<K> {
    wiki: WikiId,
    repository: RepoId,
    owner: Arc<ScratchOwner<K>>,
}
impl<K> fmt::Debug for WikiScratchScope<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("WikiScratchScope(..)")
    }
}
impl<K: WikiObjectOwnership> FencedWikiRecovery<K> {
    /// Call bounded cleanup to completion before consuming this controller.
    /// Existing rows require cleanup, never replay/adoption of historical proof.
    pub fn begin_scratch(
        self,
        limits: WikiScratchLimits,
    ) -> impl Future<Output = Result<WikiScratchLedger<K>, WikiWriteError>> + Send + use<K> {
        let owner = Arc::new(ScratchOwner { recovery: self });
        let held = Arc::clone(&owner);
        let job = owner.recovery.writer.start_operation(jobs::Admission::Recovery, move |pool, _catalog| async move {
            let mut tx = pool.begin_immediate().await?;
            state::epoch(&mut tx, held.recovery.epoch).await?;
            if !state::empty(&mut tx).await? { return Err(WikiWriteError::ScratchCleanupRequired) }
            state::changed(sqlx::query("UPDATE wiki_scan_control SET epoch=?1,phase='scan',max_scopes=?2,max_objects=?3,max_edges=?4,max_bytes=?5,scopes=0,objects=0,edges=0,logical_bytes=0 WHERE singleton=1")
                .bind(held.recovery.epoch).bind(i64::from(limits.scopes())).bind(i64::from(limits.objects())).bind(i64::from(limits.edges()))
                .bind(i64::try_from(limits.logical_bytes()).map_err(|_| WikiWriteError::Binding)?).execute(&mut *tx).await?.rows_affected())?;
            tx.commit().await?; Ok(())
        });
        async move {
            job?.await.map_err(|_| WikiWriteError::Uncertain)??;
            Ok(WikiScratchLedger { owner })
        }
    }
    /// Deletes at most size edges, objects and scopes each. No unbounded cascade,
    /// whole-history DELETE or VACUUM is hidden in startup or a page request.
    pub fn cleanup_scratch(&self, size: WikiStartupBatchSize) -> impl Future<Output = Result<bool, WikiWriteError>> + Send + use<K> {
        let epoch = self.epoch;
        let objects = self.objects.clone();
        let job = self.writer.start_operation(jobs::Admission::Recovery, move |pool, _catalog| async move {
            let _objects = objects;
            cleanup::batch(&pool, epoch, size).await
        });
        async move { job?.await.map_err(|_| WikiWriteError::Uncertain)? }
    }
}
impl<K: WikiObjectOwnership> WikiScratchLedger<K> {
    /// Actual retained I/O resource for the stopped static coordinator; this is
    /// not a native integrity capability and does not expose connection options.
    pub fn objects(&self) -> &K {
        self.owner.recovery.objects.objects()
    }
    /// Trusted stopped coordinator supplies catalog/native scope data, never HTTP
    /// input. No row, ID or public record becomes native authority through this API.
    pub fn scope(
        &self,
        wiki: WikiId,
        repository: RepoId,
    ) -> impl Future<Output = Result<WikiScratchScope<K>, WikiWriteError>> + Send + use<K> {
        let owner = Arc::clone(&self.owner);
        let held = Arc::clone(&owner);
        let job = self.owner.recovery.writer.start_operation(jobs::Admission::Recovery, move |pool, _catalog| async move {
            if repository.0 <= 0 { return Err(WikiWriteError::Binding) }
            let epoch = held.recovery.epoch;
            let mut tx = pool.begin_immediate().await?;
            state::active(&mut tx, epoch, "scan").await?;
            let (catalog, pending): (Option<i64>, Option<i64>) = sqlx::query_as("SELECT (SELECT repo_id FROM wiki_repositories WHERE id=?1),(SELECT repo_id FROM wiki_mutation_intents INDEXED BY wiki_mutation_one_active WHERE wiki_id=?1 AND state IN ('pending','quarantined'))")
                .bind(wiki.get()).fetch_one(&mut *tx).await?;
            if (catalog.is_none() && pending.is_none()) || catalog.is_some_and(|id| id!=repository.0) || pending.is_some_and(|id| id!=repository.0) {
                return Err(WikiWriteError::Binding);
            }
            state::charge(&mut tx, epoch, 1, 0, 0, SCOPE_CHARGE).await?;
            sqlx::query("INSERT INTO wiki_scan_scopes(epoch,wiki_id,repo_id) VALUES(?1,?2,?3)")
                .bind(epoch).bind(wiki.get()).bind(repository.0).execute(&mut *tx).await?;
            tx.commit().await?; Ok(())
        });
        async move {
            job?.await.map_err(|_| WikiWriteError::Uncertain)??;
            Ok(WikiScratchScope { wiki, repository, owner })
        }
    }
    /// SQL-only drain. Native cursors/results and escaped scope handles can still
    /// retain actual owners; this does not report a whole Wiki platform stopped.
    pub async fn close(&self) -> Result<(), WikiWriteError> {
        self.owner.recovery.close().await
    }
}
impl<K: WikiObjectOwnership> WikiScratchScope<K> {
    /// Persist BEFORE consuming the native continuation for the next batch.
    /// At most256 records are accepted atomically; errors leave quotas/rows intact.
    pub fn record(&self, batch: Vec<WikiScanRecord>) -> impl Future<Output = Result<WikiScratchUsage, WikiWriteError>> + Send + use<K> {
        let held = Arc::clone(&self.owner);
        let wiki = self.wiki;
        let repository = self.repository;
        let job = self.owner.recovery.writer.start_operation(jobs::Admission::Recovery, move |pool, _catalog| async move {
            if batch.is_empty() || batch.len() > usize::from(MAX_SCAN_RECORDS) {
                return Err(WikiWriteError::Binding);
            }
            let epoch = held.recovery.epoch;
            let mut tx = pool.begin_immediate().await?;
            state::active(&mut tx, epoch, "scan").await?;
            let open: i64 = sqlx::query_scalar("SELECT scanned FROM wiki_scan_scopes WHERE epoch=?1 AND wiki_id=?2 AND repo_id=?3")
                .bind(epoch)
                .bind(wiki.get())
                .bind(repository.0)
                .fetch_one(&mut *tx)
                .await?;
            if open != 0 {
                return Err(WikiWriteError::Binding);
            }
            for record in batch {
                records::apply(&mut tx, epoch, wiki, record).await?;
            }
            let usage = state::usage(&mut tx).await?;
            tx.commit().await?;
            Ok(usage)
        });
        async move { job?.await.map_err(|_| WikiWriteError::Uncertain)? }
    }
    /// Data-only end marker after the actual native iterator ends. SQL cannot
    /// establish native EOF from caller-provided records, and never mints a cap.
    pub fn finish(&self) -> impl Future<Output = Result<WikiScratchUsage, WikiWriteError>> + Send + use<K> {
        let held = Arc::clone(&self.owner);
        let wiki = self.wiki;
        let repository = self.repository;
        let job = self.owner.recovery.writer.start_operation(jobs::Admission::Recovery, move |pool, _catalog| async move {
            let epoch = held.recovery.epoch;
            let mut tx = pool.begin_immediate().await?;
            state::active(&mut tx, epoch, "scan").await?;
            let unfinished: i64 =
                sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM wiki_scan_objects WHERE epoch=?1 AND wiki_id=?2 AND complete=0)")
                    .bind(epoch)
                    .bind(wiki.get())
                    .fetch_one(&mut *tx)
                    .await?;
            if unfinished != 0 {
                return Err(WikiWriteError::ScratchIncomplete);
            }
            state::changed(
                sqlx::query("UPDATE wiki_scan_scopes SET scanned=1 WHERE epoch=?1 AND wiki_id=?2 AND repo_id=?3")
                    .bind(epoch)
                    .bind(wiki.get())
                    .bind(repository.0)
                    .execute(&mut *tx)
                    .await?
                    .rows_affected(),
            )?;
            let usage = state::usage(&mut tx).await?;
            tx.commit().await?;
            Ok(usage)
        });
        async move { job?.await.map_err(|_| WikiWriteError::Uncertain)? }
    }
}
