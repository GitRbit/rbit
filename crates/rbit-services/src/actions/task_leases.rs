//! The storage adapter of `rbit_actions::scheduling::leases`: tasks and jobs as stored,
//! and the runner protocol's answers to a refused lease.
use rbit_actions::scheduling::leases::{self, Refusal, TaskLeases};
use rbit_core::actions::{Job, Task, TaskId};

use super::{
    ActionsService,
    runners::{CachedRunner, RunnerError},
};
use crate::ServiceError;

/// The task store, for the lease checks.
struct StoredLeases<'a>(&'a ActionsService);

impl TaskLeases for StoredLeases<'_> {
    type Task = Task;
    type Job = Job;
    type Error = ServiceError;

    async fn task(&self, id: i64) -> Result<Option<Task>, ServiceError> {
        Ok(self.0.ctx().database.actions_runs().task(TaskId(id)).await?)
    }

    async fn job_of(&self, task: &Task) -> Result<Option<Job>, ServiceError> {
        Ok(self.0.ctx().database.actions_runs().job(task.job_id).await?)
    }

    fn runner_of(&self, task: &Task) -> Option<i64> {
        task.runner_id.map(|runner| runner.0)
    }

    fn is_done(&self, task: &Task) -> bool {
        task.status.is_done()
    }

    fn task_id(&self, task: &Task) -> i64 {
        task.id.0
    }

    fn current_task(&self, job: &Job) -> Option<i64> {
        job.task_id.map(|task| task.0)
    }
}

/// A refused lease as the runner protocol answers it.
fn refused(refusal: Refusal) -> RunnerError {
    match refusal {
        Refusal::NoTask | Refusal::NoJob => RunnerError::not_found(refusal.message()),
        Refusal::OtherRunner => RunnerError::forbidden(refusal.message()),
        Refusal::Ended | Refusal::Rerun | Refusal::LogClosed => RunnerError::conflict(refusal.message()),
    }
}

impl ActionsService {
    /// Task `id` of `runner`, running (`rbit_actions::scheduling::leases::running`).
    pub(super) async fn running_task(&self, runner: &CachedRunner, id: TaskId) -> Result<Task, RunnerError> {
        leases::running(&StoredLeases(self), runner.id.0, id.0).await?.map_err(refused)
    }

    /// Task `id` of `runner` while it holds its job's lease: running, and the job's current
    /// attempt (`rbit_actions::scheduling::leases::leased`). Uploads check it before and
    /// again after their body.
    pub(super) async fn leased_task(&self, runner: &CachedRunner, id: TaskId) -> Result<(Task, Job), RunnerError> {
        leases::leased(&StoredLeases(self), runner.id.0, id.0).await?.map_err(refused)
    }
}
