//! Test reports of job attempts (migrations `0079_actions_test_reports.sql` and
//! `0085_actions_test_passes.sql`), and the tests that both failed and passed on one commit
//! (agent plan steps 3.4 and 3.6).
use std::collections::{BTreeMap, BTreeSet};

use super::ActionsRunStore;
use crate::StorageError;
use rbit_core::{
    RepoId,
    actions::{RunId, TaskId},
};

/// A failed test as stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredTest {
    pub id: String,
    pub path: Option<String>,
    pub line: Option<u64>,
    pub message: String,
}

/// A job attempt's test report as stored: counts, and the first failed tests in order.
/// `passed` (the ids of the first passed tests) is stored but not read back.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StoredReport {
    pub total: u64,
    pub failed: u64,
    pub items: Vec<StoredTest>,
    pub passed: Vec<String>,
}

/// A failed test's task, id, path, line and message, as read.
type TestRow = (i64, String, Option<String>, Option<i64>, String);

/// Failed tests kept per attempt, whatever the caller passes.
const MAX_STORED_TESTS: usize = 100;
/// Passed test ids kept per attempt, whatever the caller passes.
const MAX_STORED_PASSES: usize = 2000;

impl ActionsRunStore {
    /// Stores `report` as task `task`'s, replacing an earlier one.
    pub async fn record_test_report(&self, task: TaskId, report: &StoredReport) -> Result<(), StorageError> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        sqlx::query("DELETE FROM actions_test_failures WHERE task_id = ?").bind(task.0).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM actions_test_passes WHERE task_id = ?").bind(task.0).execute(&mut *tx).await?;
        for test in report.passed.iter().filter(|test| !test.is_empty() && test.len() <= 1100).take(MAX_STORED_PASSES) {
            sqlx::query("INSERT OR IGNORE INTO actions_test_passes (task_id, test_id) VALUES (?, ?)")
                .bind(task.0)
                .bind(test)
                .execute(&mut *tx)
                .await?;
        }
        sqlx::query("INSERT OR REPLACE INTO actions_test_reports (task_id, total, failed) VALUES (?, ?, ?)")
            .bind(task.0)
            .bind(i64::try_from(report.total).unwrap_or(i64::MAX))
            .bind(i64::try_from(report.failed).unwrap_or(i64::MAX))
            .execute(&mut *tx)
            .await?;
        for (ordinal, test) in (0_i64..).zip(report.items.iter().take(MAX_STORED_TESTS)) {
            sqlx::query("INSERT INTO actions_test_failures (task_id, ordinal, test_id, path, line, message) VALUES (?, ?, ?, ?, ?, ?)")
                .bind(task.0)
                .bind(ordinal)
                .bind(&test.id)
                .bind(&test.path)
                .bind(test.line.and_then(|line| i64::try_from(line).ok()))
                .bind(&test.message)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// The test reports of those of `tasks` that have one.
    pub async fn test_reports(&self, tasks: &[TaskId]) -> Result<BTreeMap<TaskId, StoredReport>, StorageError> {
        if tasks.is_empty() {
            return Ok(BTreeMap::new());
        }
        let ids = format!("[{}]", tasks.iter().map(|task| task.0.to_string()).collect::<Vec<_>>().join(","));
        let counts: Vec<(i64, i64, i64)> =
            sqlx::query_as("SELECT task_id, total, failed FROM actions_test_reports WHERE task_id IN (SELECT value FROM json_each(?))")
                .bind(&ids)
                .fetch_all(&self.pool)
                .await?;
        let mut reports: BTreeMap<TaskId, StoredReport> = counts
            .into_iter()
            .map(|(task, total, failed)| {
                let count = |value: i64| u64::try_from(value).unwrap_or_default();
                (TaskId(task), StoredReport { total: count(total), failed: count(failed), ..StoredReport::default() })
            })
            .collect();
        let tests: Vec<TestRow> = sqlx::query_as(
            "SELECT task_id, test_id, path, line, message FROM actions_test_failures \
             WHERE task_id IN (SELECT value FROM json_each(?)) ORDER BY task_id, ordinal",
        )
        .bind(&ids)
        .fetch_all(&self.pool)
        .await?;
        for (task, id, path, line, message) in tests {
            if let Some(report) = reports.get_mut(&TaskId(task)) {
                report.items.push(StoredTest { id, path, line: line.and_then(|line| u64::try_from(line).ok()), message });
            }
        }
        Ok(reports)
    }

    /// Those of `tests` that failed in a job of `repo` at commit `sha` while, at that commit,
    /// in another attempt or another run, a job of the same name passed or its report says
    /// the test passed (in a job that failed for another test): flaky there.
    pub async fn flaky_tests(&self, repo: RepoId, sha: &str, tests: &[String]) -> Result<BTreeSet<String>, StorageError> {
        if tests.is_empty() {
            return Ok(BTreeSet::new());
        }
        let wanted = serde_json::to_string(tests).unwrap_or_else(|_| "[]".to_owned());
        let rows: Vec<String> = sqlx::query_scalar(
            "SELECT DISTINCT f.test_id FROM actions_test_failures f \
             JOIN actions_tasks t ON t.id = f.task_id JOIN actions_jobs j ON j.id = t.job_id JOIN actions_runs r ON r.id = j.run_id \
             WHERE r.repo_id = ?1 AND r.sha = ?2 AND f.test_id IN (SELECT value FROM json_each(?3)) \
             AND (EXISTS (SELECT 1 FROM actions_tasks t2 JOIN actions_jobs j2 ON j2.id = t2.job_id \
                          JOIN actions_runs r2 ON r2.id = j2.run_id \
                          WHERE r2.repo_id = ?1 AND r2.sha = ?2 AND j2.name = j.name AND t2.status = 1) \
                  OR EXISTS (SELECT 1 FROM actions_test_passes p JOIN actions_tasks t3 ON t3.id = p.task_id \
                             JOIN actions_jobs j3 ON j3.id = t3.job_id JOIN actions_runs r3 ON r3.id = j3.run_id \
                             WHERE p.test_id = f.test_id AND p.task_id <> f.task_id \
                             AND r3.repo_id = ?1 AND r3.sha = ?2 AND j3.name = j.name))",
        )
        .bind(repo.0)
        .bind(sha)
        .bind(wanted)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().collect())
    }

    /// The newest finished run of workflow `path` on `reference` of `repo` before run
    /// `before`: what a failure is compared with.
    pub async fn previous_run(&self, repo: RepoId, path: &str, reference: &str, before: RunId) -> Result<Option<RunId>, StorageError> {
        let id: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM actions_runs WHERE repo_id = ? AND workflow_path = ? AND ref = ? AND id < ? AND status IN (1, 2, 3) \
             ORDER BY id DESC LIMIT 1",
        )
        .bind(repo.0)
        .bind(path)
        .bind(reference)
        .bind(before.0)
        .fetch_optional(&self.pool)
        .await?;
        Ok(id.map(RunId))
    }
}
