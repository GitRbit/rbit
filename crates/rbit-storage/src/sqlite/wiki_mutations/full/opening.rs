//! Connection setup is separate from command statements and verified on replacement.
use super::{FullWikiWriter, WikiCatalogHandle, WikiCatalogLease, WikiCatalogLeaseError, WikiWriteError, retention, shutdown};
use rbit_wikis::ownership::WikiCatalogOwnership;
use sqlx::sqlite::{SqlitePoolOptions, SqliteSynchronous};
use std::{future::Future, sync::Arc};

pub(super) async fn open(lease: WikiCatalogLease) -> Result<FullWikiWriter, WikiWriteError> {
    open_after(lease, std::future::ready(())).await
}

// Private startup-boundary seam; production uses Ready without task allocation.
pub(super) async fn open_after(lease: WikiCatalogLease, initialized: impl Future<Output = ()>) -> Result<FullWikiWriter, WikiWriteError> {
    let lease = tokio::task::spawn_blocking(move || {
        lease.check_bound()?;
        Ok::<_, WikiCatalogLeaseError>(lease)
    })
    .await
    .map_err(|_| WikiWriteError::Uncertain)??;
    let options = lease.connection_options().create_if_missing(false).synchronous(SqliteSynchronous::Full).optimize_on_close(false, None);
    let catalog = WikiCatalogHandle(Arc::new(lease));
    let retention = retention::NativeRetention::new(catalog.clone());
    let state = Arc::new(retention::NativeState::default());
    let initialization = Arc::clone(&state);
    let pool = SqlitePoolOptions::new()
        .min_connections(0)
        .max_connections(1)
        .idle_timeout(None)
        .max_lifetime(None)
        .test_before_acquire(false)
        .after_connect(move |connection, _| {
            let state = Arc::clone(&initialization);
            Box::pin(async move {
                state.opened()?;
                let actual: i64 = sqlx::query_scalar("PRAGMA synchronous").fetch_one(connection).await.inspect_err(|_| state.fail())?;
                if actual != 2 {
                    state.fail();
                    return Err(sqlx::Error::Protocol("wiki writer requires FULL synchronization".into()));
                }
                Ok(())
            })
        })
        .connect_lazy_with(options);
    // Install real pool-close ownership BEFORE any async physical connection setup.
    let writer = shutdown::own_pool(pool, catalog, state, retention);
    match initialize(&writer, initialized).await {
        Ok(()) => Ok(writer),
        Err(error) => {
            writer.0.state.fail();
            // Preserve primary setup error. Even if close itself becomes uncertain,
            // this is not a stopped proof; the supervisor retains normal cleanup.
            let _cleanup = writer.close().await;
            Err(error)
        }
    }
}

async fn initialize(writer: &FullWikiWriter, initialized: impl Future<Output = ()>) -> Result<(), WikiWriteError> {
    let attempt = retention::InitializationAttempt::begin(Arc::clone(&writer.0.state));
    let connection = tokio::select! {
        acquired = writer.0.pool.acquire() => acquired?,
        () = writer.0.state.unproved() => return Err(WikiWriteError::Quarantined),
    };
    attempt.complete();
    drop(connection);
    initialized.await;
    let check = writer.catalog();
    tokio::task::spawn_blocking(move || check.check_bound()).await.map_err(|_| WikiWriteError::Uncertain)??;
    Ok(())
}
