//! Actions: shared cache. The rows of `actions/cache` entries (migration
//! `0058_actions_caches.sql`). The files live under `<data_dir>/actions/caches`, written and
//! removed by rbit-services' `actions::caches`, which also enforces sizes and quotas.

use rbit_core::{
    RepoId, Timestamp,
    actions::{CacheEntry, CacheEntryId},
};
use sqlx::SqlitePool;

use super::actions_rows::corrupt;
use crate::StorageError;

#[derive(Debug, sqlx::FromRow)]
struct CacheRow {
    id: i64,
    repo_id: i64,
    scope: String,
    key: String,
    version: String,
    size: i64,
    sha256: Vec<u8>,
    created_at: i64,
    used_at: i64,
}

impl TryFrom<CacheRow> for CacheEntry {
    type Error = StorageError;

    fn try_from(row: CacheRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: CacheEntryId(row.id),
            repo_id: RepoId(row.repo_id),
            scope: row.scope,
            key: row.key,
            version: row.version,
            size: u64::try_from(row.size).map_err(|error| corrupt("cache entry size", error))?,
            sha256: row.sha256.as_slice().try_into().map_err(|_| corrupt("cache entry digest", "not 32 bytes"))?,
            created_at: Timestamp(row.created_at),
            used_at: Timestamp(row.used_at),
        })
    }
}

macro_rules! select_cache {
    ($clause:expr) => {
        concat!("SELECT id, repo_id, scope, key, version, size, sha256, created_at, used_at FROM actions_caches ", $clause)
    };
}

/// A finished upload, to record.
#[derive(Debug, Clone)]
pub struct NewCacheEntry<'a> {
    pub repo: RepoId,
    pub scope: &'a str,
    pub key: &'a str,
    pub version: &'a str,
    pub size: u64,
    pub sha256: [u8; 32],
    pub now: Timestamp,
}

/// What recording an upload did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CacheRecorded {
    /// The new entry, and the one of the same key it replaced (whose file goes).
    Stored { entry: Box<CacheEntry>, replaced: Option<Box<CacheEntry>> },
    /// The scope has the key already and replacing was not asked.
    Exists,
}

// Actions: cache REST API.
/// Which entries a list shows.
#[derive(Debug, Clone, Copy, Default)]
pub struct CacheFilter<'a> {
    /// Keys starting with this.
    pub key_prefix: Option<&'a str>,
    pub scope: Option<&'a str>,
}

/// The order of a list, GitHub's `sort` and `direction`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CacheOrder {
    #[default]
    UsedDesc,
    UsedAsc,
    CreatedDesc,
    CreatedAsc,
    SizeDesc,
    SizeAsc,
}

#[derive(Debug, Clone)]
pub struct ActionsCacheStore {
    pool: SqlitePool,
}

impl ActionsCacheStore {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Records `new`, replacing an entry of the same key and version in its scope only when
    /// `replace`.
    pub async fn record(&self, new: &NewCacheEntry<'_>, replace: bool) -> Result<CacheRecorded, StorageError> {
        let size = i64::try_from(new.size).map_err(|error| corrupt("cache entry size", error))?;
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let existing: Option<CacheRow> = sqlx::query_as(select_cache!("WHERE repo_id = ? AND scope = ? AND key = ? AND version = ?"))
            .bind(new.repo.0)
            .bind(new.scope)
            .bind(new.key)
            .bind(new.version)
            .fetch_optional(&mut *tx)
            .await?;
        let replaced = match existing {
            Some(_) if !replace => return Ok(CacheRecorded::Exists),
            Some(row) => {
                sqlx::query("DELETE FROM actions_caches WHERE id = ?").bind(row.id).execute(&mut *tx).await?;
                Some(Box::new(CacheEntry::try_from(row)?))
            }
            None => None,
        };
        let row: CacheRow = sqlx::query_as(
            "INSERT INTO actions_caches (repo_id, scope, key, version, size, sha256, created_at, used_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?) \
             RETURNING id, repo_id, scope, key, version, size, sha256, created_at, used_at",
        )
        .bind(new.repo.0)
        .bind(new.scope)
        .bind(new.key)
        .bind(new.version)
        .bind(size)
        .bind(&new.sha256[..])
        .bind(new.now.0)
        .bind(new.now.0)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(CacheRecorded::Stored { entry: Box::new(CacheEntry::try_from(row)?), replaced })
    }

    /// The entry for exactly `key` and `version` in `scope` of `repo`.
    pub async fn exact(&self, repo: RepoId, scope: &str, key: &str, version: &str) -> Result<Option<CacheEntry>, StorageError> {
        let row: Option<CacheRow> = sqlx::query_as(select_cache!("WHERE repo_id = ? AND scope = ? AND key = ? AND version = ?"))
            .bind(repo.0)
            .bind(scope)
            .bind(key)
            .bind(version)
            .fetch_optional(&self.pool)
            .await?;
        row.map(CacheEntry::try_from).transpose()
    }

    /// The newest entry of `version` in `scope` of `repo` whose key starts with `prefix`
    /// (byte for byte, as GitHub's `restore-keys` match).
    pub async fn newest_with_prefix(
        &self,
        repo: RepoId,
        scope: &str,
        prefix: &str,
        version: &str,
    ) -> Result<Option<CacheEntry>, StorageError> {
        let row: Option<CacheRow> = sqlx::query_as(select_cache!(
            "WHERE repo_id = ?1 AND scope = ?2 AND version = ?3 AND substr(key, 1, length(?4)) = ?4 \
             ORDER BY created_at DESC, id DESC LIMIT 1"
        ))
        .bind(repo.0)
        .bind(scope)
        .bind(version)
        .bind(prefix)
        .fetch_optional(&self.pool)
        .await?;
        row.map(CacheEntry::try_from).transpose()
    }

    /// Entry `id`.
    pub async fn find(&self, id: CacheEntryId) -> Result<Option<CacheEntry>, StorageError> {
        let row: Option<CacheRow> = sqlx::query_as(select_cache!("WHERE id = ?")).bind(id.0).fetch_optional(&self.pool).await?;
        row.map(CacheEntry::try_from).transpose()
    }

    /// Notes that a job restored entry `id` at `now`.
    pub async fn touch(&self, id: CacheEntryId, now: Timestamp) -> Result<(), StorageError> {
        sqlx::query("UPDATE actions_caches SET used_at = MAX(used_at, ?) WHERE id = ?").bind(now.0).bind(id.0).execute(&self.pool).await?;
        Ok(())
    }

    /// Bytes of cache entries `repo` keeps.
    pub async fn stored_bytes(&self, repo: RepoId) -> Result<u64, StorageError> {
        let bytes: i64 = sqlx::query_scalar("SELECT COALESCE(SUM(size), 0) FROM actions_caches WHERE repo_id = ?")
            .bind(repo.0)
            .fetch_one(&self.pool)
            .await?;
        Ok(u64::try_from(bytes).unwrap_or_default())
    }

    /// Up to `limit` of `repo`'s entries, least recently used first, for eviction.
    pub async fn least_used(&self, repo: RepoId, limit: u32) -> Result<Vec<CacheEntry>, StorageError> {
        let rows: Vec<CacheRow> = sqlx::query_as(select_cache!("WHERE repo_id = ? ORDER BY used_at, id LIMIT ?"))
            .bind(repo.0)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter().map(CacheEntry::try_from).collect()
    }

    /// Up to `limit` entries not used since `before`, oldest first, for the sweeper.
    pub async fn unused_since(&self, before: Timestamp, limit: u32) -> Result<Vec<CacheEntry>, StorageError> {
        let rows: Vec<CacheRow> = sqlx::query_as(select_cache!("WHERE used_at < ? ORDER BY used_at LIMIT ?"))
            .bind(before.0)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter().map(CacheEntry::try_from).collect()
    }

    // Actions: cache REST API.
    /// A page of `repo`'s entries, filtered by key prefix and scope, in `order`, and how many
    /// match in all.
    pub async fn of_repo(
        &self,
        repo: RepoId,
        filter: &CacheFilter<'_>,
        order: CacheOrder,
        offset: u32,
        limit: u32,
    ) -> Result<(Vec<CacheEntry>, u64), StorageError> {
        macro_rules! filtered {
            ($start:expr, $end:expr) => {
                concat!(
                    $start,
                    "WHERE repo_id = ?1 AND (?2 IS NULL OR substr(key, 1, length(?2)) = ?2) AND (?3 IS NULL OR scope = ?3)",
                    $end
                )
            };
        }
        macro_rules! page {
            ($order:expr) => {
                filtered!(select_cache!(""), concat!(" ORDER BY ", $order, " LIMIT ?4 OFFSET ?5"))
            };
        }
        let query = match order {
            CacheOrder::UsedDesc => page!("used_at DESC, id DESC"),
            CacheOrder::UsedAsc => page!("used_at, id"),
            CacheOrder::CreatedDesc => page!("created_at DESC, id DESC"),
            CacheOrder::CreatedAsc => page!("created_at, id"),
            CacheOrder::SizeDesc => page!("size DESC, id DESC"),
            CacheOrder::SizeAsc => page!("size, id"),
        };
        let rows: Vec<CacheRow> = sqlx::query_as(query)
            .bind(repo.0)
            .bind(filter.key_prefix)
            .bind(filter.scope)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await?;
        let total: i64 = sqlx::query_scalar(filtered!("SELECT COUNT(*) FROM actions_caches ", ""))
            .bind(repo.0)
            .bind(filter.key_prefix)
            .bind(filter.scope)
            .fetch_one(&self.pool)
            .await?;
        Ok((rows.into_iter().map(CacheEntry::try_from).collect::<Result<_, _>>()?, u64::try_from(total).unwrap_or_default()))
    }

    /// `repo`'s entries of exactly `key`, in `scope` when given.
    pub async fn with_key(&self, repo: RepoId, key: &str, scope: Option<&str>) -> Result<Vec<CacheEntry>, StorageError> {
        let rows: Vec<CacheRow> =
            sqlx::query_as(select_cache!("WHERE repo_id = ?1 AND key = ?2 AND (?3 IS NULL OR scope = ?3) ORDER BY id"))
                .bind(repo.0)
                .bind(key)
                .bind(scope)
                .fetch_all(&self.pool)
                .await?;
        rows.into_iter().map(CacheEntry::try_from).collect()
    }

    /// How many entries `repo` keeps, and their bytes.
    pub async fn usage(&self, repo: RepoId) -> Result<(u64, u64), StorageError> {
        let (count, bytes): (i64, i64) = sqlx::query_as("SELECT COUNT(*), COALESCE(SUM(size), 0) FROM actions_caches WHERE repo_id = ?")
            .bind(repo.0)
            .fetch_one(&self.pool)
            .await?;
        Ok((u64::try_from(count).unwrap_or_default(), u64::try_from(bytes).unwrap_or_default()))
    }

    /// Deletes entry `id`'s row; its file is the caller's.
    pub async fn delete(&self, id: CacheEntryId) -> Result<bool, StorageError> {
        let result = sqlx::query("DELETE FROM actions_caches WHERE id = ?").bind(id.0).execute(&self.pool).await?;
        Ok(result.rows_affected() == 1)
    }
}
