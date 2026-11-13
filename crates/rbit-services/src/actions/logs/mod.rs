//! Task logs (§5.5, §12.6): appends by line offset, the server's masking pass, files under
//! `<data_dir>/actions/logs`, gzip when a task ends, and reads for the job pages.
//!
//! * `POST tasks/{id}/logs?from=<line>` ([`crate::ActionsService::append_log`]) carries lines
//!   `from…` of the task's log. Only lines from the server's current count are stored, so
//!   a resend overlaps harmlessly; a gap is `409` naming the line to resend from. Appends
//!   to one task are serialised.
//! * Every line passes the masking pass (§11.10) and is cut at 64 KiB. Beyond
//!   `[actions] max_log_bytes` lines are counted but dropped, after one marker line.
//! * Files: `<repo id>/<task id>.log` while the task runs and `.log.gz` after
//!   ([`crate::ActionsService::finalise_log`]), with `<task id>.idx` beside them: one
//!   little-endian `u64` per line, where the line ends, so any line range is read without
//!   scanning. Both are synced before the database records the progress; before a process
//!   first appends to a task, files longer than the record are cut back (a crash between
//!   the two).
//! * Reads ([`crate::ActionsService::task_log`], [`crate::ActionsService::step_log`],
//!   [`crate::ActionsService::raw_log`]) work on live and finished logs alike, for readers of the
//!   repository only: the whole log, a step's lines, from a line on (a live tail) or the
//!   last lines within a budget ([`STEP_VIEW_LINES`], [`STEP_VIEW_BYTES`]).
//!
//! The masking pass: a [`Masker`] is built per task when its job specification is
//! assembled, from every secret value, every line of at least four characters of a
//! multi-line one, and the task token, and is kept in memory while the task runs. After a
//! restart it is rebuilt from the task's secrets alone: the token was never stored. The
//! runner masks first (with base64 forms too); this pass is the server's defence in depth.
//! It covers job outputs ([`super::tasks`]) too.
//!
//! The server side of the runner protocol (stream S) owns this file.

mod append;
mod archive;
mod batch;
mod batch_reader;
// Agent loop: failure digests.
mod digest;
mod disk;
mod masking;
mod plan;
mod read;
mod reader;
mod sections;
mod types;
mod work;

pub use archive::add_raw_log;
pub use batch::JobLogDetail;
pub use digest::{DIGEST_ANNOTATIONS, DigestAnnotation, FailedJob, FailureDigest};
pub(crate) use digest::{pick_step, termination_of};
pub use masking::{MASK, MIN_MASKED_CHARS, Masker};
pub use sections::{Sections, has_log, sections};
pub use types::{LogLimits, LogRange, LogWindow, MAX_LINES_PER_CALL, RawLog, STEP_VIEW_BYTES, STEP_VIEW_LINES, readable_line};

#[cfg(test)]
mod tests;

#[cfg(test)]
mod batch_tests;
