//! What CI already said about a list of commits (agent plan step 4.9): the finished runs
//! of each, so a bisection starts from recorded results instead of running anything.
use std::collections::BTreeMap;

use super::ActionsRunStore;
use crate::StorageError;
use rbit_core::{RepoId, actions::Status};

impl ActionsRunStore {
    /// The statuses of the finished runs of `shas` in `repo`, of workflow `path` when given,
    /// by commit. A commit without a finished run is absent.
    pub async fn finished_runs_of(&self, repo: RepoId, shas: &[String], path: Option<&str>) -> Result<BTreeMap<String, Vec<Status>>, StorageError> {
        if shas.is_empty() {
            return Ok(BTreeMap::new());
        }
        let wanted = serde_json::to_string(shas).unwrap_or_else(|_| "[]".to_owned());
        let rows: Vec<(String, i64)> = sqlx::query_as(
            "SELECT sha, status FROM actions_runs WHERE repo_id = ? AND sha IN (SELECT value FROM json_each(?)) \
             AND (? IS NULL OR workflow_path = ?) AND status IN (1, 2, 3)",
        )
        .bind(repo.0)
        .bind(wanted)
        .bind(path)
        .bind(path)
        .fetch_all(&self.pool)
        .await?;
        let mut by_sha: BTreeMap<String, Vec<Status>> = BTreeMap::new();
        for (sha, status) in rows {
            let status = match status {
                1 => Status::Success,
                2 => Status::Failure,
                _ => Status::Cancelled,
            };
            by_sha.entry(sha).or_default().push(status);
        }
        Ok(by_sha)
    }
}
