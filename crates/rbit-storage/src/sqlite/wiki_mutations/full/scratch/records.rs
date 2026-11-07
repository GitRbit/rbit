//! Atomically persist one bounded native batch; incomplete objects stay incomplete.
use super::{WikiWriteError, state};
use crate::sqlite::pool::Conn;
use rbit_wikis::startup::scratch::{EDGE_CHARGE, OBJECT_CHARGE};
use rbit_wikis::{
    WikiId, WikiPageId, WikiRevisionId,
    startup::objects::{WikiScanRecord, WikiScannedKind},
};

fn kind(kind: WikiScannedKind) -> i64 {
    match kind {
        WikiScannedKind::Blob => 1,
        WikiScannedKind::Tree => 2,
        WikiScannedKind::Commit => 3,
    }
}
fn id(id: &WikiRevisionId) -> Result<&[u8], WikiWriteError> {
    let bytes = id.as_bytes();
    if bytes.len() != 20 {
        return Err(WikiWriteError::Binding);
    }
    Ok(bytes)
}
pub(super) async fn apply(connection: &mut Conn, epoch: i64, wiki: WikiId, record: WikiScanRecord) -> Result<(), WikiWriteError> {
    match record {
        WikiScanRecord::Object { id: object, kind: native_kind, bytes } => {
            let maximum = match native_kind {
                WikiScannedKind::Blob => 256 * 1024,
                WikiScannedKind::Tree => 1024 * 1024,
                WikiScannedKind::Commit => 64 * 1024,
            };
            if bytes > maximum {
                return Err(WikiWriteError::Binding);
            }
            state::charge(connection, epoch, 0, 1, 0, OBJECT_CHARGE).await?;
            sqlx::query("INSERT INTO wiki_scan_objects(epoch,wiki_id,oid,kind,bytes) SELECT ?1,?2,?3,?4,?5 FROM wiki_scan_scopes WHERE epoch=?1 AND wiki_id=?2 AND scanned=0")
                .bind(epoch).bind(wiki.get()).bind(id(&object)?).bind(kind(native_kind)).bind(i64::from(bytes))
                .execute(connection).await.map(|result| state::changed(result.rows_affected()))??;
        }
        WikiScanRecord::Commit { id: object, actor, incarnation, at } => {
            if actor.0 <= 0 || incarnation.is_some_and(|value| value <= 0) {
                return Err(WikiWriteError::Binding);
            }
            state::changed(sqlx::query("UPDATE wiki_scan_objects SET actor_id=?4,actor_incarnation=?5,committed_at=?6 WHERE epoch=?1 AND wiki_id=?2 AND oid=?3 AND kind=3 AND complete=0 AND actor_id IS NULL")
                .bind(epoch).bind(wiki.get()).bind(id(&object)?).bind(actor.0).bind(incarnation).bind(at.seconds())
                .execute(connection).await?.rows_affected())?;
        }
        WikiScanRecord::Edge { source, ordinal, target, kind: expected, page } => {
            let source_id = id(&source)?;
            let (source_kind, complete, next): (i64, i64, i64) =
                sqlx::query_as("SELECT kind,complete,next_ordinal FROM wiki_scan_objects WHERE epoch=?1 AND wiki_id=?2 AND oid=?3")
                    .bind(epoch)
                    .bind(wiki.get())
                    .bind(source_id)
                    .fetch_one(&mut *connection)
                    .await?;
            let valid = match source_kind {
                2 => ordinal < 10_000 && expected == WikiScannedKind::Blob && page.is_some(),
                3 => {
                    page.is_none()
                        && ((ordinal == 0 && expected == WikiScannedKind::Tree) || (ordinal == 1 && expected == WikiScannedKind::Commit))
                }
                _ => false,
            };
            if complete != 0 || next != i64::from(ordinal) || !valid {
                return Err(WikiWriteError::Binding);
            }
            state::charge(connection, epoch, 0, 0, 1, EDGE_CHARGE).await?;
            sqlx::query(
                "INSERT INTO wiki_scan_edges(epoch,wiki_id,source,ordinal,target,expected_kind,page_id) VALUES(?1,?2,?3,?4,?5,?6,?7)",
            )
            .bind(epoch)
            .bind(wiki.get())
            .bind(source_id)
            .bind(i64::from(ordinal))
            .bind(id(&target)?)
            .bind(kind(expected))
            .bind(page.map(WikiPageId::get))
            .execute(&mut *connection)
            .await?;
            state::changed(sqlx::query("UPDATE wiki_scan_objects SET next_ordinal=next_ordinal+1 WHERE epoch=?1 AND wiki_id=?2 AND oid=?3 AND complete=0 AND next_ordinal=?4")
                .bind(epoch).bind(wiki.get()).bind(source_id).bind(i64::from(ordinal)).execute(connection).await?.rows_affected())?;
        }
        WikiScanRecord::Complete { id: object } => {
            state::changed(sqlx::query("UPDATE wiki_scan_objects SET complete=1 WHERE epoch=?1 AND wiki_id=?2 AND oid=?3 AND complete=0 AND ((kind=1 AND next_ordinal=0 AND actor_id IS NULL) OR (kind=2 AND actor_id IS NULL) OR (kind=3 AND next_ordinal IN (1,2) AND actor_id IS NOT NULL AND committed_at IS NOT NULL))")
                .bind(epoch).bind(wiki.get()).bind(id(&object)?).execute(connection).await?.rows_affected())?;
        }
    }
    Ok(())
}
