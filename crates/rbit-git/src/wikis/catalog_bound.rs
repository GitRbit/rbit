//! Keep the actual catalog namespace owner with real queued/running native work.
use super::{
    WikiObjectError,
    managed_writer::{PreparedWikiObjects, WikiObjectWriter},
};
use rbit_wikis::{WikiRevisionId, commands::WikiIntent, ownership::WikiCatalogOwnership};
use std::{fmt, sync::Arc};

struct BoundOwner<C> {
    writer: WikiObjectWriter,
    catalog: C,
}

/// Concrete static ownership port; production composition supplies the storage
/// lease obtained from the actual opened `Database` before opening any object root.
/// It adds no authentication, integrity, durability or visible-publication proof.
pub struct CatalogBoundWikiObjects<C> {
    owner: Arc<BoundOwner<C>>,
}
impl<C> Clone for CatalogBoundWikiObjects<C> {
    fn clone(&self) -> Self {
        Self { owner: Arc::clone(&self.owner) }
    }
}
impl<C> fmt::Debug for CatalogBoundWikiObjects<C> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("CatalogBoundWikiObjects(..)")
    }
}

pub struct CatalogPreparedWikiObjects<C> {
    prepared: PreparedWikiObjects,
    // Last: catalog exclusion outlives the prepared object/scope and its payload.
    _owner: Arc<BoundOwner<C>>,
}
impl<C> fmt::Debug for CatalogPreparedWikiObjects<C> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("CatalogPreparedWikiObjects(..)")
    }
}
impl<C> CatalogPreparedWikiObjects<C> {
    pub fn intent(&self) -> &WikiIntent {
        self.prepared.intent()
    }
    pub const fn revision(&self) -> WikiRevisionId {
        self.prepared.revision()
    }
}

impl WikiObjectWriter {
    /// Startup only: bind an actual held catalog namespace through its trusted I/O port. A
    /// manually implemented port is not an authenticated or portable capability.
    pub fn bind_catalog<C: WikiCatalogOwnership>(self, catalog: C) -> Result<CatalogBoundWikiObjects<C>, WikiObjectError> {
        catalog.check_bound().map_err(|_| WikiObjectError::Binding)?;
        if catalog.store_id() != self.store_id() {
            return Err(WikiObjectError::Binding);
        }
        Ok(CatalogBoundWikiObjects { owner: Arc::new(BoundOwner { writer: self, catalog }) })
    }
}

impl<C: WikiCatalogOwnership> CatalogBoundWikiObjects<C> {
    pub async fn prepare(&self, intent: WikiIntent) -> Result<CatalogPreparedWikiObjects<C>, WikiObjectError> {
        let (prepared, owner) = self
            .owner
            .writer
            .prepare_retaining(intent, Arc::clone(&self.owner), |owner| owner.catalog.check_bound().map_err(|_| WikiObjectError::Binding))
            .await?;
        Ok(CatalogPreparedWikiObjects { prepared, _owner: owner })
    }
}
