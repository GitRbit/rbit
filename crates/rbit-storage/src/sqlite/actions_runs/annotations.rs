//! Annotations of task logs (migration `0074_actions_annotations.sql`): kept as the log is
//! appended, read for the run page.
use super::ActionsRunStore;
use crate::StorageError;
use rbit_core::actions::TaskId;

/// One kept annotation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredAnnotation {
    pub task: TaskId,
    /// The task-log line, from 0.
    pub line: i64,
    /// `error`, `warning` or `notice`.
    pub level: String,
    pub text: String,
}

impl ActionsRunStore {
    /// Keeps `annotations` (task-log line, level, text) of `task` while it has fewer than
    /// `max`, in order; a line already kept stays as it was.
    pub async fn add_annotations(&self, task: TaskId, annotations: &[(i64, &str, &str)], max: i64) -> Result<(), StorageError> {
        if annotations.is_empty() {
            return Ok(());
        }
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        for (line, level, text) in annotations {
            sqlx::query(
                "INSERT OR IGNORE INTO actions_annotations (task_id, line, level, text) \
                 SELECT ?1, ?2, ?3, ?4 WHERE (SELECT COUNT(*) FROM actions_annotations WHERE task_id = ?1) < ?5",
            )
            .bind(task.0)
            .bind(line)
            .bind(level)
            .bind(text)
            .bind(max)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// The annotations of `tasks`, by task and line.
    pub async fn annotations_of_tasks(&self, tasks: &[TaskId]) -> Result<Vec<StoredAnnotation>, StorageError> {
        if tasks.is_empty() {
            return Ok(Vec::new());
        }
        let ids = format!("[{}]", tasks.iter().map(|task| task.0.to_string()).collect::<Vec<_>>().join(","));
        let rows: Vec<(i64, i64, String, String)> = sqlx::query_as(
            "SELECT task_id, line, level, text FROM actions_annotations \
             WHERE task_id IN (SELECT value FROM json_each(?)) ORDER BY task_id, line",
        )
        .bind(ids)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|(task, line, level, text)| StoredAnnotation { task: TaskId(task), line, level, text }).collect())
    }
}
