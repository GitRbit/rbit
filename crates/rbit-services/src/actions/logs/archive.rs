//! Finalization, retention removal and streaming raw-log ZIP entries.
use super::types::LogFiles;
use crate::{ActionsService, ServiceError};
use rbit_core::{
    RepoId,
    actions::{LogState, TaskId},
};
use std::{fs::File, io::Write, sync::PoisonError};

impl ActionsService {
    /// Compresses a stopped task's log (gzip, level 6) and marks it final. Nothing for a
    /// log that is already final or expired.
    pub async fn finalise_log(&self, task: TaskId) -> Result<(), ServiceError> {
        let lock = self.log_lock(task);
        let guard = lock.lock().await;
        let store = self.ctx().database.actions_runs();
        let Some(stored) = store.task(task).await? else { return Ok(()) };
        if stored.log_state == LogState::Live {
            let job = store.job(stored.job_id).await?.ok_or(ServiceError::NotFound)?;
            let files = LogFiles::new(&self.logs_dir(), job.repo_id, task);
            tokio::task::spawn_blocking(move || compress(&files)).await.map_err(ServiceError::internal)?.map_err(ServiceError::internal)?;
            store.set_log_state(task, LogState::Final).await?;
        }
        drop(guard);
        self.inner.log_locks.lock().unwrap_or_else(PoisonError::into_inner).remove(&task);
        Ok(())
    }

    /// Removes a task's log files (retention, the instance log budget, deleted runs).
    pub async fn delete_log(&self, repo: RepoId, task: TaskId) -> Result<(), ServiceError> {
        let files = LogFiles::new(&self.logs_dir(), repo, task);
        tokio::task::spawn_blocking(move || rbit_actions::logs::remove(&files))
            .await
            .map_err(ServiceError::internal)?
            .map_err(ServiceError::internal)
    }
}

/// Gzips the plain log into place and removes it; the index stays
/// (`rbit_actions::logs::compress`).
pub(super) fn compress(files: &LogFiles) -> std::io::Result<()> {
    rbit_actions::logs::compress(files)
}

// Actions: run logs.
/// Adds `log` (a raw log's file: gzip or not) to `zip` as `name`, decompressed and read
/// once more to count its size first, so it never needs to be held in memory. A live log
/// is taken as far as it is written now.
///
/// # Errors
///
/// What reading the log or writing the zip fails with.
pub fn add_raw_log<W: Write>(zip: &mut crate::actions::downloads::ZipWriter<W>, name: &str, log: File, gzip: bool) -> std::io::Result<()> {
    rbit_actions::logs::add_raw_log(zip, name, log, gzip)
}

/// A job name as a file name in the run log archive (`rbit_actions::logs`).
pub(super) use rbit_actions::logs::log_file_name;
