//! Authorized job-page metadata and log windows, pinned to one current task.
use super::{
    LogLimits, LogRange, LogWindow,
    batch_reader::{LogSnapshot, read_windows},
    plan::WindowRequest,
    types::LogFiles,
    work::ReadWork,
};
use crate::{ActionsService, RepoContext, ServiceError, actions::browse::JobDetail};
use rbit_core::{
    Account, AccountId,
    actions::{JobId, LogState, Status, Step, Task},
};
use std::collections::BTreeMap;

/// A job-page read. Logs follow exactly the same step order as `detail.steps`.
#[derive(Debug)]
pub struct JobLogDetail {
    pub detail: JobDetail,
    pub people: BTreeMap<AccountId, Account>,
    /// `None` until a step starts; retained/expired windows preserve their flags.
    pub logs: Vec<Option<LogWindow>>,
}

impl ActionsService {
    /// Resolve a readable enabled repository's job/run/current task once, then read
    /// every started step through one index snapshot and one data descriptor.
    /// No route, response markup, polling interval or per-step budget is changed.
    pub async fn job_log_detail(
        &self,
        repo: &RepoContext,
        number: i64,
        job: JobId,
        limits: LogLimits,
    ) -> Result<JobLogDetail, ServiceError> {
        let detail = self.job_detail(repo, job).await?;
        if detail.run.number != number {
            return Err(ServiceError::NotFound);
        }
        let people = self.people([detail.run.actor, detail.run.trigger_actor, detail.run.approved_by]).await?;
        let requests: Vec<_> = detail.steps.iter().filter(|step| step.started_at.is_some()).map(|step| request(step, limits)).collect();
        let windows = if let Some(id) = detail.job.task_id {
            let task = self.ctx().database.actions_runs().task(id).await?.ok_or(ServiceError::NotFound)?;
            if task.job_id != detail.job.id {
                return Err(ServiceError::NotFound);
            }
            let files = LogFiles::new(&self.logs_dir(), detail.job.repo_id, id);
            match read(files.clone(), snapshot(&task), requests.clone()).await? {
                Ok(windows) => windows,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    // A finalizer/retention may have removed the selected representation.
                    // Retry once only if authorized persisted log state actually changed.
                    self.require_pages(repo).await?;
                    let next = self.ctx().database.actions_runs().task(id).await?.ok_or(ServiceError::NotFound)?;
                    if next.job_id != detail.job.id || !retryable(task.log_state, next.log_state) {
                        return Err(ServiceError::internal(error));
                    }
                    read(files, snapshot(&next), requests).await?.map_err(ServiceError::internal)?
                }
                Err(error) => return Err(ServiceError::internal(error)),
            }
        } else {
            Vec::new()
        };
        let mut windows = windows.into_iter();
        let logs = detail.steps.iter().map(|step| if step.started_at.is_some() { windows.next() } else { None }).collect();
        Ok(JobLogDetail { detail, people, logs })
    }
}

fn request(step: &Step, limits: LogLimits) -> WindowRequest {
    let start = u64::try_from(step.log_index).unwrap_or_default();
    let end = (step.status != Status::Running).then(|| start.saturating_add(u64::try_from(step.log_length).unwrap_or_default()));
    WindowRequest { start, end, range: LogRange::Tail, limits }
}

fn snapshot(task: &Task) -> LogSnapshot {
    LogSnapshot {
        lines: u64::try_from(task.log_lines).unwrap_or_default(),
        bytes: u64::try_from(task.log_bytes).unwrap_or_default(),
        state: task.log_state,
        live: task.log_state == LogState::Live && !task.status.is_done(),
        truncated: task.log_truncated,
    }
}

// Only owned paths, immutable request metadata and bounded buffers enter this closure.
// Dropping the awaiter does not claim Tokio's blocking work has stopped.
async fn read(
    files: LogFiles,
    snapshot: LogSnapshot,
    requests: Vec<WindowRequest>,
) -> Result<std::io::Result<Vec<LogWindow>>, ServiceError> {
    tokio::task::spawn_blocking(move || {
        let mut work = ReadWork::default();
        let result = read_windows(&files, snapshot, &requests, &mut work);
        work.record();
        result
    })
    .await
    .map_err(ServiceError::internal)
}

/// Only forward on-disk lifecycle transitions can explain a vanished representation.
pub(super) fn retryable(previous: LogState, next: LogState) -> bool {
    matches!((previous, next), (LogState::Live, LogState::Final | LogState::Expired) | (LogState::Final, LogState::Expired))
}
