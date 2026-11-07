//! Small control reads and explicit record charges; never a persisted proof.
use super::WikiWriteError;
use crate::sqlite::pool::Conn;
use rbit_wikis::startup::scratch::WikiScratchUsage;

pub(super) async fn epoch(connection: &mut Conn, expected: i64) -> Result<(), WikiWriteError> {
    let (mode, actual): (String, i64) =
        sqlx::query_as("SELECT mode,epoch FROM wiki_writer_protocol WHERE singleton=1").fetch_one(&mut *connection).await?;
    if mode != "preparing" || actual != expected {
        return Err(WikiWriteError::Binding);
    }
    Ok(())
}
pub(super) async fn active(connection: &mut Conn, expected: i64, phase: &str) -> Result<(), WikiWriteError> {
    epoch(connection, expected).await?;
    let (actual, current): (i64, String) =
        sqlx::query_as("SELECT epoch,phase FROM wiki_scan_control WHERE singleton=1").fetch_one(connection).await?;
    if actual != expected || current != phase {
        return Err(WikiWriteError::Binding);
    }
    Ok(())
}
pub(super) async fn empty(connection: &mut Conn) -> Result<bool, WikiWriteError> {
    let occupied: i64 = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM wiki_scan_scopes) OR EXISTS(SELECT 1 FROM wiki_scan_objects) OR EXISTS(SELECT 1 FROM wiki_scan_edges)",
    )
    .fetch_one(connection)
    .await?;
    Ok(occupied == 0)
}
pub(super) async fn usage(connection: &mut Conn) -> Result<WikiScratchUsage, WikiWriteError> {
    let (scopes, objects, edges, bytes): (i64, i64, i64, i64) =
        sqlx::query_as("SELECT scopes,objects,edges,logical_bytes FROM wiki_scan_control WHERE singleton=1").fetch_one(connection).await?;
    Ok(WikiScratchUsage {
        scopes: scopes.try_into().map_err(|_| WikiWriteError::Binding)?,
        objects: objects.try_into().map_err(|_| WikiWriteError::Binding)?,
        edges: edges.try_into().map_err(|_| WikiWriteError::Binding)?,
        logical_bytes: bytes.try_into().map_err(|_| WikiWriteError::Binding)?,
    })
}
pub(super) async fn charge(
    connection: &mut Conn,
    epoch: i64,
    scopes: i64,
    objects: i64,
    edges: i64,
    bytes: i64,
) -> Result<(), WikiWriteError> {
    let affected = sqlx::query("UPDATE wiki_scan_control SET scopes=scopes+?2,objects=objects+?3,edges=edges+?4,logical_bytes=logical_bytes+?5 WHERE singleton=1 AND epoch=?1 AND phase='scan' AND scopes<=max_scopes-?2 AND objects<=max_objects-?3 AND edges<=max_edges-?4 AND logical_bytes<=max_bytes-?5")
        .bind(epoch).bind(scopes).bind(objects).bind(edges).bind(bytes).execute(connection).await?.rows_affected();
    if affected != 1 {
        return Err(WikiWriteError::ScratchCapacity);
    }
    Ok(())
}
pub(super) fn changed(affected: u64) -> Result<(), WikiWriteError> {
    if affected != 1 {
        return Err(WikiWriteError::Binding);
    }
    Ok(())
}
