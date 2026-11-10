//! Serialized append/ack, crash recovery and durable progress commits.
use super::{
    Masker,
    types::{LogFiles, Progress},
};
use crate::{
    ActionsService, ServiceError,
    actions::runners::{CachedRunner, RunnerError},
};
use rbit_actions::{
    logs::{Append, accept_append, split_body},
    scheduling::leases,
};
use rbit_core::actions::{Job, LogState, Task, TaskId};
use rbit_workflow::protocol::{LogsResponse, MAX_LOG_LINE_BYTES};
use std::{
    path::PathBuf,
    sync::{Arc, PoisonError},
};

impl ActionsService {
    /// `<data_dir>/actions/logs`.
    pub(in crate::actions) fn logs_dir(&self) -> PathBuf {
        self.ctx().config.storage.actions_dir().join("logs")
    }

    /// The lock serialising appends to `task`'s log, and whether this process already
    /// checked its files against the database.
    pub(super) fn log_lock(&self, task: TaskId) -> Arc<tokio::sync::Mutex<bool>> {
        let mut locks = self.inner.log_locks.lock().unwrap_or_else(PoisonError::into_inner);
        Arc::clone(locks.entry(task).or_default())
    }

    /// `POST tasks/{id}/logs?from=<line>`: `body`'s lines are task-log lines `from…`. Lines
    /// the server already has are dropped, so a resend is harmless; a gap is a conflict
    /// that names the line to resend from. Answers the line count after the append.
    pub async fn append_log(&self, runner: &CachedRunner, id: TaskId, from: u64, body: &[u8]) -> Result<LogsResponse, RunnerError> {
        let text = String::from_utf8_lossy(body);
        let lines = split_body(&text).map_err(RunnerError::too_large)?;
        let lock = self.log_lock(id);
        let mut checked = lock.lock().await;
        let store = self.ctx().database.actions_runs();
        let task = store.task(id).await?.ok_or_else(|| RunnerError::not_found("there is no such task"))?;
        leases::owned(task.runner_id.map(|id| id.0), runner.id.0).map_err(|refusal| RunnerError::forbidden(refusal.message()))?;
        leases::log_open(task.status.is_done(), task.log_state == LogState::Live)
            .map_err(|refusal| RunnerError::conflict(refusal.message()))?;
        let current = u64::try_from(task.log_lines).unwrap_or_default();
        let skip = match accept_append(from, current, lines.len()) {
            Append::Gap { current } => return Err(RunnerError::log_gap(current)),
            Append::Nothing { current } => return Ok(LogsResponse { next_line: current }),
            Append::Store { skip } => skip,
        };
        let job = store.job(task.job_id).await?.ok_or_else(|| RunnerError::not_found("there is no such task"))?;
        let masker = self.masker(&task, &job).await?;
        let fresh: Vec<String> = lines[skip..].iter().map(|line| clean_line(&masker, line)).collect();
        // Actions: annotations. The fresh lines start at the log's current end.
        let annotations = super::super::annotations::found(&fresh, current);
        let progress = self.write_lines(&task, &job, fresh, &mut checked).await?;
        self.keep_annotations(task.id, annotations).await;
        Ok(LogsResponse { next_line: progress.lines })
    }

    /// Appends `lines` to `task`'s log with its lock held (`checked` is the lock's flag),
    /// and records the progress.
    async fn write_lines(&self, task: &Task, job: &Job, lines: Vec<String>, checked: &mut bool) -> Result<Progress, ServiceError> {
        let files = LogFiles::new(&self.logs_dir(), job.repo_id, task.id);
        let stored = Progress {
            lines: u64::try_from(task.log_lines).unwrap_or_default(),
            bytes: u64::try_from(task.log_bytes).unwrap_or_default(),
            truncated: task.log_truncated,
        };
        let (recover, max_bytes) = (!*checked, self.config().max_log_bytes);
        let progress = tokio::task::spawn_blocking(move || append_lines(&files, stored, &lines, max_bytes, recover))
            .await
            .map_err(ServiceError::internal)?
            .map_err(ServiceError::internal)?;
        *checked = true;
        // Actions: metrics.
        metrics::counter!("rbit_actions_log_bytes_total").increment(progress.bytes.saturating_sub(stored.bytes));
        let lines = i64::try_from(progress.lines).unwrap_or(i64::MAX);
        let bytes = i64::try_from(progress.bytes).unwrap_or(i64::MAX);
        self.ctx().database.actions_runs().set_log_progress(task.id, lines, bytes, progress.truncated).await?;
        Ok(progress)
    }

    /// Writes one line from rbit itself (why a task failed before it ran) into `task`'s log.
    pub(in crate::actions) async fn write_system_line(&self, task: TaskId, line: &str) -> Result<(), ServiceError> {
        let lock = self.log_lock(task);
        let mut checked = lock.lock().await;
        let store = self.ctx().database.actions_runs();
        let Some(task) = store.task(task).await? else { return Ok(()) };
        let Some(job) = store.job(task.job_id).await? else { return Ok(()) };
        if task.log_state != LogState::Live {
            return Ok(());
        }
        self.write_lines(&task, &job, vec![clean_line(&Masker::none(), &format!("rbit: {line}"))], &mut checked).await?;
        Ok(())
    }
}

/// A line as stored: masked, without a trailing `\r`, at most `MAX_LOG_LINE_BYTES`.
pub(super) fn clean_line(masker: &Masker, line: &str) -> String {
    rbit_actions::logs::clean_line(masker, line, MAX_LOG_LINE_BYTES)
}

/// Appends `lines` after `stored`, within `max_bytes`; with `recover`, first cuts the files
/// back to what the database recorded (a crash between writing them and the row). The
/// write and sync ordering is `rbit_actions::logs::append_lines`.
pub(super) fn append_lines(
    files: &LogFiles,
    progress: Progress,
    lines: &[String],
    max_bytes: u64,
    recover: bool,
) -> std::io::Result<Progress> {
    rbit_actions::logs::append_lines(files, progress, lines, max_bytes, recover)
}
