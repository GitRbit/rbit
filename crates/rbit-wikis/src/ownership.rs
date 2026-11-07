//! Runtime namespace ownership port; no authentication or durable-state assertion.
use crate::WikiStoreId;
use std::path::Path;

/// Trusted composition implements this with a real held lock derived from the
/// actual opened catalog, never a caller-selected path or a parsed boolean/ID.
/// Verification checks observed physical binding under a stable topology contract.
/// It does not prove local-volume durability, safe restore or absence of old code.
/// The synchronous check performs I/O: call at startup or inside admitted blocking
/// work, never on a request's asynchronous executor thread.
pub trait WikiCatalogOwnership: Send + Sync + 'static {
    type Error: Send + Sync;
    fn check_bound(&self) -> Result<(), Self::Error>;
    fn store_id(&self) -> WikiStoreId;
    fn catalog_path(&self) -> &Path;
}

/// Actual managed object-root resource paired with that same catalog. This named
/// I/O port checks binding only; it is not integrity/durability/readiness authority.
pub trait WikiObjectOwnership: WikiCatalogOwnership {
    fn check_objects_bound(&self) -> Result<(), Self::Error>;
}
