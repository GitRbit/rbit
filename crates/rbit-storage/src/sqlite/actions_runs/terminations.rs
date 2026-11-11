//! Termination records of tasks (migration `0075_actions_terminations.sql`): how each task
//! ended, kept once, read with the jobs that show it.
use std::collections::BTreeMap;

use super::ActionsRunStore;
use crate::StorageError;
use rbit_core::{
    Timestamp,
    actions::{TaskId, Termination, TerminationReason},
};

impl ActionsRunStore {
    /// Records how `task` ended, unless a record exists: the first word on a task stands.
    /// Whether this call recorded it.
    pub async fn record_termination(&self, task: TaskId, termination: Termination, now: Timestamp) -> Result<bool, StorageError> {
        let done = sqlx::query(
            "INSERT OR IGNORE INTO actions_terminations (task_id, reason, step, exit_code, recorded_at) \
             SELECT ?1, ?2, ?3, ?4, ?5 WHERE EXISTS (SELECT 1 FROM actions_tasks WHERE id = ?1)",
        )
        .bind(task.0)
        .bind(termination.reason.as_str())
        .bind(termination.step.map(i64::from))
        .bind(termination.exit_code)
        .bind(now.0)
        .execute(&self.pool)
        .await?;
        Ok(done.rows_affected() == 1)
    }

    /// How each of `tasks` ended: its termination record when it has one, and whether its
    /// log is complete (finalised and not cut short).
    pub async fn task_ends(&self, tasks: &[TaskId]) -> Result<BTreeMap<TaskId, TaskEnd>, StorageError> {
        if tasks.is_empty() {
            return Ok(BTreeMap::new());
        }
        let ids = format!("[{}]", tasks.iter().map(|task| task.0.to_string()).collect::<Vec<_>>().join(","));
        let rows: Vec<(i64, Option<String>, Option<i64>, Option<i32>, String, bool)> = sqlx::query_as(
            "SELECT t.id, x.reason, x.step, x.exit_code, t.log_state, t.log_truncated FROM actions_tasks t              LEFT JOIN actions_terminations x ON x.task_id = t.id WHERE t.id IN (SELECT value FROM json_each(?))",
        )
        .bind(ids)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(task, reason, step, exit_code, log_state, truncated)| {
                let termination = reason.map(|reason| Termination {
                    reason: TerminationReason::parse(&reason),
                    step: step.and_then(|step| u32::try_from(step).ok()),
                    exit_code,
                });
                (TaskId(task), TaskEnd { termination, log_complete: log_state == "final" && !truncated })
            })
            .collect())
    }
}

/// How a task ended, as [`ActionsRunStore::task_ends`] reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskEnd {
    pub termination: Option<Termination>,
    /// The log was finalised and nothing was cut from it.
    pub log_complete: bool,
}
