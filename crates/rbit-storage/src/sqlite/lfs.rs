//! Git LFS: stored objects, the repositories that may use them, and file locks. The
//! bytes of objects live on disk; see `LfsService`.
//!
//! Milestone 3 workstream L (Git LFS) owns this file. Keep row types here rather than in `rows.rs`.

use std::collections::HashMap;

use rbit_core::{
    AccountId, RepoId, Timestamp,
    lfs::{LfsLock, LfsLockId, LfsOid, LockOwner},
};
use sqlx::SqlitePool;

use crate::StorageError;

/// Lock columns in [`LockRow`] order, joined with the owner's name.
macro_rules! select_lock {
    ($($clause:expr),+) => {
        concat!(
            "SELECT l.id, l.repo_id, l.path, l.owner_id, a.name AS owner_name, l.created_at \
             FROM lfs_locks l JOIN accounts a ON a.id = l.owner_id ",
            $($clause),+
        )
    };
}

/// A stored object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LfsObject {
    pub oid: LfsOid,
    pub size: u64,
    pub created_at: Timestamp,
}

#[derive(Debug, sqlx::FromRow)]
struct ObjectRow {
    oid: String,
    size: i64,
    created_at: i64,
}

impl TryFrom<ObjectRow> for LfsObject {
    type Error = StorageError;

    fn try_from(row: ObjectRow) -> Result<Self, Self::Error> {
        Ok(Self {
            oid: LfsOid::parse(&row.oid).map_err(|e| StorageError::Corrupt(e.to_string()))?,
            size: u64::try_from(row.size).map_err(|_| StorageError::Corrupt("negative object size".to_owned()))?,
            created_at: Timestamp(row.created_at),
        })
    }
}

#[derive(Debug, sqlx::FromRow)]
struct LockRow {
    id: i64,
    repo_id: i64,
    path: String,
    owner_id: i64,
    owner_name: String,
    created_at: i64,
}

impl From<LockRow> for LfsLock {
    fn from(row: LockRow) -> Self {
        Self {
            id: LfsLockId(row.id),
            repo_id: RepoId(row.repo_id),
            path: row.path,
            owner: LockOwner { id: AccountId(row.owner_id), name: row.owner_name },
            locked_at: Timestamp(row.created_at),
        }
    }
}

/// Which locks of a repository to list (the locks API's query parameters).
#[derive(Debug, Clone, Default)]
pub struct LockFilter<'a> {
    /// Only the lock of exactly this path.
    pub path: Option<&'a str>,
    /// Only this lock.
    pub id: Option<LfsLockId>,
    /// Only locks with a larger id (the cursor of the previous page).
    pub after: Option<LfsLockId>,
    pub limit: u32,
}

#[derive(Debug, Clone)]
pub struct LfsStore {
    pool: SqlitePool,
}

fn size_param(size: u64) -> Result<i64, StorageError> {
    i64::try_from(size).map_err(|_| StorageError::Corrupt("object too large".to_owned()))
}

impl LfsStore {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    // ------------------------------------------------------------------ objects

    pub async fn find_object(&self, oid: &LfsOid) -> Result<Option<LfsObject>, StorageError> {
        let row: Option<ObjectRow> = sqlx::query_as("SELECT oid, size, created_at FROM lfs_objects WHERE oid = ?")
            .bind(oid.as_str())
            .fetch_optional(&self.pool)
            .await?;
        row.map(LfsObject::try_from).transpose()
    }

    /// Records that the bytes of `oid` are in place and links the object to `repo`, in
    /// one transaction. Recording an object that exists already only adds the link (and
    /// ends any grace period it was in).
    pub async fn record_object(&self, oid: &LfsOid, size: u64, repo: RepoId) -> Result<(), StorageError> {
        let now = Timestamp::now().seconds();
        let mut tx = self.pool.begin().await?;
        sqlx::query("INSERT INTO lfs_objects (oid, size, created_at) VALUES (?, ?, ?) ON CONFLICT (oid) DO UPDATE SET unlinked_at = NULL")
            .bind(oid.as_str())
            .bind(size_param(size)?)
            .bind(now)
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT OR IGNORE INTO lfs_repo_objects (repo_id, oid, created_at) VALUES (?, ?, ?)")
            .bind(repo.0)
            .bind(oid.as_str())
            .bind(now)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Links the stored object `oid` to `repo`; `false` when there is no such object.
    pub async fn link(&self, repo: RepoId, oid: &LfsOid) -> Result<bool, StorageError> {
        let result = sqlx::query(
            "INSERT OR IGNORE INTO lfs_repo_objects (repo_id, oid, created_at) SELECT ?1, oid, ?3 FROM lfs_objects WHERE oid = ?2",
        )
        .bind(repo.0)
        .bind(oid.as_str())
        .bind(Timestamp::now().seconds())
        .execute(&self.pool)
        .await?;
        if result.rows_affected() > 0 {
            return Ok(true);
        }
        self.is_linked(repo, oid).await
    }

    pub async fn is_linked(&self, repo: RepoId, oid: &LfsOid) -> Result<bool, StorageError> {
        let linked: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM lfs_repo_objects WHERE repo_id = ? AND oid = ?")
            .bind(repo.0)
            .bind(oid.as_str())
            .fetch_one(&self.pool)
            .await?;
        Ok(linked > 0)
    }

    /// Those of `oids` that are linked to `repo`, with their sizes (one query for a batch).
    pub async fn linked_objects(&self, repo: RepoId, oids: &[LfsOid]) -> Result<HashMap<LfsOid, u64>, StorageError> {
        if oids.is_empty() {
            return Ok(HashMap::new());
        }
        let list = serde_json::to_string(&oids.iter().map(LfsOid::as_str).collect::<Vec<_>>())
            .map_err(|e| StorageError::Corrupt(e.to_string()))?;
        let rows: Vec<ObjectRow> = sqlx::query_as(
            "SELECT o.oid, o.size, o.created_at FROM lfs_objects o JOIN lfs_repo_objects l ON l.oid = o.oid \
             WHERE l.repo_id = ? AND o.oid IN (SELECT value FROM json_each(?))",
        )
        .bind(repo.0)
        .bind(list)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(|row| LfsObject::try_from(row).map(|object| (object.oid, object.size))).collect()
    }

    /// The first of `repos` that `oid` is linked to, if any.
    pub async fn linked_in_any(&self, repos: &[RepoId], oid: &LfsOid) -> Result<Option<RepoId>, StorageError> {
        if repos.is_empty() {
            return Ok(None);
        }
        let list = format!("[{}]", repos.iter().map(|id| id.0.to_string()).collect::<Vec<_>>().join(","));
        let found: Option<i64> = sqlx::query_scalar(
            "SELECT repo_id FROM lfs_repo_objects WHERE oid = ? AND repo_id IN (SELECT value FROM json_each(?)) ORDER BY repo_id LIMIT 1",
        )
        .bind(oid.as_str())
        .bind(list)
        .fetch_optional(&self.pool)
        .await?;
        Ok(found.map(RepoId))
    }

    /// Links every object of `from` to `to` as well (a new fork); returns how many links
    /// were added.
    pub async fn copy_links(&self, from: RepoId, to: RepoId) -> Result<u64, StorageError> {
        let result = sqlx::query(
            "INSERT OR IGNORE INTO lfs_repo_objects (repo_id, oid, created_at) SELECT ?2, oid, ?3 FROM lfs_repo_objects WHERE repo_id = ?1",
        )
        .bind(from.0)
        .bind(to.0)
        .bind(Timestamp::now().seconds())
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    /// How many objects `repo` uses and their total size.
    pub async fn usage(&self, repo: RepoId) -> Result<(u64, u64), StorageError> {
        let (count, bytes): (i64, i64) = sqlx::query_as(
            "SELECT COUNT(*), COALESCE(SUM(o.size), 0) FROM lfs_repo_objects l JOIN lfs_objects o ON o.oid = l.oid WHERE l.repo_id = ?",
        )
        .bind(repo.0)
        .fetch_one(&self.pool)
        .await?;
        Ok((u64::try_from(count).unwrap_or_default(), u64::try_from(bytes).unwrap_or_default()))
    }

    /// Up to `limit` objects no repository links any more, whose last use (creation or
    /// unlinking) is older than `cutoff`.
    pub async fn orphans(&self, cutoff: Timestamp, limit: u32) -> Result<Vec<LfsOid>, StorageError> {
        let rows: Vec<String> = sqlx::query_scalar(
            "SELECT oid FROM lfs_objects o WHERE NOT EXISTS (SELECT 1 FROM lfs_repo_objects l WHERE l.oid = o.oid) \
             AND MAX(created_at, COALESCE(unlinked_at, 0)) < ? ORDER BY oid LIMIT ?",
        )
        .bind(cutoff.seconds())
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(|oid| LfsOid::parse(oid).map_err(|e| StorageError::Corrupt(e.to_string()))).collect()
    }

    /// Removes the row of `oid` if it is still an orphan older than `cutoff` (checked in
    /// the same statement, so a link added meanwhile keeps it); `true` when removed.
    pub async fn delete_orphan(&self, oid: &LfsOid, cutoff: Timestamp) -> Result<bool, StorageError> {
        let result = sqlx::query(
            "DELETE FROM lfs_objects WHERE oid = ? AND NOT EXISTS (SELECT 1 FROM lfs_repo_objects l WHERE l.oid = lfs_objects.oid) \
             AND MAX(created_at, COALESCE(unlinked_at, 0)) < ?",
        )
        .bind(oid.as_str())
        .bind(cutoff.seconds())
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    // ------------------------------------------------------------------ locks

    /// Locks `path`; [`StorageError::Conflict`] when it is locked already.
    pub async fn create_lock(&self, repo: RepoId, path: &str, owner: AccountId) -> Result<LfsLock, StorageError> {
        let id: i64 = sqlx::query_scalar("INSERT INTO lfs_locks (repo_id, path, owner_id, created_at) VALUES (?, ?, ?, ?) RETURNING id")
            .bind(repo.0)
            .bind(path)
            .bind(owner.0)
            .bind(Timestamp::now().seconds())
            .fetch_one(&self.pool)
            .await
            .map_err(|e| StorageError::from_insert(e, "a lock for this path"))?;
        self.find_lock(repo, LfsLockId(id)).await?.ok_or_else(|| StorageError::Corrupt("a new lock vanished".to_owned()))
    }

    pub async fn find_lock(&self, repo: RepoId, id: LfsLockId) -> Result<Option<LfsLock>, StorageError> {
        let row: Option<LockRow> =
            sqlx::query_as(select_lock!("WHERE l.repo_id = ? AND l.id = ?")).bind(repo.0).bind(id.0).fetch_optional(&self.pool).await?;
        Ok(row.map(LfsLock::from))
    }

    pub async fn find_lock_by_path(&self, repo: RepoId, path: &str) -> Result<Option<LfsLock>, StorageError> {
        let row: Option<LockRow> =
            sqlx::query_as(select_lock!("WHERE l.repo_id = ? AND l.path = ?")).bind(repo.0).bind(path).fetch_optional(&self.pool).await?;
        Ok(row.map(LfsLock::from))
    }

    /// Locks of `repo` matching `filter`, oldest (lowest id) first.
    pub async fn list_locks(&self, repo: RepoId, filter: &LockFilter<'_>) -> Result<Vec<LfsLock>, StorageError> {
        let rows: Vec<LockRow> = sqlx::query_as(select_lock!(
            "WHERE l.repo_id = ?1 AND (?2 IS NULL OR l.path = ?2) AND (?3 IS NULL OR l.id = ?3) AND (?4 IS NULL OR l.id > ?4) \
             ORDER BY l.id LIMIT ?5"
        ))
        .bind(repo.0)
        .bind(filter.path)
        .bind(filter.id.map(|id| id.0))
        .bind(filter.after.map(|id| id.0))
        .bind(filter.limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(LfsLock::from).collect())
    }

    /// Removes lock `id` of `repo`; `true` when it existed.
    pub async fn delete_lock(&self, repo: RepoId, id: LfsLockId) -> Result<bool, StorageError> {
        let result = sqlx::query("DELETE FROM lfs_locks WHERE repo_id = ? AND id = ?").bind(repo.0).bind(id.0).execute(&self.pool).await?;
        Ok(result.rows_affected() > 0)
    }
}
