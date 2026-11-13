//! Limits of scoped tokens (migration `0082_token_limits.sql`, agent plan step 4.8): one
//! repository and some rights, beside the token.
use rbit_core::{AccessTokenId, RepoId};

use super::AccessTokenStore;
use crate::StorageError;

/// What a limited token may reach: one repository (when set) and these rights.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredLimit {
    /// Whether it is limited to one repository: `repo`, or none once that was deleted.
    pub scoped: bool,
    pub repo: Option<RepoId>,
    /// Right names, as stored.
    pub rights: Vec<String>,
}

impl AccessTokenStore {
    /// Limits token `token` to `limit`.
    pub async fn set_limit(&self, token: AccessTokenId, limit: &StoredLimit) -> Result<(), StorageError> {
        sqlx::query("INSERT OR REPLACE INTO token_limits (token_id, repo_id, scoped, rights) VALUES (?, ?, ?, ?)")
            .bind(token.0)
            .bind(limit.repo.map(|repo| repo.0))
            .bind(limit.scoped || limit.repo.is_some())
            .bind(limit.rights.join(","))
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// The limit of token `token`; `None` for a token without one.
    pub async fn limit_of(&self, token: AccessTokenId) -> Result<Option<StoredLimit>, StorageError> {
        let row: Option<(Option<i64>, bool, String)> =
            sqlx::query_as("SELECT repo_id, scoped, rights FROM token_limits WHERE token_id = ?")
                .bind(token.0)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.map(|(repo, scoped, rights)| StoredLimit {
            scoped,
            repo: repo.map(RepoId),
            rights: rights.split(',').filter(|right| !right.is_empty()).map(str::to_owned).collect(),
        }))
    }
}
