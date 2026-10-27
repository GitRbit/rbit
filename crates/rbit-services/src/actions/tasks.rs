//! `POST tasks/{id}/state`: what a runner reports about a task it runs — the heartbeat,
//! step states, job outputs and, at the end, the result — and the answer that carries
//! cancellation back (§12.6 "Lifecycle").
//!
//! * Step summaries (`GITHUB_STEP_SUMMARY`) are kept when first seen per step, masked
//!   again with the secrets the server knows, for the run page.
//! * Every call checks that the task is the calling runner's; another runner's task is
//!   `forbidden`, an unknown one `not_found` (the runner then cancels it locally).
//! * Each call is a heartbeat. Steps are keyed by index and only the steps the task was
//!   created with are updated; outputs are kept when first seen (a resent call changes
//!   nothing), masked ([`super::logs::Masker`]) and capped at 1 MiB per job (§11.10).
//! * The final call stores the result: the task stops and its token dies with it, its log
//!   is finalised, its job and the jobs that need it settle through
//!   [`ActionsService::settled`] (statuses, the next held run), and an ephemeral runner is
//!   deleted.
//! * The answer is `{cancel: true}` from the moment the job is cancelled — by then the
//!   token is already dead (§11.11). Calls about a task that already stopped change
//!   nothing and are answered the same way, and so are calls about a task whose job a
//!   rerun gave to a newer task: its outputs are dropped and its result leaves the job
//!   alone.
//!
//! The server side of the runner protocol (stream S) owns this file.

use rbit_core::{
    Timestamp,
    actions::{Status, TaskId},
};
use rbit_storage::sqlite::actions_runs::StepUpdate;
use rbit_workflow::protocol::{MAX_OUTPUT_BYTES, StepStatus, TaskResult, TaskStateRequest, TaskStateResponse};

use super::{
    ActionsService,
    runners::{CachedRunner, RunnerError},
};

impl ActionsService {
    /// `POST tasks/{id}/state`. See the module documentation.
    pub async fn task_state(
        &self,
        runner: &CachedRunner,
        id: TaskId,
        request: &TaskStateRequest,
    ) -> Result<TaskStateResponse, RunnerError> {
        request.validate().map_err(|error| RunnerError::bad_request(error.message))?;
        let store = self.ctx().database.actions_runs();
        let task = store.task(id).await?.ok_or_else(|| RunnerError::not_found("there is no such task"))?;
        if task.runner_id != Some(runner.id) {
            return Err(RunnerError::forbidden("this task belongs to another runner"));
        }
        let job = store.job(task.job_id).await?.ok_or_else(|| RunnerError::not_found("there is no such task"))?;
        if task.status.is_done() {
            return Ok(TaskStateResponse { cancel: job.status == Status::Cancelled || task.status == Status::Cancelled });
        }
        let now = Timestamp::now();
        store.heartbeat(id, now).await?;
        // Actions: third-party actions. Steps the runner added (`post` scripts) are created
        // once; a template step's number keeps its name.
        if !request.post_steps.is_empty() {
            let added: Vec<(i64, &str)> = request.post_steps.iter().map(|(index, name)| (i64::from(*index), name.as_str())).collect();
            store.add_steps(id, &added).await?;
        }
        if !request.steps.is_empty() {
            let steps: Vec<StepUpdate<'_>> = request
                .steps
                .iter()
                .map(|step| StepUpdate {
                    number: i64::from(step.index),
                    name: "",
                    status: step_status(step.status),
                    started_at: step.started_at.map(Timestamp),
                    stopped_at: step.stopped_at.map(Timestamp),
                    log_index: i64::try_from(step.log_index).unwrap_or(i64::MAX),
                    log_length: i64::try_from(step.log_length).unwrap_or(i64::MAX),
                })
                .collect();
            store.update_steps(id, &steps).await?;
        }
        // Actions: step summaries.
        if !request.summaries.is_empty() {
            let masker = self.masker(&task, &job).await?;
            let cleaned: Vec<(i64, String)> =
                request.summaries.iter().map(|(index, summary)| (i64::from(*index), masker.mask(summary))).collect();
            let summaries: Vec<(i64, &str)> = cleaned.iter().map(|(index, summary)| (*index, summary.as_str())).collect();
            store.add_summaries(id, &summaries).await?;
        }
        if job.task_id != Some(id) {
            // A rerun gave the job a new attempt while this one was still running: its
            // outputs and result no longer belong to the job, and it should stop.
            if let Some(result) = request.result {
                self.finish(runner, id, result_status(result)).await?;
            }
            return Ok(TaskStateResponse { cancel: true });
        }
        // Actions: environments.
        if let Some(url) = &request.environment_url {
            let masker = self.masker(&task, &job).await?;
            store.set_environment_url(job.id, &masker.mask(url)).await?;
        }
        if !request.outputs.is_empty() {
            let masker = self.masker(&task, &job).await?;
            let cleaned: Vec<(&str, String)> = request.outputs.iter().map(|(key, value)| (key.as_str(), masker.mask(value))).collect();
            let pairs: Vec<(&str, &str)> = cleaned.iter().map(|(key, value)| (*key, value.as_str())).collect();
            store.add_outputs(job.id, &pairs, u64::try_from(MAX_OUTPUT_BYTES).unwrap_or(u64::MAX)).await?;
        }
        let Some(result) = request.result else {
            return Ok(TaskStateResponse { cancel: job.status == Status::Cancelled });
        };
        let mut status = result_status(result);
        // A job with `continue-on-error` that fails counts as succeeded, as in `rbit-runner
        // exec`: it fails neither the run nor the jobs that need it, and `fail-fast` leaves
        // its siblings running. Its failed steps still show.
        if status == Status::Failure && self.template(&job).await?.is_some_and(|template| template.continue_on_error) {
            status = Status::Success;
        }
        self.finish(runner, id, status).await?;
        let cancelled = store.job(job.id).await?.is_some_and(|job| job.status == Status::Cancelled);
        Ok(TaskStateResponse { cancel: cancelled })
    }

    /// Stops task `id` of `runner` with `status`: its token dies, its log is finalised, its
    /// run settles, and an ephemeral runner goes away.
    async fn finish(&self, runner: &CachedRunner, id: TaskId, status: Status) -> Result<(), RunnerError> {
        let store = self.ctx().database.actions_runs();
        if let Some(settled) = store.finish_task(id, status, Timestamp::now()).await? {
            tracing::info!(task = id.0, runner = runner.id.0, %status, "task finished");
            self.settled(&settled).await?;
        }
        self.forget_masks(id);
        self.finalise_log(id).await?;
        if runner.ephemeral && self.ctx().database.actions_runners().delete_ephemeral(runner.id).await? {
            self.runner_cache().await?.remove(runner.id);
            tracing::info!(runner = runner.id.0, name = %runner.name, "ephemeral runner removed after its task");
        }
        Ok(())
    }
}

fn step_status(status: StepStatus) -> Status {
    match status {
        StepStatus::Waiting => Status::Waiting,
        StepStatus::Running => Status::Running,
        StepStatus::Success => Status::Success,
        StepStatus::Failure => Status::Failure,
        StepStatus::Cancelled => Status::Cancelled,
        StepStatus::Skipped => Status::Skipped,
    }
}

fn result_status(result: TaskResult) -> Status {
    match result {
        TaskResult::Success => Status::Success,
        TaskResult::Failure => Status::Failure,
        TaskResult::Cancelled => Status::Cancelled,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_states_map_to_statuses() {
        let steps = [
            StepStatus::Waiting,
            StepStatus::Running,
            StepStatus::Success,
            StepStatus::Failure,
            StepStatus::Cancelled,
            StepStatus::Skipped,
        ];
        let statuses: Vec<_> = steps.into_iter().map(step_status).collect();
        assert_eq!(statuses, [Status::Waiting, Status::Running, Status::Success, Status::Failure, Status::Cancelled, Status::Skipped]);
        let results: Vec<_> = [TaskResult::Success, TaskResult::Failure, TaskResult::Cancelled].into_iter().map(result_status).collect();
        assert!(results.iter().all(|status| status.is_done()));
    }
}
