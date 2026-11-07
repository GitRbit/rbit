//! Fixed publication; no credentials, identifiers or latest epoch refresh.
use super::super::{IdentityUpgradeError as Error, model::Checkpoint};
use sqlx::{Executor, Sqlite, SqliteConnection};
pub(super) async fn layout(connection: &mut SqliteConnection) -> Result<(), Error> {
    if super::super::layout::history(connection).await? < 68 {
        return Err(Error::Invalid);
    }
    let guards: i64 = sqlx::query_scalar("SELECT count(*) FROM sqlite_schema WHERE type='trigger' AND name GLOB 'identity_v3_*'")
        .fetch_one(&mut *connection)
        .await?;
    let retained:i64=sqlx::query_scalar("SELECT count(*) FROM sqlite_schema WHERE type='trigger' AND name IN ('credential_active_insert','credential_active_update','credential_active_delete')")
        .fetch_one(&mut *connection).await?;
    if guards != 45 || retained != 3 {
        return Err(Error::Invalid);
    }
    Ok(())
}
pub(super) async fn publication<'e, E: Executor<'e, Database = Sqlite>>(executor: E) -> Result<Option<Checkpoint>, Error> {
    let rows: Vec<Option<Vec<u8>>> = sqlx::query_scalar("SELECT CASE WHEN singleton=1 AND typeof(checkpoint)='blob' AND length(checkpoint)=64 THEN checkpoint END FROM credential_active_protocol LIMIT 2")
        .fetch_all(executor).await?;
    match rows.as_slice() {
        [] => Ok(None),
        [Some(bytes)] => Ok(Some(Checkpoint::decode(bytes.as_slice().try_into().map_err(|_| Error::Invalid)?)?)),
        _ => Err(Error::Invalid),
    }
}
pub(super) async fn match_prepared(connection: &mut SqliteConnection, checkpoint: Checkpoint) -> Result<(), Error> {
    if !matches!(super::super::sql::state(&mut *connection).await?, super::super::sql::State::Prepared(current) if current==checkpoint)
        || super::super::sql::pair(&mut *connection).await? != Some(checkpoint.pair)
    {
        return Err(Error::Mismatch);
    }
    Ok(())
}
pub(super) async fn publish(connection: &mut SqliteConnection, checkpoint: Checkpoint) -> Result<(), Error> {
    match_prepared(connection, checkpoint).await?;
    sqlx::query("INSERT INTO credential_active_protocol(singleton,checkpoint) VALUES(1,?)")
        .bind(checkpoint.encode().as_slice())
        .execute(connection)
        .await?;
    Ok(())
}
