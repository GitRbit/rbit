//! Authorized job-page metadata and log windows, pinned to one current task.
use super::{
    LogLimits, LogRange, LogWindow,
    batch_reader::{LogSnapshot, read_windows, representation},
    has_log,
    plan::WindowRequest,
    types::LogFiles,
    work::{ReadWork, record},
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
    /// `None` until a step starts (or says why it was skipped); retained/expired windows
    /// preserve their flags.
    pub logs: Vec<Option<LogWindow>>,
    /// The lines before the first step and after the last ([`super::sections`]), once a
    /// runner took the job.
    pub setup: Option<LogWindow>,
    pub complete: Option<LogWindow>,
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
        let mut requests: Vec<_> = detail.steps.iter().filter(|step| has_log(step)).map(|step| request(step, limits)).collect();
        let sections = super::sections(&detail.steps, detail.job.status.is_done());
        let extra = |start: u64, end: Option<u64>| WindowRequest { start, end, range: LogRange::Tail, limits };
        let setup = sections.setup.map(|(start, end)| extra(start, end));
        let complete = sections.complete.map(|start| extra(start, None));
        if detail.job.task_id.is_some() {
            requests.extend(setup.iter().chain(complete.iter()).copied());
        }
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
        let logs = detail.steps.iter().map(|step| if has_log(step) { windows.next() } else { None }).collect();
        let setup = setup.and_then(|_| windows.next());
        let complete = complete.and_then(|_| windows.next());
        Ok(JobLogDetail { detail, people, logs, setup, complete })
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
        record(&work);
        result
    })
    .await
    .map_err(ServiceError::internal)
}

/// Only forward on-disk lifecycle transitions can explain a vanished representation
/// (`rbit_actions::logs::retryable`).
pub(super) fn retryable(previous: LogState, next: LogState) -> bool {
    rbit_actions::logs::retryable(representation(previous), representation(next))
}
