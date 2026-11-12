//! Public read contracts and private on-disk task log identity.
use rbit_core::{RepoId, actions::TaskId};
use std::path::{Path, PathBuf};

/// Most lines one append may carry (`rbit_actions::logs`).
pub use rbit_actions::logs::{MAX_LINES_PER_CALL, readable_line};
/// Windows, ranges, limits and the step view's bounds are owned by `rbit_actions::logs`.
pub use rbit_actions::logs::{LogLimits, LogRange, LogWindow, STEP_VIEW_BYTES, STEP_VIEW_LINES};

/// A whole task log for download: the plain file while the task runs, the gzip file after.
#[derive(Debug)]
pub struct RawLog {
    pub file: tokio::fs::File,
    /// The file is gzip; the text is the decompressed content.
    pub gzip: bool,
}

/// The files of one task's log.
#[derive(Debug, Clone)]
pub(super) struct LogFiles {
    /// The lines while the task runs.
    pub log: PathBuf,
    /// One little-endian `u64` per stored line: the byte offset where it ends.
    pub index: PathBuf,
    /// The lines once finalised.
    pub gzip: PathBuf,
}

impl LogFiles {
    pub fn new(dir: &Path, repo: RepoId, task: TaskId) -> Self {
        let base = dir.join(repo.0.to_string());
        Self {
            log: base.join(format!("{}.log", task.0)),
            index: base.join(format!("{}.idx", task.0)),
            gzip: base.join(format!("{}.log.gz", task.0)),
        }
    }
}

/// How much of a task's log is written (`rbit_actions::logs::LogProgress`).
pub(super) use rbit_actions::logs::LogProgress as Progress;
