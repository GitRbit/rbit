//! Connection setup is separate from command statements and verified on replacement.
use super::{FullWikiWriter, WikiCatalogHandle, WikiCatalogLease, WikiCatalogLeaseError, WikiWriteError, Writer};
use crate::sqlite::pool::Pool;
use rbit_wikis::ownership::WikiCatalogOwnership;
use sqlx::sqlite::{SqlitePoolOptions, SqliteSynchronous};
use std::sync::{Arc, Mutex};
use tokio::sync::Semaphore;
use tokio_util::task::TaskTracker;

pub(super) async fn open(lease: WikiCatalogLease) -> Result<FullWikiWriter, WikiWriteError> {
    let lease = tokio::task::spawn_blocking(move || {
        lease.check_bound()?;
        Ok::<_, WikiCatalogLeaseError>(lease)
    })
    .await
    .map_err(|_| WikiWriteError::Uncertain)??;
    let options = lease.connection_options().create_if_missing(false).synchronous(SqliteSynchronous::Full);
    let catalog = WikiCatalogHandle(Arc::new(lease));
    let pool = SqlitePoolOptions::new()
        .min_connections(1)
        .max_connections(1)
        .after_connect(|connection, _| {
            Box::pin(async move {
                let actual: i64 = sqlx::query_scalar("PRAGMA synchronous").fetch_one(connection).await?;
                if actual != 2 {
                    return Err(sqlx::Error::Protocol("wiki writer requires FULL synchronization".into()));
                }
                Ok(())
            })
        })
        .connect_with(options)
        .await?;
    let check = catalog.clone();
    tokio::task::spawn_blocking(move || check.check_bound()).await.map_err(|_| WikiWriteError::Uncertain)??;
    Ok(FullWikiWriter(Arc::new(Writer {
        commands: Pool::new(pool.clone(), "wiki_mutations"),
        pool,
        gate: Mutex::new(false),
        admission: Arc::new(Semaphore::new(1)),
        tasks: TaskTracker::new(),
        catalog,
    })))
}
