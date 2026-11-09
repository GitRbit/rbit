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
}

impl Refusal {
    pub const fn message(self) -> &'static str {
        match self {
            Self::NoTask => "there is no such task",
            Self::OtherRunner => "this task belongs to another runner",
            Self::Ended => "the task has ended",
            Self::NoJob => "there is no such job",
            Self::Rerun => "the job has been rerun; this task no longer belongs to it",
        }
    }
}

/// Task `id` of `runner`, running: its own and not ended.
pub async fn running<P: TaskLeases>(port: &P, runner: i64, id: i64) -> Result<Result<P::Task, Refusal>, P::Error> {
    let Some(task) = port.task(id).await? else { return Ok(Err(Refusal::NoTask)) };
    if port.runner_of(&task) != Some(runner) {
        return Ok(Err(Refusal::OtherRunner));
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
