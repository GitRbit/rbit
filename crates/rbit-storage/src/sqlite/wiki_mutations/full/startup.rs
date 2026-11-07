//! Bounded indexed SQL scope enumeration, not completed native startup validation.
mod merge;
#[cfg(test)]
mod tests;

use super::{
    FullWikiWriter, Pool, WikiWriteError,
    fencing::{FencedWikiRecovery, WikiRecoveryObjects},
    jobs,
};
use rbit_wikis::{
    WikiId,
    ownership::WikiObjectOwnership,
    startup::{WikiStartupBatchSize, WikiStartupScope},
};
use std::{fmt, future::Future, sync::Arc};

/// In-process only. The real owner/root and epoch remain retained; no public ID,
/// marker or deserialized cursor can manufacture a continuation for another owner.
pub struct WikiStartupCursor<K> {
    after: WikiId,
    epoch: i64,
    writer: FullWikiWriter,
    objects: WikiRecoveryObjects<K>,
}
impl<K> fmt::Debug for WikiStartupCursor<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("WikiStartupCursor(..)")
    }
}
pub struct WikiStartupBatch<K> {
    scopes: Vec<WikiStartupScope>,
    next: Option<WikiStartupCursor<K>>,
}
impl<K> fmt::Debug for WikiStartupBatch<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("WikiStartupBatch(..)")
    }
}
impl<K> WikiStartupBatch<K> {
    pub fn scopes(&self) -> &[WikiStartupScope] {
        &self.scopes
    }
    pub fn into_parts(self) -> (Vec<WikiStartupScope>, Option<WikiStartupCursor<K>>) {
        (self.scopes, self.next)
    }
}

#[derive(sqlx::FromRow)]
struct CatalogRow {
    wiki: i64,
    repository: i64,
    enabled: i64,
    generation: Option<i64>,
    revision: Option<String>,
}
#[derive(Debug, PartialEq, Eq, sqlx::FromRow)]
struct PendingRow {
    wiki: i64,
    repository: i64,
}

const CATALOG: &str = "SELECT w.id AS wiki,w.repo_id AS repository,w.enabled,w.generation,CASE WHEN octet_length(v.revision)<=128 THEN v.revision ELSE NULL END AS revision FROM wiki_repositories w LEFT JOIN wiki_revisions v ON v.repo_id=w.repo_id AND v.generation=w.generation WHERE w.id>?1 ORDER BY w.id LIMIT ?2";
const PENDING: &str = "SELECT wiki_id AS wiki,repo_id AS repository FROM wiki_mutation_intents INDEXED BY wiki_mutation_one_active WHERE state IN ('pending','quarantined') AND wiki_id>?1 ORDER BY wiki_id LIMIT ?2";

async fn read_rows(
    pool: &Pool,
    epoch: i64,
    after: i64,
    size: WikiStartupBatchSize,
) -> Result<(Vec<CatalogRow>, Vec<PendingRow>), WikiWriteError> {
    let mut tx = pool.begin().await?;
    let (mode, current): (String, i64) =
        sqlx::query_as("SELECT mode,epoch FROM wiki_writer_protocol WHERE singleton=1").fetch_one(&mut *tx).await?;
    if mode != "preparing" || current != epoch {
        return Err(WikiWriteError::Binding);
    }
    let maximum = i64::from(size.get()) + 1;
    let catalog = sqlx::query_as(CATALOG).bind(after).bind(maximum).fetch_all(&mut *tx).await?;
    let pending = sqlx::query_as(PENDING).bind(after).bind(maximum).fetch_all(&mut *tx).await?;
    tx.commit().await?;
    Ok((catalog, pending))
}

impl<K: WikiObjectOwnership> FencedWikiRecovery<K> {
    /// Includes disabled catalogs and SQL orphans; native-only orphan directories,
    /// all historical closure, platform fencing and map readiness still follow.
    /// Up to two limit+1 source arrays and one limit output array are held, never
    /// a full-catalog UNION/sort or an unbounded in-memory object identity set.
    pub fn startup_batch(
        &self,
        cursor: Option<WikiStartupCursor<K>>,
        size: WikiStartupBatchSize,
    ) -> impl Future<Output = Result<WikiStartupBatch<K>, WikiWriteError>> + Send + use<K> {
        let epoch = self.epoch;
        let writer = self.writer.clone();
        let objects = self.objects.clone();
        let valid = cursor
            .as_ref()
            .is_none_or(|cursor| cursor.epoch == epoch && Arc::ptr_eq(&cursor.writer.0, &writer.0) && cursor.objects.same_owner(&objects));
        let after = cursor.as_ref().map_or(0, |cursor| cursor.after.get());
        // Keep the consumed cursor/actual root alive through the admitted job.
        let job = if valid {
            self.writer.start_operation(jobs::Admission::Recovery, move |pool, _catalog| async move {
                let _cursor = cursor;
                let (catalog, pending) = read_rows(&pool, epoch, after, size).await?;
                let (scopes, more) = merge::scopes(catalog, pending, usize::from(size.get()))?;
                let next = if more {
                    Some(WikiStartupCursor { after: scopes.last().ok_or(WikiWriteError::Binding)?.wiki, epoch, writer, objects })
                } else {
                    None
                };
                Ok(WikiStartupBatch { scopes, next })
            })
        } else {
            Err(WikiWriteError::Binding)
        };
        async move { job?.await.map_err(|_| WikiWriteError::Uncertain)? }
    }
}
