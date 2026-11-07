//! Catalog-adjacent ownership derived from the actual opened pool, never config input.
mod files;
#[cfg(test)]
mod tests;

use crate::StorageError;
use rbit_wikis::{WikiStoreId, ownership::WikiCatalogOwnership};
use std::{
    fmt,
    fs::{File, TryLockError},
    path::{Path, PathBuf},
};

#[derive(Debug, thiserror::Error)]
pub enum WikiCatalogLeaseError {
    #[error("wiki catalog already has a cooperating writer")]
    Busy,
    #[error("wiki publication requires a persistent catalog file")]
    NotPersistent,
    #[error("wiki catalog or lock uses an unsupported filesystem alias")]
    Aliased,
    #[error("wiki catalog binding changed or closed")]
    Changed,
    #[error("wiki catalog ownership is unsupported on this platform")]
    UnsupportedPlatform,
    #[error("wiki catalog ownership I/O failed")]
    Io(#[from] std::io::Error),
    #[error("wiki catalog identity query failed")]
    Storage(#[from] StorageError),
}
impl From<sqlx::Error> for WikiCatalogLeaseError {
    fn from(error: sqlx::Error) -> Self {
        Self::Storage(error.into())
    }
}

/// A real held subject/adjacent lock, not an auth or publication receipt. It parks
/// no pooled connection. Cloned runtime owners must share this value through `Arc`.
pub struct WikiCatalogLease {
    pool: sqlx::SqlitePool,
    database: File,
    path: PathBuf,
    store: WikiStoreId,
    identity: files::Identity,
    lock_path: PathBuf,
    lock_identity: files::Identity,
    // Last: keep namespace exclusion until the subject and other state are dropped.
    lock: File,
}
impl fmt::Debug for WikiCatalogLease {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("WikiCatalogLease(..)")
    }
}

impl crate::sqlite::Database {
    /// Acquire BEFORE opening a configured wiki object root. Separate Services/pools
    /// targeting the same actual catalog cannot avoid exclusion with another root.
    pub async fn wiki_catalog_lease(&self) -> Result<WikiCatalogLease, WikiCatalogLeaseError> {
        let (reported, identity): (String, Vec<u8>) = sqlx::query_as(
            "SELECT d.file,i.identity FROM pragma_database_list d CROSS JOIN wiki_store_identity i WHERE d.name='main' AND i.singleton=1",
        )
        .fetch_one(&self.store("wiki_ownership"))
        .await?;
        if reported.is_empty() {
            return Err(WikiCatalogLeaseError::NotPersistent);
        }
        let reported = Path::new(&reported);
        files::no_links(reported)?;
        let path = reported.canonicalize()?;
        let options = self.pool.connect_options();
        files::no_links(options.get_filename())?;
        let configured = options.get_filename().canonicalize()?;
        if path != configured {
            return Err(WikiCatalogLeaseError::Changed);
        }
        let database = files::open(&path, false)?;
        let subject = files::identity(&database)?;
        let mut filename = path.file_name().ok_or(WikiCatalogLeaseError::NotPersistent)?.to_os_string();
        filename.push(".wiki-writer-v1.lock");
        let lock_path = path.with_file_name(filename);
        let lock = files::open(&lock_path, true)?;
        if lock.metadata()?.len() != 0 {
            return Err(WikiCatalogLeaseError::Aliased);
        }
        lock.try_lock().map_err(|error| match error {
            TryLockError::WouldBlock => WikiCatalogLeaseError::Busy,
            TryLockError::Error(error) => WikiCatalogLeaseError::Io(error),
        })?;
        let lock_identity = files::identity(&lock)?;
        let store = WikiStoreId::try_from(<[u8; 16]>::try_from(identity).map_err(|_| WikiCatalogLeaseError::Changed)?)
            .map_err(|_| WikiCatalogLeaseError::Changed)?;
        let lease = WikiCatalogLease { pool: self.pool.clone(), database, path, store, identity: subject, lock_path, lock_identity, lock };
        lease.check_bound()?;
        Ok(lease)
    }
}

impl WikiCatalogOwnership for WikiCatalogLease {
    type Error = WikiCatalogLeaseError;
    fn store_id(&self) -> WikiStoreId {
        self.store
    }
    fn catalog_path(&self) -> &Path {
        &self.path
    }
    fn check_bound(&self) -> Result<(), WikiCatalogLeaseError> {
        let options = self.pool.connect_options();
        files::no_links(options.get_filename())?;
        if self.pool.is_closed() || options.get_filename().canonicalize()? != self.path {
            return Err(WikiCatalogLeaseError::Changed);
        }
        let current = files::open(&self.path, false)?;
        let lock = files::open(&self.lock_path, false)?;
        if files::identity(&self.database)? != self.identity
            || files::identity(&current)? != self.identity
            || files::identity(&self.lock)? != self.lock_identity
            || files::identity(&lock)? != self.lock_identity
            || lock.metadata()?.len() != 0
        {
            return Err(WikiCatalogLeaseError::Changed);
        }
        Ok(())
    }
}
