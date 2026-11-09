//! Whether a runner still holds a task's lease, for calls that act on a running task
//! (artifact uploads, the shared cache). Uploads check it before and again after their
//! body, so a task the sweeper or a rerun took away stores nothing.
use std::future::Future;

/// The stored task and job records, read when asked.
pub trait TaskLeases: Sync {
    type Task: Send + Sync;
    type Job: Send + Sync;
    type Error: Send;

    fn task(&self, id: i64) -> impl Future<Output = Result<Option<Self::Task>, Self::Error>> + Send;
    fn job_of(&self, task: &Self::Task) -> impl Future<Output = Result<Option<Self::Job>, Self::Error>> + Send;
    /// The runner the task was handed to.
    fn runner_of(&self, task: &Self::Task) -> Option<i64>;
    fn is_done(&self, task: &Self::Task) -> bool;
    fn task_id(&self, task: &Self::Task) -> i64;
    /// The task of the job's current attempt.
    fn current_task(&self, job: &Self::Job) -> Option<i64>;
}

/// Why a runner may not act on a task, with the message the runner protocol answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// Not found: there is no such task.
    NoTask,
    /// Forbidden: another runner holds it.
    OtherRunner,
    /// Conflict: it has ended.
    Ended,
    /// Not found: its job is gone.
    NoJob,
    /// Conflict: its job was rerun, so a newer task holds the lease.
    Rerun,
    /// Conflict: it has ended or its log was closed, so nothing more is appended.
    LogClosed,
}

impl Refusal {
    pub const fn message(self) -> &'static str {
        match self {
            Self::NoTask => "there is no such task",
            Self::OtherRunner => "this task belongs to another runner",
            Self::Ended => "the task has ended",
            Self::NoJob => "there is no such job",
            Self::Rerun => "the job has been rerun; this task no longer belongs to it",
            Self::LogClosed => "the task has ended; its log is closed",
        }
    }
}

/// Whether `runner` was handed the task (`owner`).
pub fn owned(owner: Option<i64>, runner: i64) -> Result<(), Refusal> {
    if owner == Some(runner) { Ok(()) } else { Err(Refusal::OtherRunner) }
}

/// Whether lines may still be appended to a task's log: it runs and its log is live.
pub const fn log_open(done: bool, log_live: bool) -> Result<(), Refusal> {
    if done || !log_live { Err(Refusal::LogClosed) } else { Ok(()) }
}

/// The answer to a state report about a task that already ended: cancel when the report
/// carries no result (its runner still runs a task the server stopped: the sweeper failed
/// it or its runner was removed, so the lease is gone), or when the job or the task was
/// cancelled. A resent final report is answered as before.
pub const fn ended_task_cancels(has_result: bool, job_cancelled: bool, task_cancelled: bool) -> bool {
    !has_result || job_cancelled || task_cancelled
}

/// Task `id` of `runner`, running: its own and not ended.
pub async fn running<P: TaskLeases>(port: &P, runner: i64, id: i64) -> Result<Result<P::Task, Refusal>, P::Error> {
    let Some(task) = port.task(id).await? else { return Ok(Err(Refusal::NoTask)) };
    if let Err(refusal) = owned(port.runner_of(&task), runner) {
        return Ok(Err(refusal));
    }
    if port.is_done(&task) {
        return Ok(Err(Refusal::Ended));
    }
    Ok(Ok(task))
}

/// Task `id` of `runner` while it holds its job's lease: [`running`], and the job's
/// current attempt (a task left behind by a rerun would otherwise file under the new one).
pub async fn leased<P: TaskLeases>(port: &P, runner: i64, id: i64) -> Result<Result<(P::Task, P::Job), Refusal>, P::Error> {
    let task = match running(port, runner, id).await? {
        Ok(task) => task,
        Err(refusal) => return Ok(Err(refusal)),
    };
    let Some(job) = port.job_of(&task).await? else { return Ok(Err(Refusal::NoJob)) };
    if port.current_task(&job) != Some(port.task_id(&task)) {
        return Ok(Err(Refusal::Rerun));
    }
    Ok(Ok((task, job)))
}
