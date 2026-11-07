//! Explicit wiki storage composition; no routes, readiness or visible publisher.
use rbit_git::wikis::{CatalogBoundWikiObjects, CatalogPreparedWikiObjects, WikiObjectError, WikiObjectWriter, WikiObjects};
use rbit_kernel::BlockingBudget;
use rbit_storage::{
    Database,
    sqlite::wiki_mutations::{
        WikiMutationStore,
        ownership::{WikiCatalogLease, WikiCatalogLeaseError},
    },
};
use rbit_wikis::{commands::WikiIntent, ownership::WikiCatalogOwnership};
use std::{fmt, path::Path};

#[derive(Debug, thiserror::Error)]
pub enum WikiStorageOpenError {
    #[error("cannot own the wiki catalog namespace")]
    Catalog(#[from] WikiCatalogLeaseError),
    #[error("cannot own the wiki object namespace")]
    Objects(#[from] WikiObjectError),
}

/// Construct only for explicitly enabled/maintenance wiki composition. This owner
/// does not declare the wiki ready: startup integrity/durability/recovery and owned
/// command execution remain required before any HTTP write surface is activated.
#[derive(Clone)]
pub struct WikiStorageOwner {
    catalog: WikiMutationStore,
    objects: CatalogBoundWikiObjects<WikiCatalogLease>,
}
impl fmt::Debug for WikiStorageOwner {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("WikiStorageOwner(..)")
    }
}
impl WikiStorageOwner {
    /// Startup only. Catalog ownership is derived from the actual opened database
    /// BEFORE object-root creation: another configured root cannot bypass exclusion.
    pub async fn open(database: &Database, object_root: &Path, work: BlockingBudget) -> Result<Self, WikiStorageOpenError> {
        let lease = database.wiki_catalog_lease().await?;
        let objects = WikiObjects::open(object_root, lease.store_id(), work)?;
        let writer = WikiObjectWriter::open(objects)?.bind_catalog(lease)?;
        Ok(Self { catalog: database.wiki_mutations(), objects: writer })
    }
    pub fn catalog(&self) -> &WikiMutationStore {
        &self.catalog
    }
    /// Unpublished work only. The input is a trusted accepted SQL fact in the future
    /// private coordinator; data construction alone grants no permission/publication.
    pub async fn prepare(&self, intent: WikiIntent) -> Result<CatalogPreparedWikiObjects<WikiCatalogLease>, WikiObjectError> {
        self.objects.prepare(intent).await
    }
}
