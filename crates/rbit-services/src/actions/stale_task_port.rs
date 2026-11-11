//! The sweep's stale-task port over `actions_runs`, task logs and masks, settling and the
//! runner store. The passes, cutoffs, reasons and step order are
//! `rbit_actions::stale_tasks::StaleTasks`; the SQL that selects and moves tasks stays the
//! race authority.
use rbit_actions::stale_tasks::{StaleTaskStore, Staleness};
use rbit_core::{
    Timestamp,
    actions::{Status, Task, TaskId, Termination, TerminationReason},
};
use rbit_storage::sqlite::actions_runs::Settled;

use super::ActionsService;
use crate::ServiceError;

pub(super) struct StaleTaskSession<'a> {
    service: &'a ActionsService,
}

impl<'a> StaleTaskSession<'a> {
    pub(super) fn new(service: &'a ActionsService) -> Self {
        Self { service }
    }
}

impl StaleTaskStore for StaleTaskSession<'_> {
    type Error = ServiceError;
    type TaskId = TaskId;
    type Task = Task;
    type Settled = Settled;

    async fn silent(&self, before: i64) -> Result<Vec<TaskId>, ServiceError> {
        Ok(self.service.ctx().database.actions_runs().silent_tasks(Timestamp(before)).await?)
    }

    async fn long_running(&self, before: i64) -> Result<Vec<TaskId>, ServiceError> {
        Ok(self.service.ctx().database.actions_runs().long_running_tasks(Timestamp(before)).await?)
    }

    async fn cancel_abandoned(&self, before: i64, now: i64) -> Result<Vec<Settled>, ServiceError> {
        Ok(self.service.ctx().database.actions_runs().cancel_abandoned(Timestamp(before), Timestamp(now)).await?)
    }

    async fn task(&self, id: TaskId) -> Result<Option<Task>, ServiceError> {
        Ok(self.service.ctx().database.actions_runs().task(id).await?)
    }

    async fn note(&self, id: TaskId, reason: &str) -> Result<(), ServiceError> {
        self.service.write_system_line(id, reason).await
    }

    async fn fail(&self, id: TaskId, staleness: Staleness, now: i64) -> Result<Option<Settled>, ServiceError> {
        let store = self.service.ctx().database.actions_runs();
        // Agent loop: a silent runner is lost; an endless task ran out of time.
        let reason = match staleness {
            Staleness::Silent => TerminationReason::RunnerLost,
            Staleness::Endless => TerminationReason::Timeout,
        };
        store.record_termination(id, Termination::because(reason), Timestamp(now)).await?;
        Ok(store.finish_task(id, Status::Failure, Timestamp(now)).await?)
    }

    async fn close(&self, id: TaskId, reason: &str) -> Result<(), ServiceError> {
        tracing::info!(task = id.0, %reason, "task timed out");
        self.service.forget_masks(id);
        self.service.finalise_log(id).await
    }

    async fn settle(&self, settled: &Settled) -> Result<(), ServiceError> {
        self.service.settled(settled).await
    }

    async fn retire_ephemeral_runner(&self, task: &Task) -> Result<(), ServiceError> {
        if let Some(runner) = task.runner_id
            && self.service.ctx().database.actions_runners().delete_ephemeral(runner).await?
        {
            self.service.runner_cache().await?.remove(runner);
            tracing::info!(runner = runner.0, "ephemeral runner removed: its task timed out");
        }
        Ok(())
    }
}
