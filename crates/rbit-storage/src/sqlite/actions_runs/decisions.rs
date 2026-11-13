//! What the trigger decided for each pushed commit (migration
//! `0077_actions_commit_decisions.sql`): whether any workflow started, and why not.
use super::ActionsRunStore;
use crate::StorageError;
use rbit_core::{RepoId, Timestamp};

/// The trigger's decision for one commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitDecision {
    pub workflows: i64,
    pub started: i64,
    /// `started`, `skip_marker`, `no_workflows`, `filtered`, `too_many_refs` or `no_jobs`.
    pub reason: String,
}

impl ActionsRunStore {
    /// Records the decision for commit `sha` of `repo`; a decision that started more runs
    /// replaces one that started fewer.
    pub async fn record_decision(&self, repo: RepoId, sha: &str, decision: &CommitDecision, now: Timestamp) -> Result<(), StorageError> {
        sqlx::query(
            "INSERT INTO actions_commit_decisions (repo_id, sha, workflows, started, reason, recorded_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
             ON CONFLICT (repo_id, sha) DO UPDATE SET workflows = excluded.workflows, started = excluded.started, \
             reason = excluded.reason, recorded_at = excluded.recorded_at WHERE excluded.started > actions_commit_decisions.started",
        )
        .bind(repo.0)
        .bind(sha)
        .bind(decision.workflows)
        .bind(decision.started)
        .bind(&decision.reason)
        .bind(now.0)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// The decision recorded for commit `sha` of `repo`, if the commit was pushed since.
    pub async fn decision(&self, repo: RepoId, sha: &str) -> Result<Option<CommitDecision>, StorageError> {
        let row: Option<(i64, i64, String)> =
            sqlx::query_as("SELECT workflows, started, reason FROM actions_commit_decisions WHERE repo_id = ? AND sha = ?")
                .bind(repo.0)
                .bind(sha)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.map(|(workflows, started, reason)| CommitDecision { workflows, started, reason }))
    }
}
