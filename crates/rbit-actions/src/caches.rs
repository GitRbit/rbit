//! The shared cache (§12.5.6): `actions/cache` entries the server keeps for every runner of
//! a repository, so a job restores what another runner saved.
//!
//! * Scopes come from the task's run ([`rbit_workflow::spec::CacheScopes`]), never from the
//!   runner: a job restores from its own ref's scope, then the default branch's, and saves
//!   only to its own.
//! * An entry matches on its key (exactly, else the newest whose key starts with a
//!   `restore-keys` prefix, in [`restore_probes`] order) and its version. Entries are
//!   immutable: saving a key the write scope has is skipped unless the runner replaces one
//!   it could not restore.
//! * A repository keeps `max_cache_storage_bytes`; a save past it evicts the least recently
//!   restored entries first, never the one just saved. Restoring counts as a use. The sweep
//!   removes entries unused for `cache_retention_days`.

use std::future::Future;

use rbit_workflow::{
    protocol::{CacheProbe, MAX_CACHE_KEY_CHARS, MAX_RESTORE_KEYS, restore_probes, sha256_from_hex, valid_cache_key, valid_cache_version},
    spec::CacheScopes,
};

use crate::archives::{ArchiveFiles, ArchiveUploads, BATCH, Refusal, StoreError, days_before};

/// What a runner looks up.
#[derive(Debug, Clone, Copy)]
pub struct CacheLookup<'a> {
    pub key: &'a str,
    pub restore_keys: &'a [String],
    pub version: &'a str,
}

/// What a runner sends with a save.
#[derive(Debug, Clone, Copy)]
pub struct CacheSave<'a> {
    pub key: &'a str,
    pub version: &'a str,
    /// The tar.gz's SHA-256, as 64 hex digits.
    pub sha256: &'a str,
    /// Replace an entry of the key the job could not restore.
    pub replace: bool,
}

/// What the application reads of a stored entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CacheFacts<'a> {
    pub id: i64,
    pub repository: i64,
    pub scope: &'a str,
    pub size: u64,
}

/// The row of a save, for the store to record.
#[derive(Debug, Clone, Copy)]
pub struct NewCacheEntry<'a> {
    pub repository: i64,
    pub scope: &'a str,
    pub key: &'a str,
    pub version: &'a str,
    pub size: u64,
    pub sha256: [u8; 32],
}

/// What recording a save did.
#[derive(Debug)]
pub enum CacheRecorded<E> {
    Stored {
        entry: E,
        replaced: Option<E>,
    },
    /// The scope has the key, and the save does not replace.
    Exists,
}

/// Stored entries: their files and rows, and a repository's usage.
pub trait CacheRecords: ArchiveFiles {
    type Entry: Send + Sync;

    fn facts<'a>(&self, entry: &'a Self::Entry) -> CacheFacts<'a>;
    fn location(&self, entry: &Self::Entry) -> Self::Location;
    fn delete(&self, entry: &Self::Entry) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn stored_bytes(&self, repository: i64) -> impl Future<Output = Result<u64, Self::Error>> + Send;
    /// A repository's least recently used entries first.
    fn least_used(&self, repository: i64, limit: u32) -> impl Future<Output = Result<Vec<Self::Entry>, Self::Error>> + Send;
}

/// One running task's view of the cache.
pub trait CacheTaskStore: CacheRecords {
    type Target: Send + Sync;
    type Opened: Send;

    /// The task's run, while the task holds its job's lease.
    fn lease(&self) -> impl Future<Output = Result<Self::Target, Self::Error>> + Send;
    fn repository(&self, target: &Self::Target) -> i64;
    fn scopes<'a>(&self, target: &'a Self::Target) -> &'a CacheScopes;
    fn exact(
        &self,
        repository: i64,
        scope: &str,
        key: &str,
        version: &str,
    ) -> impl Future<Output = Result<Option<Self::Entry>, Self::Error>> + Send;
    fn newest_with_prefix(
        &self,
        repository: i64,
        scope: &str,
        prefix: &str,
        version: &str,
    ) -> impl Future<Output = Result<Option<Self::Entry>, Self::Error>> + Send;
    fn find(&self, id: i64) -> impl Future<Output = Result<Option<Self::Entry>, Self::Error>> + Send;
    /// Opens the entry's file to send it.
    fn open(&self, entry: &Self::Entry) -> impl Future<Output = Result<Self::Opened, Self::Error>> + Send;
    /// Marks the entry used now.
    fn touch(&self, entry: &Self::Entry) -> impl Future<Output = Result<(), Self::Error>> + Send;
}

/// One running task's save.
pub trait CacheSaveStore: CacheTaskStore + ArchiveUploads {
    /// The lease again, after the body: it may have ended meanwhile.
    fn still_leased(&self, target: &Self::Target) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn record(
        &self,
        new: &NewCacheEntry<'_>,
        replace: bool,
    ) -> impl Future<Output = Result<CacheRecorded<Self::Entry>, Self::Error>> + Send;
}

/// Entries unused for too long, for the sweep.
pub trait CacheRetention: CacheRecords {
    fn unused_since(&self, cutoff: i64, limit: u32) -> impl Future<Output = Result<Vec<Self::Entry>, Self::Error>> + Send;
}

fn invalid_key<E>() -> StoreError<E> {
    StoreError::bad_request(format!("cache keys are 1 to {MAX_CACHE_KEY_CHARS} characters without commas"))
}

fn invalid_version<E>() -> StoreError<E> {
    StoreError::bad_request("the cache version must be 64 lowercase hex digits")
}

/// Lookups, restores, saves, eviction and expiry of the shared cache.
#[derive(Debug)]
pub struct SharedCache;

impl SharedCache {
    /// The entry the task restores for `lookup`: the exact key in each of its read scopes in
    /// order, then for each `restore-keys` prefix the newest match, scope by scope.
    pub async fn find<P: CacheTaskStore>(port: &P, lookup: &CacheLookup<'_>) -> Result<Option<P::Entry>, StoreError<P::Error>> {
        if !valid_cache_key(lookup.key) || lookup.restore_keys.iter().any(|key| !valid_cache_key(key)) {
            return Err(invalid_key());
        }
        if lookup.restore_keys.len() > MAX_RESTORE_KEYS {
            return Err(StoreError::bad_request(format!("a lookup has at most {MAX_RESTORE_KEYS} restore keys")));
        }
        if !valid_cache_version(lookup.version) {
            return Err(invalid_version());
        }
        let target = port.lease().await.map_err(StoreError::Port)?;
        let repository = port.repository(&target);
        for probe in restore_probes(&port.scopes(&target).read, lookup.key, lookup.restore_keys) {
            let found = match probe {
                CacheProbe::Exact { scope, key } => port.exact(repository, scope, key, lookup.version).await,
                CacheProbe::Prefix { scope, prefix } => port.newest_with_prefix(repository, scope, prefix, lookup.version).await,
            }
            .map_err(StoreError::Port)?;
            if found.is_some() {
                return Ok(found);
            }
        }
        Ok(None)
    }

    /// Entry `id`, which the task may restore, and its opened file. Restoring counts as a use.
    pub async fn open<P: CacheTaskStore>(port: &P, id: i64) -> Result<(P::Entry, P::Opened), StoreError<P::Error>> {
        let target = port.lease().await.map_err(StoreError::Port)?;
        let (repository, scopes) = (port.repository(&target), port.scopes(&target));
        let entry = port
            .find(id)
            .await
            .map_err(StoreError::Port)?
            .filter(|entry| {
                let facts = port.facts(entry);
                facts.repository == repository && scopes.read.iter().any(|scope| scope == facts.scope)
            })
            .ok_or_else(|| StoreError::Refused(Refusal::NotFound("this task has no such cache entry".into())))?;
        let opened = port.open(&entry).await.map_err(StoreError::Port)?;
        port.touch(&entry).await.map_err(StoreError::Port)?;
        Ok((entry, opened))
    }

    /// Stores `body` as entry `save.key` in the task's write scope, keeping the repository
    /// within `limit` bytes; `None` when the scope has the key already.
    pub async fn save<P: CacheSaveStore>(
        port: &P,
        save: &CacheSave<'_>,
        limit: u64,
        body: P::Body,
    ) -> Result<Option<P::Entry>, StoreError<P::Error>> {
        if !valid_cache_key(save.key) {
            return Err(invalid_key());
        }
        if !valid_cache_version(save.version) {
            return Err(invalid_version());
        }
        let expected = sha256_from_hex(save.sha256).ok_or_else(|| StoreError::bad_request("the content digest must be 64 hex digits"))?;
        let target = port.lease().await.map_err(StoreError::Port)?;
        let (repository, write) = (port.repository(&target), port.scopes(&target).write.as_str());
        if !save.replace && port.exact(repository, write, save.key, save.version).await.map_err(StoreError::Port)?.is_some() {
            return Ok(None);
        }
        let Some(received) = port.receive(body, limit).await.map_err(StoreError::Port)? else {
            return Err(StoreError::Refused(Refusal::TooLarge("the cache entry is larger than a repository's cache storage".into())));
        };
        if received.sha256 != expected {
            port.discard(&received.temporary).await;
            return Err(StoreError::bad_request("the cache entry does not match its content digest"));
        }
        // The lease may have ended while the body arrived.
        if let Err(error) = port.still_leased(&target).await {
            port.discard(&received.temporary).await;
            return Err(StoreError::Port(error));
        }
        let new =
            NewCacheEntry { repository, scope: write, key: save.key, version: save.version, size: received.size, sha256: received.sha256 };
        let (entry, replaced) = match port.record(&new, save.replace).await.map_err(StoreError::Port)? {
            CacheRecorded::Stored { entry, replaced } => (entry, replaced),
            CacheRecorded::Exists => {
                port.discard(&received.temporary).await;
                return Ok(None);
            }
        };
        // The replaced file goes first: the new row may have the old one's id.
        if let Some(replaced) = &replaced {
            port.remove(&port.location(replaced)).await;
        }
        if let Err(error) = port.place(&received.temporary, &port.location(&entry)).await {
            port.discard(&received.temporary).await;
            port.delete(&entry).await.map_err(StoreError::Port)?;
            return Err(StoreError::Port(error));
        }
        Self::evict(port, &entry, limit).await.map_err(StoreError::Port)?;
        Ok(Some(entry))
    }

    /// Removes `saved`'s repository's least recently used entries, never `saved` itself,
    /// until the repository keeps at most `limit` bytes.
    pub async fn evict<P: CacheRecords>(port: &P, saved: &P::Entry, limit: u64) -> Result<(), P::Error> {
        let saved = port.facts(saved);
        let mut stored = port.stored_bytes(saved.repository).await?;
        if stored <= limit {
            return Ok(());
        }
        for entry in port.least_used(saved.repository, BATCH).await? {
            if stored <= limit {
                break;
            }
            let facts = port.facts(&entry);
            if facts.id == saved.id {
                continue;
            }
            let size = facts.size;
            Self::remove(port, &entry).await?;
            stored = stored.saturating_sub(size);
        }
        Ok(())
    }

    /// Deletes `entry`'s file, then its row.
    pub async fn remove<P: CacheRecords>(port: &P, entry: &P::Entry) -> Result<(), P::Error> {
        port.remove(&port.location(entry)).await;
        port.delete(entry).await
    }

    /// Deletes up to [`BATCH`] entries no job restored in the `retention_days` before `now`,
    /// with their files; how many.
    pub async fn expire<P: CacheRetention>(port: &P, now: i64, retention_days: u32) -> Result<usize, P::Error> {
        let unused = port.unused_since(days_before(now, retention_days), BATCH).await?;
        for entry in &unused {
            Self::remove(port, entry).await?;
        }
        Ok(unused.len())
    }
}
