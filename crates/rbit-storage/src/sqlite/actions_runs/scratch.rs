//! Scratch refs (migration `0078_actions_scratch_refs.sql`): recorded at push, counted for
//! the owner's quota, deleted once they expire.
use super::ActionsRunStore;
use crate::StorageError;
use rbit_core::{AccountId, RepoId, Timestamp};

/// A recorded scratch ref.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScratchRef {
    pub repo: RepoId,
    pub name: String,
    pub sha: String,
}

impl ActionsRunStore {
    /// Records that `name` of `repo` now points at `sha`, pushed by `owner`; a push to an
    /// existing scratch ref starts its lifetime again.
    pub async fn record_scratch(&self, repo: RepoId, name: &str, owner: Option<AccountId>, sha: &str, now: Timestamp) -> Result<(), StorageError> {
        sqlx::query(
            "INSERT INTO actions_scratch_refs (repo_id, name, owner_id, sha, created_at) VALUES (?1, ?2, ?3, ?4, ?5) \
             ON CONFLICT (repo_id, name) DO UPDATE SET sha = excluded.sha, created_at = excluded.created_at",
        )
        .bind(repo.0)
        .bind(name)
        .bind(owner.map(|owner| owner.0))
        .bind(sha)
        .bind(now.0)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// How many scratch refs `owner` has in `repo`, other than `except`.
    pub async fn count_scratch(&self, repo: RepoId, owner: AccountId, except: &str) -> Result<u64, StorageError> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM actions_scratch_refs WHERE repo_id = ? AND owner_id = ? AND name <> ?")
            .bind(repo.0)
            .bind(owner.0)
            .bind(except)
            .fetch_one(&self.pool)
            .await?;
        Ok(u64::try_from(count).unwrap_or_default())
    }

    /// Scratch refs recorded before `before`, oldest first, at most `limit`.
    pub async fn expired_scratch(&self, before: Timestamp, limit: u32) -> Result<Vec<ScratchRef>, StorageError> {
        let rows: Vec<(i64, String, String)> =
            sqlx::query_as("SELECT repo_id, name, sha FROM actions_scratch_refs WHERE created_at < ? ORDER BY created_at LIMIT ?")
                .bind(before.0)
                .bind(limit)
                .fetch_all(&self.pool)
                .await?;
        Ok(rows.into_iter().map(|(repo, name, sha)| ScratchRef { repo: RepoId(repo), name, sha }).collect())
    }

    /// Forgets a scratch ref, unless a newer push moved it to another commit meanwhile.
    pub async fn forget_scratch(&self, repo: RepoId, name: &str, sha: &str) -> Result<(), StorageError> {
        sqlx::query("DELETE FROM actions_scratch_refs WHERE repo_id = ? AND name = ? AND sha = ?").bind(repo.0).bind(name).bind(sha).execute(&self.pool).await?;
        Ok(())
    }
}
