//! Keyset edge-kind checks against already-complete unique object rows.
use super::{ScratchOwner, WikiScratchLedger, WikiWriteError, jobs, state};
use rbit_wikis::{ownership::WikiObjectOwnership, startup::WikiStartupBatchSize};
use std::{fmt, future::Future, sync::Arc};

pub struct WikiScratchEdgeCursor<K> {
    wiki: i64,
    source: [u8; 20],
    ordinal: i64,
    owner: Arc<ScratchOwner<K>>,
}
impl<K> fmt::Debug for WikiScratchEdgeCursor<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("WikiScratchEdgeCursor(..)")
    }
}
pub struct WikiScratchEdgeBatch<K> {
    checked: u16,
    next: Option<WikiScratchEdgeCursor<K>>,
}
impl<K> fmt::Debug for WikiScratchEdgeBatch<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("WikiScratchEdgeBatch(..)")
    }
}
impl<K> WikiScratchEdgeBatch<K> {
    pub const fn checked(&self) -> u16 {
        self.checked
    }
    /// None means only that indexed SQL edge work ended, never native readiness.
    pub fn into_next(self) -> Option<WikiScratchEdgeCursor<K>> {
        self.next
    }
}
#[derive(sqlx::FromRow)]
struct EdgeRow {
    wiki_id: i64,
    source: Vec<u8>,
    ordinal: i64,
    expected_kind: i64,
    actual_kind: Option<i64>,
    target_complete: Option<i64>,
    source_complete: Option<i64>,
}
const EDGES: &str = "SELECT e.wiki_id,e.source,e.ordinal,e.expected_kind,t.kind AS actual_kind,t.complete AS target_complete,s.complete AS source_complete FROM wiki_scan_edges e LEFT JOIN wiki_scan_objects t ON t.epoch=e.epoch AND t.wiki_id=e.wiki_id AND t.oid=e.target LEFT JOIN wiki_scan_objects s ON s.epoch=e.epoch AND s.wiki_id=e.wiki_id AND s.oid=e.source WHERE e.epoch=?1 AND (e.wiki_id,e.source,e.ordinal)>(?2,?3,?4) ORDER BY e.wiki_id,e.source,e.ordinal LIMIT ?5";

impl<K: WikiObjectOwnership> WikiScratchLedger<K> {
    /// Consume the opaque continuation between <=size indexed joins. Complete
    /// scratch data is still not native/SQL agreement, synchronization or a cap.
    pub fn validate_edges(
        &self,
        cursor: Option<WikiScratchEdgeCursor<K>>,
        size: WikiStartupBatchSize,
    ) -> impl Future<Output = Result<WikiScratchEdgeBatch<K>, WikiWriteError>> + Send + use<K> {
        let owner = Arc::clone(&self.owner);
        let valid = cursor.as_ref().is_none_or(|cursor| Arc::ptr_eq(&cursor.owner, &owner));
        let after = cursor.as_ref().map_or((0, [0; 20], -1), |cursor| (cursor.wiki, cursor.source, cursor.ordinal));
        let job = if valid {
            self.owner.recovery.writer.start_operation(jobs::Admission::Recovery, move |pool, _catalog| async move {
            let _cursor = cursor;
            let epoch = owner.recovery.epoch;
            let mut tx = pool.begin_immediate().await?;
            state::epoch(&mut tx, epoch).await?;
            let (actual, phase): (i64,String) = sqlx::query_as("SELECT epoch,phase FROM wiki_scan_control WHERE singleton=1").fetch_one(&mut *tx).await?;
            if actual!=epoch || !matches!(phase.as_str(),"scan"|"edges") { return Err(WikiWriteError::Binding) }
            if phase=="scan" {
                let unfinished: i64 = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM wiki_scan_scopes WHERE epoch=?1 AND scanned=0) OR EXISTS(SELECT 1 FROM wiki_scan_objects WHERE epoch=?1 AND complete=0)")
                    .bind(epoch).fetch_one(&mut *tx).await?;
                if unfinished!=0 { return Err(WikiWriteError::ScratchIncomplete) }
                state::changed(sqlx::query("UPDATE wiki_scan_control SET phase='edges' WHERE singleton=1 AND epoch=?1 AND phase='scan'")
                    .bind(epoch).execute(&mut *tx).await?.rows_affected())?;
            }
            let rows: Vec<EdgeRow> = sqlx::query_as(EDGES).bind(epoch).bind(after.0).bind(after.1.as_slice()).bind(after.2)
                .bind(i64::from(size.get())+1).fetch_all(&mut *tx).await?;
            let more = rows.len()>usize::from(size.get());
            let mut last = None; let mut checked=0;
            for row in rows.into_iter().take(usize::from(size.get())) {
                if row.actual_kind!=Some(row.expected_kind) || row.target_complete!=Some(1) || row.source_complete!=Some(1) {
                    return Err(WikiWriteError::ScratchIncomplete);
                }
                last = Some((row.wiki_id, <[u8;20]>::try_from(row.source).map_err(|_| WikiWriteError::Binding)?, row.ordinal));
                checked+=1;
            }
            if !more { state::changed(sqlx::query("UPDATE wiki_scan_control SET phase='agreement' WHERE singleton=1 AND epoch=?1 AND phase='edges'")
                .bind(epoch).execute(&mut *tx).await?.rows_affected())?; }
            tx.commit().await?;
            let next = if more { let (wiki,source,ordinal)=last.ok_or(WikiWriteError::Binding)?; Some(WikiScratchEdgeCursor { wiki,source,ordinal,owner }) } else { None };
            Ok(WikiScratchEdgeBatch { checked,next })
        })
        } else {
            Err(WikiWriteError::Binding)
        };
        async move { job?.await.map_err(|_| WikiWriteError::Uncertain)? }
    }
}
