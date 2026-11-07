//! Public read contracts and private on-disk task log identity.
use rbit_core::{RepoId, actions::TaskId};
use std::path::{Path, PathBuf};

/// Most lines one append may carry.
pub const MAX_LINES_PER_CALL: usize = 10_000;
/// What a job page's step view shows at most: the last lines of the step.
pub const STEP_VIEW_LINES: usize = 10_000;
/// … and at most this many bytes of them.
pub const STEP_VIEW_BYTES: usize = 2 * 1024 * 1024;
/// gzip level of finalised logs.
pub(super) const GZIP_LEVEL: u32 = 6;

/// Where a window of log lines starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogRange {
    /// From this line of the task log on (a live tail polls with its last `next_line`).
    From(u64),
    /// The last lines, as many as the limits allow.
    Tail,
}

/// Lines of a task's log, as a page shows them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools, reason = "independent facts a page shows")]
pub struct LogWindow {
    /// The task-log line number of `lines[0]`.
    pub first_line: u64,
    pub lines: Vec<String>,
    /// Where the next window starts.
    pub next_line: u64,
    /// Lines before `first_line` that this window left out.
    pub more_before: bool,
    /// The task still runs: more lines may come.
    pub live: bool,
    /// The task wrote more than `[actions] max_log_bytes`; later lines were dropped.
    pub truncated: bool,
    /// Retention or the instance log budget removed the log.
    pub expired: bool,
}

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

/// How much of a task's log is written: the protocol's line count, the bytes on disk and
/// whether later lines were dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Progress {
    pub lines: u64,
    pub bytes: u64,
    pub truncated: bool,
}

/// Maximum retained lines and bytes for one step, with at least one stored line.
#[derive(Debug, Clone, Copy)]
pub struct LogLimits {
    pub lines: usize,
    pub bytes: usize,
}
