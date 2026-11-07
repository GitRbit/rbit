//! Instrument the actual read statements shared by legacy and batched log pages.
//! SQL, row mapping and the underlying pool are unchanged; writes stay in their
//! original transaction path. This is deliberately not a store-wide migration.
use super::{ActionsRunStore, JobRow, RunRow, StepRow, TaskRow};
use crate::{StorageError, sqlite::pool::Pool};
use rbit_core::{
    RepoId,
    actions::{Job, JobId, Run, RunId, Step, Task, TaskId},
};

impl ActionsRunStore {
    fn read_pool(&self) -> Pool {
        Pool::new(self.pool.clone(), "actions_runs")
    }

    /// Run `id`, only if it belongs to `repo`.
    pub async fn find_run(&self, repo: RepoId, id: RunId) -> Result<Option<Run>, StorageError> {
        let pool = self.read_pool();
        let row: Option<RunRow> =
            sqlx::query_as(select_run!("WHERE id = ? AND repo_id = ?")).bind(id.0).bind(repo.0).fetch_optional(&pool).await?;
        row.map(Run::try_from).transpose()
    }

    /// The steps of `tasks`, by task and number.
    pub async fn steps_of_tasks(&self, tasks: &[TaskId]) -> Result<Vec<Step>, StorageError> {
        if tasks.is_empty() {
            return Ok(Vec::new());
        }
        let ids = format!("[{}]", tasks.iter().map(|task| task.0.to_string()).collect::<Vec<_>>().join(","));
        let pool = self.read_pool();
        let rows: Vec<StepRow> = sqlx::query_as(
            "SELECT task_id, number, name, status, started_at, stopped_at, log_index, log_length FROM actions_steps \
             WHERE task_id IN (SELECT value FROM json_each(?)) ORDER BY task_id, number",
        )
        .bind(ids)
        .fetch_all(&pool)
        .await?;
        rows.into_iter().map(Step::try_from).collect()
    }

    /// Job `id`, only if it belongs to `repo`.
    pub async fn find_job(&self, repo: RepoId, id: JobId) -> Result<Option<Job>, StorageError> {
        let pool = self.read_pool();
        let row: Option<JobRow> =
            sqlx::query_as(select_job!("WHERE j.id = ? AND j.repo_id = ?")).bind(id.0).bind(repo.0).fetch_optional(&pool).await?;
        row.map(Job::try_from).transpose()
    }

    pub async fn task(&self, id: TaskId) -> Result<Option<Task>, StorageError> {
        let pool = self.read_pool();
        let row: Option<TaskRow> = sqlx::query_as(select_task!("WHERE id = ?")).bind(id.0).fetch_optional(&pool).await?;
        row.map(Task::try_from).transpose()
    }

    /// The steps of a task, by number.
    pub async fn steps(&self, task: TaskId) -> Result<Vec<Step>, StorageError> {
        let pool = self.read_pool();
        let rows: Vec<StepRow> = sqlx::query_as(
            "SELECT task_id, number, name, status, started_at, stopped_at, log_index, log_length FROM actions_steps \
             WHERE task_id = ? ORDER BY number",
        )
        .bind(task.0)
        .fetch_all(&pool)
        .await?;
        rows.into_iter().map(Step::try_from).collect()
    }
}
