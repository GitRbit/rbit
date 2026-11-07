//! Finalization, retention removal and streaming raw-log ZIP entries.
use super::types::{GZIP_LEVEL, LogFiles};
use crate::{ActionsService, ServiceError};
use rbit_core::{
    RepoId,
    actions::{LogState, TaskId},
};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    sync::PoisonError,
};

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
        for path in [files.log, files.index, files.gzip] {
            match tokio::fs::remove_file(&path).await {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(ServiceError::internal(error)),
            }
        }
        Ok(())
    }
}

/// Gzips the plain log into place and removes it; the index stays.
pub(super) fn compress(files: &LogFiles) -> std::io::Result<()> {
    let mut source = match File::open(&files.log) {
        Ok(source) => source,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    let partial = files.gzip.with_extension("gz.partial");
    let mut encoder = flate2::write::GzEncoder::new(File::create(&partial)?, flate2::Compression::new(GZIP_LEVEL));
    std::io::copy(&mut source, &mut encoder)?;
    encoder.finish()?.sync_all()?;
    std::fs::rename(&partial, &files.gzip)?;
    drop(source);
    std::fs::remove_file(&files.log)
}

// Actions: run logs.
/// Adds `log` (a raw log's file: gzip or not) to `zip` as `name`, decompressed and read
/// once more to count its size first, so it never needs to be held in memory. A live log
/// is taken as far as it is written now.
///
/// # Errors
///
/// What reading the log or writing the zip fails with.
pub fn add_raw_log<W: Write>(
    zip: &mut crate::actions::downloads::ZipWriter<W>,
    name: &str,
    mut log: File,
    gzip: bool,
) -> std::io::Result<()> {
    if gzip {
        let size = std::io::copy(&mut flate2::read::GzDecoder::new(&log), &mut std::io::sink())?;
        log.seek(SeekFrom::Start(0))?;
        zip.add(name, size, &mut flate2::read::GzDecoder::new(log).take(size))
    } else {
        let size = log.metadata()?.len();
        zip.add(name, size, &mut log.take(size))
    }
}

/// A job name as a file name in the run log archive: no path separators or control
/// characters, at most 100 characters.
pub(super) fn log_file_name(name: &str) -> String {
    let safe: String = name
        .chars()
        .map(|c| if c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') { '_' } else { c })
        .take(100)
        .collect();
    let safe = safe.trim().trim_matches('.').to_owned();
    if safe.is_empty() { "job".to_owned() } else { safe }
}
