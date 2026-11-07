use super::{WikiWriteError, state};
use crate::sqlite::pool::Pool;
use rbit_wikis::startup::WikiStartupBatchSize;

pub(super) async fn batch(pool: &Pool, epoch: i64, size: WikiStartupBatchSize) -> Result<bool, WikiWriteError> {
    let mut tx = pool.begin_immediate().await?;
    state::epoch(&mut tx, epoch).await?;
    let limit = i64::from(size.get());
    sqlx::query("DELETE FROM wiki_scan_edges WHERE (epoch,wiki_id,source,ordinal) IN (SELECT epoch,wiki_id,source,ordinal FROM wiki_scan_edges ORDER BY epoch,wiki_id,source,ordinal LIMIT ?1)")
        .bind(limit).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM wiki_scan_objects WHERE (epoch,wiki_id,oid) IN (SELECT epoch,wiki_id,oid FROM wiki_scan_objects ORDER BY epoch,wiki_id,oid LIMIT ?1) AND NOT EXISTS(SELECT 1 FROM wiki_scan_edges e WHERE e.epoch=wiki_scan_objects.epoch AND e.wiki_id=wiki_scan_objects.wiki_id AND e.source=wiki_scan_objects.oid)")
        .bind(limit).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM wiki_scan_scopes WHERE (epoch,wiki_id) IN (SELECT epoch,wiki_id FROM wiki_scan_scopes ORDER BY epoch,wiki_id LIMIT ?1) AND NOT EXISTS(SELECT 1 FROM wiki_scan_objects o WHERE o.epoch=wiki_scan_scopes.epoch AND o.wiki_id=wiki_scan_scopes.wiki_id)")
        .bind(limit).execute(&mut *tx).await?;
    let empty = state::empty(&mut tx).await?;
    if empty {
        state::changed(sqlx::query("UPDATE wiki_scan_control SET epoch=0,phase='idle',max_scopes=0,max_objects=0,max_edges=0,max_bytes=0,scopes=0,objects=0,edges=0,logical_bytes=0 WHERE singleton=1")
            .execute(&mut *tx).await?.rows_affected())?;
    }
    tx.commit().await?;
    Ok(empty)
}
