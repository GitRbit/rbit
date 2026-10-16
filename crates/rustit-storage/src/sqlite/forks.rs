//! Fork relationships: which repository a fork was created from.
//!
//! Milestone 2 workstream R (repository essentials) owns this file. Keep row types here rather than in `rows.rs`.

use rustit_core::{AccountId, RepoId, Timestamp, repository::OwnedRepository};
use sqlx::SqlitePool;

use super::rows::OwnedRepositoryRow;
use crate::StorageError;

/// Repository columns aliased like [`OwnedRepositoryRow`], for `r` joined with its owner `o`.
macro_rules! owned_repo_columns {
    () => {
        "r.id, r.owner_id, r.name, r.description, r.visibility, r.default_branch, r.created_at, r.updated_at, \
         r.pushed_at, o.kind AS owner_kind, o.name AS owner_name, o.display_name AS owner_display_name, \
         o.bio AS owner_bio, o.created_at AS owner_created_at"
    };
}

/// Repositories `r` the viewer `?2` (site admin when `?3`) may read: public, owned by
/// them, or owned by an organisation they belong to — the rules of
/// `rustit_auth::repo_access` as of milestone 1, expressed in SQL like `StarStore::starred_by`.
macro_rules! readable_by_viewer {
    () => {
        "(r.visibility = 'public' OR ?3 OR r.owner_id = ?2 \
          OR EXISTS (SELECT 1 FROM org_members m WHERE m.org_id = r.owner_id AND m.user_id = ?2))"
    };
}

#[derive(Debug, Clone)]
pub struct ForkStore {
    pool: SqlitePool,
}

impl ForkStore {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Records that `fork` was created from `parent`.
    pub async fn record(&self, fork: RepoId, parent: RepoId) -> Result<(), StorageError> {
        sqlx::query("INSERT INTO repo_forks (repo_id, parent_id, created_at) VALUES (?, ?, ?)")
            .bind(fork.0)
            .bind(parent.0)
            .bind(Timestamp::now().seconds())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// The repository `fork` was created from, if it still exists.
    pub async fn parent_of(&self, fork: RepoId) -> Result<Option<OwnedRepository>, StorageError> {
        let row: Option<OwnedRepositoryRow> = sqlx::query_as(concat!(
            "SELECT ",
            owned_repo_columns!(),
            " FROM repo_forks f JOIN repositories r ON r.id = f.parent_id JOIN accounts o ON o.id = r.owner_id WHERE f.repo_id = ?"
        ))
        .bind(fork.0)
        .fetch_optional(&self.pool)
        .await?;
        row.map(OwnedRepository::try_from).transpose()
    }

    /// Direct forks of `parent` that the viewer may read.
    pub async fn count_readable(&self, parent: RepoId, viewer: Option<AccountId>, viewer_is_admin: bool) -> Result<u64, StorageError> {
        let count: i64 = sqlx::query_scalar(concat!(
            "SELECT COUNT(*) FROM repo_forks f JOIN repositories r ON r.id = f.repo_id WHERE f.parent_id = ?1 AND ",
            readable_by_viewer!()
        ))
        .bind(parent.0)
        .bind(viewer.map(|id| id.0))
        .bind(viewer_is_admin)
        .fetch_one(&self.pool)
        .await?;
        Ok(u64::try_from(count).unwrap_or_default())
    }

    /// Direct forks of `parent` that the viewer may read, newest first.
    pub async fn list_readable(
        &self,
        parent: RepoId,
        viewer: Option<AccountId>,
        viewer_is_admin: bool,
        limit: u32,
    ) -> Result<Vec<OwnedRepository>, StorageError> {
        let rows: Vec<OwnedRepositoryRow> = sqlx::query_as(concat!(
            "SELECT ",
            owned_repo_columns!(),
            " FROM repo_forks f JOIN repositories r ON r.id = f.repo_id JOIN accounts o ON o.id = r.owner_id \
             WHERE f.parent_id = ?1 AND ",
            readable_by_viewer!(),
            " ORDER BY f.created_at DESC, r.id DESC LIMIT ?4"
        ))
        .bind(parent.0)
        .bind(viewer.map(|id| id.0))
        .bind(viewer_is_admin)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(OwnedRepository::try_from).collect()
    }
}
