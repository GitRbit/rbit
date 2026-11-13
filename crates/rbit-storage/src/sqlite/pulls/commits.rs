//! The pull requests a commit belongs to (agent plan step 4.5): as its head, its merge
//! commit or the head it merged, open or not.
use rbit_core::RepoId;

use super::{Issue, ListedRow, PullRequest, PullStore};
use crate::StorageError;

/// Most pull requests [`PullStore::with_commit`] returns.
const MAX_PULLS_FOR_COMMIT: i64 = 50;

impl PullStore {
    /// Pull requests of `repo` whose head is `sha`, that merged as `sha` or that merged
    /// head `sha`, newest first, at most 50.
    pub async fn with_commit(&self, repo: RepoId, sha: &str) -> Result<Vec<(Issue, PullRequest)>, StorageError> {
        let rows: Vec<ListedRow> = sqlx::query_as(concat!(
            "SELECT ",
            pull_columns!(),
            ", ",
            issue_columns!(),
            " FROM pull_requests p JOIN issues i ON i.id = p.issue_id \
             WHERE p.repo_id = ?1 AND (p.head_sha = ?2 OR p.merge_commit_sha = ?2 OR p.merged_head_sha = ?2) ORDER BY i.id DESC LIMIT ?3"
        ))
        .bind(repo.0)
        .bind(sha)
        .bind(MAX_PULLS_FOR_COMMIT)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(ListedRow::into_pair).collect()
    }
}
