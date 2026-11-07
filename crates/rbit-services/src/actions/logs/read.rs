//! Authorized single-task/step queries and bounded blocking window reads.
use super::{LogRange, LogWindow, RawLog, reader::read_window, types::LogFiles};
use crate::{ActionsService, RepoContext, ServiceError};
use rbit_core::actions::{Job, LogState, Status, Step, Task, TaskId};

impl ActionsService {
    /// Lines of task `task`'s log for a reader of `repo`: from a line on, or the tail,
    /// within `max_lines` and `max_bytes` (at least one line).
    pub async fn task_log(
        &self,
        repo: &RepoContext,
        task: TaskId,
        range: LogRange,
        max_lines: usize,
        max_bytes: usize,
    ) -> Result<LogWindow, ServiceError> {
        let (task, job) = self.readable_task(repo, task).await?;
        self.window(&task, &job, 0, None, range, max_lines, max_bytes).await
    }

    /// Lines of step `step` (its number, from 0) of task `task` for a reader of `repo`. A
    /// running step's window reaches to the end of the log so far; others end where the
    /// runner said. [`super::STEP_VIEW_LINES`] and [`super::STEP_VIEW_BYTES`] are what a page shows.
    pub async fn step_log(
        &self,
        repo: &RepoContext,
        task: TaskId,
        step: i64,
        range: LogRange,
        max_lines: usize,
        max_bytes: usize,
    ) -> Result<LogWindow, ServiceError> {
        let (task, job) = self.readable_task(repo, task).await?;
        let steps = self.ctx().database.actions_runs().steps(task.id).await?;
        let step = steps.into_iter().find(|candidate| candidate.number == step).ok_or(ServiceError::NotFound)?;
        let start = u64::try_from(step.log_index).unwrap_or_default();
        let end = if step.status == Status::Running { None } else { Some(start + u64::try_from(step.log_length).unwrap_or_default()) };
        self.window(&task, &job, start, end, range, max_lines, max_bytes).await
    }

    /// The steps of task `task` for a reader of `repo`.
    pub async fn task_steps(&self, repo: &RepoContext, task: TaskId) -> Result<(Task, Vec<Step>), ServiceError> {
        let (task, _) = self.readable_task(repo, task).await?;
        let steps = self.ctx().database.actions_runs().steps(task.id).await?;
        Ok((task, steps))
    }

    /// Task `task`'s whole log for a reader of `repo`; `None` when nothing was written or
    /// the log expired.
    pub async fn raw_log(&self, repo: &RepoContext, task: TaskId) -> Result<Option<RawLog>, ServiceError> {
        let (task, job) = self.readable_task(repo, task).await?;
        let files = LogFiles::new(&self.logs_dir(), job.repo_id, task.id);
        let (path, gzip) = match task.log_state {
            LogState::Expired => return Ok(None),
            LogState::Live => (files.log, false),
            LogState::Final => (files.gzip, true),
        };
        match tokio::fs::File::open(&path).await {
            Ok(file) => Ok(Some(RawLog { file, gzip })),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(ServiceError::internal(error)),
        }
    }

    // Actions: run logs.
    /// The files of run `run`'s log archive, as GitHub names them (`<n>_<job name>.txt`, in
    /// job order), with the task whose log each one is; jobs that never ran have none.
    pub async fn run_log_files(&self, repo: &RepoContext, run: rbit_core::actions::RunId) -> Result<Vec<(String, TaskId)>, ServiceError> {
        let detail = self.run_by_id(repo, run).await?;
        Ok(detail
            .jobs
            .iter()
            .enumerate()
            .filter_map(|(index, job)| Some((format!("{index}_{}.txt", super::archive::log_file_name(&job.name)), job.task_id?)))
            .collect())
    }

    /// Task `task` and its job, when the job is `repo`'s and the caller may read it.
    pub(super) async fn readable_task(&self, repo: &RepoContext, task: TaskId) -> Result<(Task, Job), ServiceError> {
        if !repo.access.can_read() {
            return Err(ServiceError::NotFound);
        }
        let store = self.ctx().database.actions_runs();
        let task = store.task(task).await?.ok_or(ServiceError::NotFound)?;
        let job = store.find_job(repo.repo.repo.id, task.job_id).await?.ok_or(ServiceError::NotFound)?;
        Ok((task, job))
    }

    /// Lines `start..end` (`None`: to the end) of the log, windowed by `range` and limits.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn window(
        &self,
        task: &Task,
        job: &Job,
        start: u64,
        end: Option<u64>,
        range: LogRange,
        max_lines: usize,
        max_bytes: usize,
    ) -> Result<LogWindow, ServiceError> {
        let mut window = LogWindow {
            first_line: start,
            next_line: start,
            live: task.log_state == LogState::Live && !task.status.is_done(),
            truncated: task.log_truncated,
            expired: task.log_state == LogState::Expired,
            ..LogWindow::default()
        };
        if window.expired {
            return Ok(window);
        }
        let files = LogFiles::new(&self.logs_dir(), job.repo_id, task.id);
        let gzip = task.log_state == LogState::Final;
        tokio::task::spawn_blocking(move || {
            read_window(&files, gzip, start, end, range, max_lines.max(1), max_bytes, &mut window).map(|()| window)
        })
        .await
        .map_err(ServiceError::internal)?
        .map_err(ServiceError::internal)
    }
}
