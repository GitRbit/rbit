//! The sweep, once a minute while Actions is enabled (§5.5, §11.8, §11.11, §12.6):
//!
//! * **Zombies**: a running task without a state call for `[actions]
//!   zombie_timeout_seconds` fails — its runner is gone or cut off.
//! * **Endless tasks**: a task running longer than `endless_timeout_seconds` fails.
//!   Both say why in the task's log, finalise it, settle the job and its dependents, and
//!   delete an ephemeral runner whose task ended this way.
//! * **Abandoned jobs**: a job waiting longer than `abandoned_timeout_seconds` for a runner
//!   is cancelled.
//! * **Logs**: logs of runs finished more than `log_retention_days` ago are deleted and
//!   marked expired; when the logs kept exceed `max_log_storage_bytes`, the oldest
//!   finished runs lose theirs first. Logs of stopped tasks that were never finalised
//!   (their runner went away) are finalised.
//! * **Runs** finished more than `run_retention_days` ago are deleted, after their logs.
//! * **Registration tokens** that expired or were revoked more than a day ago are deleted.
//! * When runners were last seen is written ([`ActionsService::flush_seen`]).
//!
//! The server side of the runner protocol (stream S) owns this file.

use std::time::Duration;

use rbit_core::{
    Timestamp,
    actions::{RunId, Status, TaskId},
};

use super::ActionsService;
use crate::ServiceError;

/// How often the sweep runs.
pub const SWEEP_INTERVAL: Duration = Duration::from_mins(1);
/// Finished runs whose logs one sweep looks at, per pass.
const RUNS_PER_PASS: u32 = 100;
/// Stale registration tokens are kept this long after they expired or were revoked, so
/// settings can still show them.
const STALE_TOKEN_GRACE_SECONDS: i64 = 24 * 60 * 60;

/// What one sweep did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Swept {
    pub zombies: usize,
    pub endless: usize,
    pub abandoned: usize,
    /// Runs whose logs were deleted (retention and the instance budget).
    pub expired_logs: usize,
    pub deleted_runs: u64,
    pub deleted_tokens: u64,
}

impl ActionsService {
    /// The sweep loop; ends with the service.
    pub(super) async fn run_sweeper(self) {
        let mut ticks = tokio::time::interval(SWEEP_INTERVAL);
        ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                () = self.inner.shutdown.cancelled() => return,
                _ = ticks.tick() => {}
            }
            match self.sweep(Timestamp::now()).await {
                Ok(swept) if swept != Swept::default() => tracing::info!(?swept, "Actions sweep"),
                Ok(_) => {}
                Err(error) => tracing::warn!(%error, "Actions sweep failed; trying again in a minute"),
            }
        }
    }

    /// One sweep as of `now`. See the module documentation.
    pub async fn sweep(&self, now: Timestamp) -> Result<Swept, ServiceError> {
        let config = self.config();
        let store = self.ctx().database.actions_runs();
        let mut swept = Swept::default();

        let silent = store.silent_tasks(before(now, config.zombie_timeout_seconds)).await?;
        let reason = format!("the runner sent no word for {} seconds, so the task failed", config.zombie_timeout_seconds);
        for task in silent {
            swept.zombies += usize::from(self.time_out(task, &reason, now).await?);
        }
        let endless = store.long_running_tasks(before(now, config.endless_timeout_seconds)).await?;
        let reason = format!("the task ran longer than {} seconds, so it failed", config.endless_timeout_seconds);
        for task in endless {
            swept.endless += usize::from(self.time_out(task, &reason, now).await?);
        }
        for settled in store.cancel_abandoned(before(now, config.abandoned_timeout_seconds), now).await? {
            swept.abandoned += 1;
            self.settled(&settled).await?;
        }

        for task in store.unfinalised_logs(RUNS_PER_PASS).await? {
            self.finalise_log(task).await?;
        }
        let log_cutoff = before(now, u64::from(config.log_retention_days).saturating_mul(86_400));
        swept.expired_logs += self.expire_logs_before(log_cutoff, now, RUNS_PER_PASS).await?;
        // The instance budget: the oldest finished runs lose their logs first, one at a time.
        let mut pruned = 0;
        while pruned < RUNS_PER_PASS && store.stored_log_bytes().await? > config.max_log_storage_bytes {
            if self.expire_logs_before(now, now, 1).await? == 0 {
                break;
            }
            pruned += 1;
        }
        swept.expired_logs += usize::try_from(pruned).unwrap_or(usize::MAX);
        // Deleted runs take their log files with them first.
        let run_cutoff = before(now, u64::from(config.run_retention_days).saturating_mul(86_400));
        loop {
            let expired = self.expire_logs_before(run_cutoff, now, RUNS_PER_PASS).await?;
            swept.expired_logs += expired;
            if expired < usize::try_from(RUNS_PER_PASS).unwrap_or(usize::MAX) {
                break;
            }
        }
        swept.deleted_runs = store.delete_finished_before(run_cutoff).await?;

        swept.deleted_tokens =
            self.ctx().database.actions_runners().delete_stale_registration_tokens(Timestamp(now.0 - STALE_TOKEN_GRACE_SECONDS)).await?;
        self.flush_seen().await?;
        Ok(swept)
    }

    /// Fails running task `task` with `reason` in its log; deletes its runner when that was
    /// ephemeral. False when it was no longer running.
    async fn time_out(&self, task: TaskId, reason: &str, now: Timestamp) -> Result<bool, ServiceError> {
        let store = self.ctx().database.actions_runs();
        let Some(stored) = store.task(task).await? else { return Ok(false) };
        self.write_system_line(task, reason).await?;
        let Some(settled) = store.finish_task(task, Status::Failure, now).await? else { return Ok(false) };
        tracing::info!(task = task.0, %reason, "task timed out");
        self.forget_masks(task);
        self.finalise_log(task).await?;
        self.settled(&settled).await?;
        if let Some(runner) = stored.runner_id
            && self.ctx().database.actions_runners().delete_ephemeral(runner).await?
        {
            self.runner_cache().await?.remove(runner);
            tracing::info!(runner = runner.0, "ephemeral runner removed: its task timed out");
        }
        Ok(true)
    }

    /// Deletes the logs of up to `limit` runs finished before `cutoff`, oldest first, and
    /// marks them expired. Answers how many runs lost their logs.
    async fn expire_logs_before(&self, cutoff: Timestamp, now: Timestamp, limit: u32) -> Result<usize, ServiceError> {
        let store = self.ctx().database.actions_runs();
        let runs = store.runs_with_logs(cutoff, limit).await?;
        for (run, tasks) in &runs {
            self.expire_run_logs(*run, tasks, now).await?;
        }
        Ok(runs.len())
    }

    async fn expire_run_logs(&self, run: RunId, tasks: &[TaskId], now: Timestamp) -> Result<(), ServiceError> {
        let store = self.ctx().database.actions_runs();
        if let Some(stored) = store.run(run).await? {
            for task in tasks {
                self.delete_log(stored.repo_id, *task).await?;
            }
        }
        store.expire_logs(run, now).await?;
        Ok(())
    }
}

/// `seconds` before `now`.
fn before(now: Timestamp, seconds: u64) -> Timestamp {
    Timestamp(now.0.saturating_sub(i64::try_from(seconds).unwrap_or(i64::MAX)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cutoffs_never_overflow() {
        assert_eq!(before(Timestamp(1_000), 600), Timestamp(400));
        assert!(before(Timestamp(1_000), u64::MAX).0 < 0);
    }
}
