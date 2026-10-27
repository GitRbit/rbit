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
//! * **Orphaned files**, once an hour: log files whose task and artifact files whose run is
//!   gone (deleted with its repository, or by run retention before its artifacts expired),
//!   and unfinished artifact uploads ([`ActionsService::remove_orphaned_files`]).
//!
//! The server side of the runner protocol (stream S) owns this file.

use std::{
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use rbit_core::{
    Timestamp,
    actions::{RunId, Status, TaskId},
};

use super::ActionsService;
use crate::ServiceError;

/// How often the sweep runs.
pub const SWEEP_INTERVAL: Duration = Duration::from_mins(1);
/// A file without its row is removed once it is this old: a younger one may belong to a
/// row on its way in.
const ORPHAN_AGE: Duration = Duration::from_hours(1);
/// An unfinished artifact upload is removed once it is this old.
const STALE_UPLOAD_AGE: Duration = Duration::from_hours(24);
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
    // Actions: join (J).
    /// Job conditions decided late (their decision was lost to a restart).
    pub decided: usize, // Actions: artifacts (A).
    /// Artifacts that expired, removed with their files.
    pub expired_artifacts: usize,
    /// Files removed because what they belonged to is gone.
    pub orphaned_files: usize,
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

    // Actions: metrics.
    /// Sets the `rbit_actions_jobs_waiting` and `rbit_actions_runners_online` gauges as of
    /// `now`; the sweep does it once a minute.
    pub async fn record_gauges(&self, now: Timestamp) -> Result<(), ServiceError> {
        let waiting = self.ctx().database.actions_runs().count_all_waiting().await?;
        let online = self.runner_cache().await?.online(now);
        #[allow(clippy::cast_precision_loss)] // Gauges; precision beyond 2^53 jobs or runners is moot.
        {
            metrics::gauge!("rbit_actions_jobs_waiting").set(waiting as f64);
            metrics::gauge!("rbit_actions_runners_online").set(online as f64);
        }
        Ok(())
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
        // Actions: join (J).
        swept.decided = self.decide_stragglers().await?;
        // Actions: artifacts (A).
        swept.expired_artifacts = self.expire_artifacts(now).await?;
        // Actions: metrics.
        self.record_gauges(now).await?;

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
        if now.0.rem_euclid(3600) < 60 {
            swept.orphaned_files = self.remove_orphaned_files().await?;
        }

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

impl ActionsService {
    /// Removes the files whose rows are gone: `logs/<repo>/<task>.*` without their task
    /// (task ids are never reused) and `artifacts/<run>/` without its run (its artifacts
    /// went with it), once older than [`ORPHAN_AGE`]; and uploads in `artifacts/tmp` older
    /// than [`STALE_UPLOAD_AGE`]. Answers how many files were removed.
    pub async fn remove_orphaned_files(&self) -> Result<usize, ServiceError> {
        let store = self.ctx().database.actions_runs();
        let mut removed = 0;
        for dir in entries(&self.logs_dir(), true).await {
            for file in old_files(&dir, ORPHAN_AGE).await {
                let task = file.file_name().and_then(|name| name.to_str()).and_then(|name| name.split('.').next()?.parse().ok());
                if let Some(task) = task
                    && store.task(TaskId(task)).await?.is_none()
                {
                    removed += usize::from(remove(&file).await);
                }
            }
        }
        for dir in entries(&self.artifacts_dir(), true).await {
            let name = dir.file_name().and_then(|name| name.to_str()).unwrap_or_default();
            if name == "tmp" {
                for file in old_files(&dir, STALE_UPLOAD_AGE).await {
                    removed += usize::from(remove(&file).await);
                }
            } else if let Ok(run) = name.parse()
                && store.run(RunId(run)).await?.is_none()
            {
                for file in old_files(&dir, ORPHAN_AGE).await {
                    removed += usize::from(remove(&file).await);
                }
                // Only once empty: a younger file stays, and so does its directory.
                let _ = tokio::fs::remove_dir(&dir).await;
            }
        }
        Ok(removed)
    }
}

/// The directories (`dirs`) or files in `dir`; none when it cannot be read.
async fn entries(dir: &Path, dirs: bool) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Ok(mut reader) = tokio::fs::read_dir(dir).await else { return found };
    while let Ok(Some(entry)) = reader.next_entry().await {
        if entry.file_type().await.is_ok_and(|kind| if dirs { kind.is_dir() } else { kind.is_file() }) {
            found.push(entry.path());
        }
    }
    found
}

/// The files in `dir` last changed at least `age` ago.
async fn old_files(dir: &Path, age: Duration) -> Vec<PathBuf> {
    let mut old = Vec::new();
    for file in entries(dir, false).await {
        let modified = tokio::fs::metadata(&file).await.and_then(|metadata| metadata.modified());
        if modified.is_ok_and(|modified| SystemTime::now().duration_since(modified).is_ok_and(|since| since >= age)) {
            old.push(file);
        }
    }
    old
}

/// Removes `file`; whether it did, logging why not.
async fn remove(file: &Path) -> bool {
    match tokio::fs::remove_file(file).await {
        Ok(()) => true,
        Err(error) => {
            if error.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(%error, file = %file.display(), "cannot remove an orphaned Actions file");
            }
            false
        }
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
