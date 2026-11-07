//! One borrowed claim/replay adapter and its original reusable concrete store.
use super::{ActionsService, CachedRunner, RunnerError, step_name};
use rbit_actions::claims::{ClaimQueue, Claimed, LeaseReplay, Specification};
use rbit_auth::TaskTokenSecret;
use rbit_core::{
    Timestamp,
    actions::{Job, Status, Task},
};
use rbit_storage::sqlite::actions_runs::{ActionsRunStore, ClaimRequest};
use rbit_workflow::spec::JobSpec;
use std::{fmt, sync::atomic::Ordering};

pub(super) struct ClaimSession<'a> {
    service: &'a ActionsService,
    runner: &'a CachedRunner,
    store: ActionsRunStore,
}
impl fmt::Debug for ClaimSession<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClaimSession").field("runner", &self.runner.id).finish_non_exhaustive()
    }
}
impl<'a> ClaimSession<'a> {
    pub(super) fn new(service: &'a ActionsService, runner: &'a CachedRunner) -> Self {
        Self { service, runner, store: service.ctx().database.actions_runs() }
    }
}
impl Specification for ClaimSession<'_> {
    type Error = RunnerError;
    type Job = Job;
    type Task = Task;
    type Token = TaskTokenSecret;
    type Spec = JobSpec;
    fn new_token(&self) -> TaskTokenSecret {
        TaskTokenSecret::generate()
    }
    async fn specification(&self, job: &Job, task: &Task, token: &TaskTokenSecret) -> Result<Result<JobSpec, String>, RunnerError> {
        Ok(self.service.job_spec(job, task, token).await?)
    }
    fn remember_masks(&self, spec: &JobSpec) {
        self.service.remember_masks(spec);
    }
    async fn fail_unrunnable(&self, task: &Task, reason: &str) -> Result<(), RunnerError> {
        Ok(self.service.fail_unrunnable(task, reason).await?)
    }
}
impl ClaimQueue for ClaimSession<'_> {
    async fn claim(&self, token: &TaskTokenSecret, key: Option<&str>) -> Result<Option<Claimed<Job, Task>>, RunnerError> {
        let runner = self.runner;
        let request = ClaimRequest {
            runner: runner.id,
            scope: runner.scope,
            labels: &runner.labels,
            host_labels: &runner.host_labels,
            ephemeral: runner.ephemeral,
            instance_runners_for_all: self.service.config().instance_runners_for_all,
            token_digest: &token.digest(),
            idempotency_key: key,
            now: Timestamp::now(),
        };
        Ok(self.store.claim(&request).await?.map(|claimed| Claimed { job: claimed.job, task: claimed.task }))
    }
    fn remember_key(&self, key: &str, task: &Task) {
        self.runner.remember(key, task.id);
    }
    fn claimed(&self, claim: &Claimed<Job, Task>) {
        tracing::info!(task = claim.task.id.0, job = claim.job.id.0, runner = self.runner.id.0, "job claimed");
    }
    async fn create_steps(&self, task: &Task, spec: &JobSpec) -> Result<(), RunnerError> {
        let names: Vec<String> = spec.template.steps.iter().enumerate().map(|(index, step)| step_name(index, step)).collect();
        Ok(self.store.create_steps(task.id, &names).await?)
    }
    async fn report_statuses(&self, job: &Job) -> Result<(), RunnerError> {
        Ok(self.service.report_statuses(job.run_id).await?)
    }
}
impl LeaseReplay for ClaimSession<'_> {
    async fn lookup(&self, key: &str) -> Result<Option<Task>, RunnerError> {
        Ok(match self.runner.remembered(key) {
            Some(task) => self.store.task(task).await?,
            None if !self.runner.keys_loaded.swap(true, Ordering::SeqCst) => {
                self.store.task_by_idempotency_key(self.runner.id, key).await?
            }
            None => None,
        })
    }
    fn belongs_to_runner(&self, task: &Task) -> bool {
        task.runner_id == Some(self.runner.id)
    }
    fn is_running(&self, task: &Task) -> bool {
        task.status == Status::Running
    }
    async fn replace_token(&self, task: &Task, token: &TaskTokenSecret) -> Result<bool, RunnerError> {
        Ok(self.store.replace_task_token(task.id, &token.digest()).await?)
    }
    async fn job(&self, task: &Task) -> Result<Option<Job>, RunnerError> {
        Ok(self.store.job(task.job_id).await?)
    }
    fn resumed(&self, task: &Task) {
        tracing::info!(task = task.id.0, runner = self.runner.id.0, "poll retried: the same task with a new token");
    }
}
