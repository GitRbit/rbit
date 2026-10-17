//! redb key/value storage for hot, simple, rebuildable data: login sessions and
//! content-addressed insight caches.
//!
//! redb transactions are synchronous, so every public method hops onto Tokio's blocking
//! pool; callers stay fully async.

mod insights;
mod sessions;

use std::{path::Path, sync::Arc};

pub use insights::{CachedCommit, CachedComparisonCounts, CachedLanguage, CachedLanguageStats, CachedLastCommits, InsightsCache};
pub use sessions::{SessionRecord, SessionStore, StoredSession};

use crate::{StorageError, error::kv_error};

/// Shared handle to the redb database file.
#[derive(Debug, Clone)]
pub struct KvStore {
    db: Arc<redb::Database>,
}

impl KvStore {
    pub fn open(path: &Path) -> Result<Self, StorageError> {
        let db = redb::Database::create(path).map_err(kv_error)?;
        let store = Self { db: Arc::new(db) };
        store.sessions().ensure_table()?;
        store.insights().ensure_tables()?;
        Ok(store)
    }

    pub fn sessions(&self) -> SessionStore {
        SessionStore::new(Arc::clone(&self.db))
    }

    pub fn insights(&self) -> InsightsCache {
        InsightsCache::new(Arc::clone(&self.db))
    }
}
