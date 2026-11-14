//! How far runs are: their jobs counted by state, for the runs list.
use std::collections::BTreeMap;

use super::ActionsRunStore;
use crate::StorageError;
use rbit_core::actions::RunId;

/// A run's jobs by state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RunProgress {
    pub total: u64,
    /// Finished, whatever the result: succeeded, failed, cancelled or skipped.
    pub done: u64,
    pub running: u64,
    /// Waiting for a runner.
    pub waiting: u64,
}

impl ActionsRunStore {
    /// The job counts of `runs`, read in one query; a run without jobs is left out.
    pub async fn progress(&self, runs: &[RunId]) -> Result<BTreeMap<RunId, RunProgress>, StorageError> {
        if runs.is_empty() {
            return Ok(BTreeMap::new());
        }
        let ids = format!("[{}]", runs.iter().map(|run| run.0.to_string()).collect::<Vec<_>>().join(","));
        let rows: Vec<(i64, i64, i64, i64, i64)> = sqlx::query_as(
            "SELECT run_id, COUNT(*), COALESCE(SUM(status IN (1, 2, 3, 4)), 0), COALESCE(SUM(status = 6), 0), \
             COALESCE(SUM(status = 5), 0) FROM actions_jobs WHERE run_id IN (SELECT value FROM json_each(?)) GROUP BY run_id",
        )
        .bind(ids)
        .fetch_all(&self.pool)
        .await?;
        let count = |value: i64| u64::try_from(value).unwrap_or_default();
        Ok(rows
            .into_iter()
            .map(|(run, total, done, running, waiting)| {
                (RunId(run), RunProgress { total: count(total), done: count(done), running: count(running), waiting: count(waiting) })
            })
            .collect())
    }
}
