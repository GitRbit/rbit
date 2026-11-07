//! Stopped-startup SQL fencing/replay; no managed readiness or native head receipt.
use super::super::{AcceptancePolicy, accept_on};
use super::{CommittedWikiAcceptance, FullWikiWriter, Pool, WikiCatalogHandle, WikiWriteError, jobs};
use rbit_core::Timestamp;
use rbit_wikis::{
    WikiStoreId,
    commands::{WikiIntent, WikiWriteCommand},
    ownership::{WikiCatalogOwnership, WikiObjectOwnership},
};
use std::{
    fmt,
    future::Future,
    num::NonZeroU32,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

struct GrantedObjects<K> {
    objects: K,
    permission: Arc<AtomicBool>,
}
impl<K> Drop for GrantedObjects<K> {
    fn drop(&mut self) {
        // Revoke BEFORE actual root ownership is destroyed. Each queued SQL job
        // and returned acceptance retains this guard, not merely a copied flag.
        self.permission.store(false, Ordering::Release);
    }
}
/// Actual object resource plus private SQL-permission lifetime. It has no public
/// constructor/flag setter and is not native integrity/readiness authority.
pub struct WikiRecoveryObjects<K>(Arc<GrantedObjects<K>>);
impl<K> Clone for WikiRecoveryObjects<K> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}
impl<K> fmt::Debug for WikiRecoveryObjects<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("WikiRecoveryObjects(..)")
    }
}
impl<K> WikiRecoveryObjects<K> {
    pub fn objects(&self) -> &K {
        &self.0.objects
    }
}
impl<K: WikiObjectOwnership> WikiCatalogOwnership for WikiRecoveryObjects<K> {
    type Error = K::Error;
    fn check_bound(&self) -> Result<(), Self::Error> {
        self.0.objects.check_bound()
    }
    fn store_id(&self) -> WikiStoreId {
        self.0.objects.store_id()
    }
    fn catalog_path(&self) -> &Path {
        self.0.objects.catalog_path()
    }
}
impl<K: WikiObjectOwnership> WikiObjectOwnership for WikiRecoveryObjects<K> {
    fn check_objects_bound(&self) -> Result<(), Self::Error> {
        self.0.objects.check_objects_bound()
    }
}

/// Requires a stopped host and prerequisite destructive-operation bridges BEFORE
/// production use. No public transition to managed/ready is provided in this slice.
pub struct FencedWikiRecovery<K> {
    epoch: i64,
    pub(super) writer: FullWikiWriter,
    // Last: keep actual object-root ownership through the controller lifetime.
    objects: WikiRecoveryObjects<K>,
}
impl<K> fmt::Debug for FencedWikiRecovery<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("FencedWikiRecovery(..)")
    }
}

/// Exact stored acceptance after an actual verified FULL checkpoint. Retains the
/// real root/catalog resources. This is not native closure, auth or visible success.
pub struct FencedWikiAcceptance<K> {
    acceptance: CommittedWikiAcceptance,
    epoch: i64,
    objects: WikiRecoveryObjects<K>,
}
impl<K> fmt::Debug for FencedWikiAcceptance<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("FencedWikiAcceptance(..)")
    }
}
impl<K> FencedWikiAcceptance<K> {
    pub fn intent(&self) -> &WikiIntent {
        self.acceptance.intent()
    }
    /// Data for the next owned phase, never a validated native-head capability.
    pub fn into_parts(self) -> (WikiIntent, i64, WikiRecoveryObjects<K>) {
        (self.acceptance.intent, self.epoch, self.objects)
    }
}

impl FullWikiWriter {
    /// STOPPED maintenance/startup only. The returned owner still refuses new user
    /// commands. Opening a host listener/legacy destructive route during this phase
    /// violates the explicit deployment contract; no central activation is added.
    pub fn begin_recovery<K: WikiObjectOwnership>(
        self,
        objects: K,
    ) -> impl Future<Output = Result<FencedWikiRecovery<K>, WikiWriteError>> + Send + use<K> {
        let writer = self.clone();
        let permission = Arc::clone(&self.0.protocol_permission);
        let job = self.start_operation(jobs::Admission::BeginFence, move |pool, catalog| async move {
            let objects = check_objects(catalog.clone(), objects).await?;
            let objects = WikiRecoveryObjects(Arc::new(GrantedObjects { objects, permission }));
            objects.0.permission.store(true, Ordering::Release);
            let epoch = prepare_protocol(&pool).await?;
            check_objects(catalog, objects.clone()).await?;
            Ok(FencedWikiRecovery { epoch, writer, objects })
        });
        async move { job?.await.map_err(|_| WikiWriteError::Uncertain)? }
    }
}

impl<K: WikiObjectOwnership> FencedWikiRecovery<K> {
    /// Replay ONLY. Missing commands are never inserted, reissued or restamped.
    /// Busy/partial checkpoint leaves the original pending record for exact retry.
    pub fn replay(
        &self,
        command: WikiWriteCommand,
        at: Timestamp,
        max_pending: NonZeroU32,
    ) -> impl Future<Output = Result<FencedWikiAcceptance<K>, WikiWriteError>> + Send + use<K> {
        let objects = self.objects.clone();
        let epoch = self.epoch;
        let job = self.writer.start_operation(jobs::Admission::Recovery, move |pool, catalog| async move {
            check_objects(catalog.clone(), objects.clone()).await?;
            let accepted = accept_on(&pool, command, at, max_pending, AcceptancePolicy::ExistingAtEpoch(epoch))
                .await
                .map_err(WikiWriteError::Acceptance)?;
            synchronize_replay(&pool).await?;
            check_objects(catalog.clone(), objects.clone()).await?;
            Ok(FencedWikiAcceptance { acceptance: CommittedWikiAcceptance { intent: accepted.intent, _catalog: catalog }, epoch, objects })
        });
        async move { job?.await.map_err(|_| WikiWriteError::Uncertain)? }
    }
    /// Drain/close SQL only. Native roots/escaped values intentionally retain their
    /// resources; this does not claim an entire Wiki platform is stopped or ready.
    pub async fn close(&self) -> Result<(), WikiWriteError> {
        self.writer.close().await
    }
}

async fn check_objects<K: WikiObjectOwnership>(catalog: WikiCatalogHandle, objects: K) -> Result<K, WikiWriteError> {
    tokio::task::spawn_blocking(move || {
        catalog.check_bound()?;
        if objects.store_id() != catalog.store_id() || objects.catalog_path() != catalog.catalog_path() {
            return Err(WikiWriteError::Binding);
        }
        objects.check_objects_bound().map_err(|_| WikiWriteError::Binding)?;
        Ok(objects)
    })
    .await
    .map_err(|_| WikiWriteError::Uncertain)?
}

async fn prepare_protocol(pool: &Pool) -> Result<i64, WikiWriteError> {
    let mut tx = pool.begin_immediate().await?;
    let (mode, previous): (String, i64) =
        sqlx::query_as("SELECT mode,epoch FROM wiki_writer_protocol WHERE singleton=1").fetch_one(&mut *tx).await?;
    if !matches!(mode.as_str(), "legacy" | "preparing" | "managed") || previous < 0 {
        return Err(WikiWriteError::Binding);
    }
    let epoch = previous.checked_add(1).ok_or(WikiWriteError::EpochExhausted)?;
    let changed = sqlx::query("UPDATE wiki_writer_protocol SET mode='preparing',epoch=?1 WHERE singleton=1 AND epoch=?2")
        .bind(epoch)
        .bind(previous)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if changed != 1 {
        return Err(WikiWriteError::Binding);
    }
    tx.commit().await?;
    Ok(epoch)
}

async fn synchronize_replay(pool: &Pool) -> Result<(), WikiWriteError> {
    let (busy, total, completed): (i64, i64, i64) = sqlx::query_as("PRAGMA main.wal_checkpoint(FULL)").fetch_one(pool).await?;
    if busy != 0 || total < 0 || completed != total {
        return Err(WikiWriteError::ReplaySyncIncomplete);
    }
    Ok(())
}
