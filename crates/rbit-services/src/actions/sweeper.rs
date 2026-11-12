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
//! * **Cache entries** no job restored for `cache_retention_days` are deleted.
//! * **Orphaned files**, once an hour: log files whose task and artifact files whose run is
//!   gone (deleted with its repository, or by run retention before its artifacts expired),
//!   cache files whose entry is gone, and unfinished artifact and cache uploads
//!   ([`ActionsService::remove_orphaned_files`]).
//!
//! The server side of the runner protocol (stream S) owns this file.

use std::{
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use rbit_actions::{
    archives::{ArtifactDirectory, ORPHAN_AGE, STALE_UPLOAD_AGE, UPLOADS_DIRECTORY},
    stale_tasks::{StaleTasks, Timeouts},
};
use rbit_core::{
    Timestamp,
    actions::{RunId, TaskId},
};

use super::{ActionsService, stale_task_port::StaleTaskSession};
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
    // Actions: join (J).
    /// Job conditions decided late (their decision was lost to a restart).
    pub decided: usize, // Actions: artifacts (A).
    /// Artifacts that expired, removed with their files.
    pub expired_artifacts: usize,
    // Actions: shared cache.
    /// Cache entries no job restored for `cache_retention_days`, removed with their files.
    pub expired_caches: usize,
    /// Files removed because what they belonged to is gone.
    pub orphaned_files: usize,
    /// Scratch refs deleted because they outlived their lifetime.
    pub expired_scratch: usize,
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

        let timeouts = Timeouts {
            zombie_seconds: config.zombie_timeout_seconds,
            endless_seconds: config.endless_timeout_seconds,
            abandoned_seconds: config.abandoned_timeout_seconds,
        };
        let stale = StaleTasks::sweep(&StaleTaskSession::new(self), &timeouts, now.0).await?;
        (swept.zombies, swept.endless, swept.abandoned) = (stale.zombies, stale.endless, stale.abandoned);
        if stale.zombies + stale.endless + stale.abandoned > 0 {
            // A freed runner may take jobs it had to leave out (exclusive labels): look again.
            self.bump_tasks_version();
        }
        // Actions: join (J).
        swept.decided = self.decide_stragglers().await?;
        // Actions: artifacts (A).
        swept.expired_artifacts = self.expire_artifacts(now).await?;
        // Actions: shared cache.
        swept.expired_caches = self.expire_caches(now).await?;
        swept.expired_scratch = self.expire_scratch(now).await?;
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
            let kind = ArtifactDirectory::named(name);
            if kind == ArtifactDirectory::Uploads {
                for file in old_files(&dir, STALE_UPLOAD_AGE).await {
                    removed += usize::from(remove(&file).await);
                }
            } else if let ArtifactDirectory::Run(run) = kind
                && store.run(RunId(run)).await?.is_none()
            {
                for file in old_files(&dir, ORPHAN_AGE).await {
                    removed += usize::from(remove(&file).await);
                }
                // Only once empty: a younger file stays, and so does its directory.
                let _ = tokio::fs::remove_dir(&dir).await;
            }
        }
        // Actions: shared cache. Saves cut short, and files of entries that went with their
        // repository.
        for dir in entries(&self.caches_dir(), true).await {
            let stale = dir.file_name().is_some_and(|name| name == UPLOADS_DIRECTORY);
            for file in old_files(&dir, if stale { STALE_UPLOAD_AGE } else { ORPHAN_AGE }).await {
                if stale || self.orphaned_cache_file(&file).await? {
                    removed += usize::from(remove(&file).await);
                }
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

/// `seconds` before `now` ([`rbit_actions::stale_tasks::before`]).
fn before(now: Timestamp, seconds: u64) -> Timestamp {
    Timestamp(rbit_actions::stale_tasks::before(now.0, seconds))
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
