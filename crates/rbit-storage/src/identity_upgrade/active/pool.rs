//! No public registration/options/raw-pool escape. The real backend is retained
//! inside each `SQLite` worker's callbacks, including after its outer pool is dropped.
use super::{ActiveIdentityPair, kv, sql};
use crate::identity_upgrade::{IdentityUpgradeLimits, model::Checkpoint};
use crate::{identity_upgrade::IdentityUpgradeError as Error, sqlite::writer_compatibility};
use sqlx::{
    SqliteConnection,
    sqlite::{SqlitePool, SqlitePoolOptions},
};
use std::sync::Arc;

pub(crate) struct ActiveSqlPool {
    inner: SqlitePool,
}
impl ActiveSqlPool {
    /// Crate-private ownership handoff to Database, never a public raw-pool escape.
    pub(crate) fn into_storage_parts(self) -> (SqlitePool, crate::sqlite::protocol::SqlProtocol) {
        let protocol = crate::sqlite::protocol::SqlProtocol::accepted(&self);
        (self.inner, protocol)
    }
}
impl std::fmt::Debug for ActiveSqlPool {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("ActiveSqlPool").finish_non_exhaustive()
    }
}
pub(super) async fn bind_writer(connection: &mut SqliteConnection, db: Arc<redb::Database>) -> Result<(), Error> {
    let mut handle = connection.lock_handle().await?;
    for name in ["rbit_credential_fenced_v2", "rbit_credential_active_v3"] {
        let retained = Arc::clone(&db);
        handle.create_collation(name, move |a, b| {
            let _actual_owner = &retained;
            a.cmp(b)
        })?;
    }
    Ok(())
}
pub(super) async fn accept_connection(
    connection: &mut SqliteConnection,
    db: Arc<redb::Database>,
    checkpoint: Checkpoint,
    limits: IdentityUpgradeLimits,
) -> Result<(), Error> {
    sql::layout(connection).await?;
    if sql::publication(&mut *connection).await? != Some(checkpoint) {
        return Err(Error::Mismatch);
    }
    sql::match_prepared(connection, checkpoint).await?;
    let check = Arc::clone(&db);
    if !tokio::task::spawn_blocking(move || kv::is_active(&check, checkpoint, limits)).await?? {
        return Err(Error::Mismatch);
    }
    bind_writer(connection, db).await
}
impl ActiveIdentityPair {
    pub(super) async fn sql_pool(&mut self) -> Result<ActiveSqlPool, Error> {
        // Derive from the actual accepted connection; never accept another caller's
        // filename, options, pool or claimed pair as the authority for this factory.
        let paths: Vec<String> = sqlx::query_scalar("SELECT file FROM pragma_database_list WHERE name='main' LIMIT 2")
            .fetch_all(&mut self.owner.connection)
            .await?;
        let [path] = paths.as_slice() else { return Err(Error::Invalid) };
        if path.is_empty() {
            return Err(Error::Invalid);
        }
        let db = Arc::clone(&self.owner.db);
        let options =
            writer_compatibility::database_options(std::path::Path::new(path))?.create_if_missing(false).pragma("trusted_schema", "OFF");
        let checkpoint = self.checkpoint;
        let limits = self.owner.limits;
        let inner = SqlitePoolOptions::new()
            .max_connections(crate::sqlite::connection_capacity())
            .after_connect(move |connection, _| {
                let db = Arc::clone(&db);
                Box::pin(async move { accept_connection(connection, db, checkpoint, limits).await.map_err(|error| protocol_error(&error)) })
            })
            .connect_with(options)
            .await?;
        Ok(ActiveSqlPool { inner })
    }
}
fn protocol_error(error: &Error) -> sqlx::Error {
    sqlx::Error::Protocol(error.to_string())
}

#[cfg(test)]
pub(super) async fn verify_pool_lifetime(mut accepted: ActiveIdentityPair, path: &std::path::Path) {
    use sqlx::Connection;
    let pool = accepted.sql_pool().await.unwrap();
    let connection = pool.inner.acquire().await.unwrap().detach();
    let ActiveIdentityPair { owner, .. } = accepted;
    let super::super::owner::StoppedIdentityUpgrade { connection: initial, db, .. } = owner;
    initial.close().await.unwrap();
    drop(db);
    pool.inner.close().await;
    drop(pool);
    assert!(redb::Database::open(path).is_err(), "SQLite worker callbacks retain the actual native owner");
    connection.close().await.unwrap();
}
