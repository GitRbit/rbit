//! Task logs (§5.5, §12.6): appends by line offset, the server's masking pass, files under
//! `<data_dir>/actions/logs`, gzip when a task ends, and reads for the job pages.
//!
//! * `POST tasks/{id}/logs?from=<line>` ([`ActionsService::append_log`]) carries lines
//!   `from…` of the task's log. Only lines from the server's current count are stored, so
//!   a resend overlaps harmlessly; a gap is `409` naming the line to resend from. Appends
//!   to one task are serialised.
//! * Every line passes the masking pass (§11.10) and is cut at 64 KiB. Beyond
//!   `[actions] max_log_bytes` lines are counted but dropped, after one marker line.
//! * Files: `<repo id>/<task id>.log` while the task runs and `.log.gz` after
//!   ([`ActionsService::finalise_log`]), with `<task id>.idx` beside them: one
//!   little-endian `u64` per line, where the line ends, so any line range is read without
//!   scanning. Both are synced before the database records the progress; before a process
//!   first appends to a task, files longer than the record are cut back (a crash between
//!   the two).
//! * Reads ([`ActionsService::task_log`], [`ActionsService::step_log`],
//!   [`ActionsService::raw_log`]) work on live and finished logs alike, for readers of the
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

use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{Arc, PoisonError},
};

use aho_corasick::{AhoCorasick, MatchKind};
use rbit_core::{
    RepoId,
    actions::{Job, LogState, Status, Step, Task, TaskId},
};
use rbit_workflow::{
    protocol::{LogsResponse, MAX_LOG_LINE_BYTES},
    spec::JobSpec,
};

use super::{
    ActionsService,
    runners::{CachedRunner, RunnerError},
};
use crate::{RepoContext, ServiceError};

/// Masked text becomes this.
pub const MASK: &str = "***";
/// Shorter values are not masked: they would blank out ordinary words.
pub const MIN_MASKED_CHARS: usize = 4;

/// Replaces every known secret in text with [`MASK`].
#[derive(Debug, Clone)]
pub struct Masker {
    automaton: Option<AhoCorasick>,
}

impl Masker {
    /// Masks every value, and every line of a multi-line value, of at least
    /// [`MIN_MASKED_CHARS`] characters.
    pub fn new<'a>(values: impl IntoIterator<Item = &'a str>) -> Self {
        let mut patterns: Vec<&str> = Vec::new();
        for value in values {
            patterns.push(value);
            if value.contains('\n') {
                patterns.extend(value.lines().map(|line| line.trim_end_matches('\r')));
            }
        }
        patterns.retain(|pattern| pattern.chars().count() >= MIN_MASKED_CHARS);
        patterns.sort_unstable();
        patterns.dedup();
        let automaton =
            if patterns.is_empty() { None } else { AhoCorasick::builder().match_kind(MatchKind::LeftmostLongest).build(&patterns).ok() };
        Self { automaton }
    }

    /// Masks nothing.
    pub fn none() -> Self {
        Self { automaton: None }
    }

    /// `text` with every secret replaced.
    pub fn mask(&self, text: &str) -> String {
        match &self.automaton {
            Some(automaton) if automaton.is_match(text) => automaton.replace_all(text, &vec![MASK; automaton.patterns_len()]),
            _ => text.to_owned(),
        }
    }
}

impl ActionsService {
    /// Remembers what `spec`'s task must never store: its secrets and its token.
    pub(super) fn remember_masks(&self, spec: &JobSpec) {
        let values = spec.secrets.values().map(rbit_workflow::secret::Secret::expose).chain([spec.token.expose()]);
        let masker = Arc::new(Masker::new(values));
        self.inner.masks.lock().unwrap_or_else(PoisonError::into_inner).insert(TaskId(spec.task_id.0), masker);
    }

    /// Forgets a finished task's masks.
    pub(super) fn forget_masks(&self, task: TaskId) {
        self.inner.masks.lock().unwrap_or_else(PoisonError::into_inner).remove(&task);
    }

    /// The masker of `task` of `job`: the one built when it was handed out, or else one
    /// rebuilt from its run's secrets.
    pub(super) async fn masker(&self, task: &Task, job: &Job) -> Result<Arc<Masker>, ServiceError> {
        if let Some(masker) = self.inner.masks.lock().unwrap_or_else(PoisonError::into_inner).get(&task.id) {
            return Ok(Arc::clone(masker));
        }
        let Some(run) = self.ctx().database.actions_runs().run(job.run_id).await? else { return Ok(Arc::new(Masker::none())) };
        let mut secrets = if run.trusted { self.task_secrets(&run).await? } else { std::collections::BTreeMap::default() };
        // Actions: environments. The job had its environment's secrets too.
        if let Some(environment) = &job.environment
            && let Ok(values) = self.environment_values(&run, environment).await?
        {
            secrets.extend(values.secrets);
        }
        let masker = Arc::new(Masker::new(secrets.values().map(String::as_str)));
        self.inner.masks.lock().unwrap_or_else(PoisonError::into_inner).insert(task.id, Arc::clone(&masker));
        Ok(masker)
    }
}

// ------------------------------------------------------------------ log files

/// Most lines one append may carry.
pub const MAX_LINES_PER_CALL: usize = 10_000;
/// What a job page's step view shows at most: the last lines of the step.
pub const STEP_VIEW_LINES: usize = 10_000;
/// … and at most this many bytes of them.
pub const STEP_VIEW_BYTES: usize = 2 * 1024 * 1024;
/// gzip level of finalised logs.
const GZIP_LEVEL: u32 = 6;

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
struct LogFiles {
    /// The lines while the task runs.
    log: PathBuf,
    /// One little-endian `u64` per stored line: the byte offset where it ends.
    index: PathBuf,
    /// The lines once finalised.
    gzip: PathBuf,
}

impl LogFiles {
    fn new(dir: &Path, repo: RepoId, task: TaskId) -> Self {
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
struct Progress {
    lines: u64,
    bytes: u64,
    truncated: bool,
}

impl ActionsService {
    /// `<data_dir>/actions/logs`.
    pub(super) fn logs_dir(&self) -> PathBuf {
        self.ctx().config.storage.actions_dir().join("logs")
    }

    /// The lock serialising appends to `task`'s log, and whether this process already
    /// checked its files against the database.
    fn log_lock(&self, task: TaskId) -> Arc<tokio::sync::Mutex<bool>> {
        let mut locks = self.inner.log_locks.lock().unwrap_or_else(PoisonError::into_inner);
        Arc::clone(locks.entry(task).or_default())
    }

    /// `POST tasks/{id}/logs?from=<line>`: `body`'s lines are task-log lines `from…`. Lines
    /// the server already has are dropped, so a resend is harmless; a gap is a conflict
    /// that names the line to resend from. Answers the line count after the append.
    pub async fn append_log(&self, runner: &CachedRunner, id: TaskId, from: u64, body: &[u8]) -> Result<LogsResponse, RunnerError> {
        let text = String::from_utf8_lossy(body);
        let mut lines: Vec<&str> = if text.is_empty() { Vec::new() } else { text.split('\n').collect() };
        if text.ends_with('\n') {
            lines.pop();
        }
        if lines.len() > MAX_LINES_PER_CALL {
            return Err(RunnerError::too_large(format!("send at most {MAX_LINES_PER_CALL} lines per call")));
        }
        let lock = self.log_lock(id);
        let mut checked = lock.lock().await;
        let store = self.ctx().database.actions_runs();
        let task = store.task(id).await?.ok_or_else(|| RunnerError::not_found("there is no such task"))?;
        if task.runner_id != Some(runner.id) {
            return Err(RunnerError::forbidden("this task belongs to another runner"));
        }
        if task.status.is_done() || task.log_state != LogState::Live {
            return Err(RunnerError::conflict("the task has ended; its log is closed"));
        }
        let current = u64::try_from(task.log_lines).unwrap_or_default();
        if from > current {
            return Err(RunnerError::log_gap(current));
        }
        let skip = usize::try_from(current - from).unwrap_or(usize::MAX);
        if skip >= lines.len() {
            return Ok(LogsResponse { next_line: current });
        }
        let job = store.job(task.job_id).await?.ok_or_else(|| RunnerError::not_found("there is no such task"))?;
        let masker = self.masker(&task, &job).await?;
        let fresh: Vec<String> = lines[skip..].iter().map(|line| clean_line(&masker, line)).collect();
        let progress = self.write_lines(&task, &job, fresh, &mut checked).await?;
        Ok(LogsResponse { next_line: progress.lines })
    }

    /// Appends `lines` to `task`'s log with its lock held (`checked` is the lock's flag),
    /// and records the progress.
    async fn write_lines(&self, task: &Task, job: &Job, lines: Vec<String>, checked: &mut bool) -> Result<Progress, ServiceError> {
        let files = LogFiles::new(&self.logs_dir(), job.repo_id, task.id);
        let stored = Progress {
            lines: u64::try_from(task.log_lines).unwrap_or_default(),
            bytes: u64::try_from(task.log_bytes).unwrap_or_default(),
            truncated: task.log_truncated,
        };
        let (recover, max_bytes) = (!*checked, self.config().max_log_bytes);
        let lease = self.admit().map_err(ServiceError::internal)?;
        let progress = tokio::task::spawn_blocking(move || {
            let _lease = lease;
            append_lines(&files, stored, &lines, max_bytes, recover)
        })
        .await
        .map_err(ServiceError::internal)?
        .map_err(ServiceError::internal)?;
        *checked = true;
        // Actions: metrics.
        metrics::counter!("rbit_actions_log_bytes_total").increment(progress.bytes.saturating_sub(stored.bytes));
        let lines = i64::try_from(progress.lines).unwrap_or(i64::MAX);
        let bytes = i64::try_from(progress.bytes).unwrap_or(i64::MAX);
        self.ctx().database.actions_runs().set_log_progress(task.id, lines, bytes, progress.truncated).await?;
        Ok(progress)
    }

    /// Writes one line from rbit itself (why a task failed before it ran) into `task`'s log.
    pub(super) async fn write_system_line(&self, task: TaskId, line: &str) -> Result<(), ServiceError> {
        let lock = self.log_lock(task);
        let mut checked = lock.lock().await;
        let store = self.ctx().database.actions_runs();
        let Some(task) = store.task(task).await? else { return Ok(()) };
        let Some(job) = store.job(task.job_id).await? else { return Ok(()) };
        if task.log_state != LogState::Live {
            return Ok(());
        }
        self.write_lines(&task, &job, vec![clean_line(&Masker::none(), &format!("rbit: {line}"))], &mut checked).await?;
        Ok(())
    }

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
            let lease = self.admit().map_err(ServiceError::internal)?;
            tokio::task::spawn_blocking(move || {
                let _lease = lease;
                compress(&files)
            })
            .await
            .map_err(ServiceError::internal)?
            .map_err(ServiceError::internal)?;
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

    // ------------------------------------------------------------------ reads

    /// Lines of task `task`'s log for a reader of `repo`: from a line on, or the tail,
    /// within `max_lines` and `max_bytes` (at least one line).
    pub async fn task_log(
        &self,
        repo: &RepoContext,
        task: TaskId,
        range: LogRange,
        max_lines: usize,
        max_bytes: usize,
    ) -> Result<LogWindow, ServiceError> {
        let (task, job) = self.readable_task(repo, task).await?;
        self.window(&task, &job, 0, None, range, max_lines, max_bytes).await
    }

    /// Lines of step `step` (its number, from 0) of task `task` for a reader of `repo`. A
    /// running step's window reaches to the end of the log so far; others end where the
    /// runner said. [`STEP_VIEW_LINES`] and [`STEP_VIEW_BYTES`] are what a page shows.
    pub async fn step_log(
        &self,
        repo: &RepoContext,
        task: TaskId,
        step: i64,
        range: LogRange,
        max_lines: usize,
        max_bytes: usize,
    ) -> Result<LogWindow, ServiceError> {
        let (task, job) = self.readable_task(repo, task).await?;
        let steps = self.ctx().database.actions_runs().steps(task.id).await?;
        let step = steps.into_iter().find(|candidate| candidate.number == step).ok_or(ServiceError::NotFound)?;
        let start = u64::try_from(step.log_index).unwrap_or_default();
        let end = if step.status == Status::Running { None } else { Some(start + u64::try_from(step.log_length).unwrap_or_default()) };
        self.window(&task, &job, start, end, range, max_lines, max_bytes).await
    }

    /// The steps of task `task` for a reader of `repo`.
    pub async fn task_steps(&self, repo: &RepoContext, task: TaskId) -> Result<(Task, Vec<Step>), ServiceError> {
        let (task, _) = self.readable_task(repo, task).await?;
        let steps = self.ctx().database.actions_runs().steps(task.id).await?;
        Ok((task, steps))
    }

    /// Task `task`'s whole log for a reader of `repo`; `None` when nothing was written or
    /// the log expired.
    pub async fn raw_log(&self, repo: &RepoContext, task: TaskId) -> Result<Option<RawLog>, ServiceError> {
        let (task, job) = self.readable_task(repo, task).await?;
        let files = LogFiles::new(&self.logs_dir(), job.repo_id, task.id);
        let (path, gzip) = match task.log_state {
            LogState::Expired => return Ok(None),
            LogState::Live => (files.log, false),
            LogState::Final => (files.gzip, true),
        };
        match tokio::fs::File::open(&path).await {
            Ok(file) => Ok(Some(RawLog { file, gzip })),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(ServiceError::internal(error)),
        }
    }

    // Actions: run logs.
    /// The files of run `run`'s log archive, as GitHub names them (`<n>_<job name>.txt`, in
    /// job order), with the task whose log each one is; jobs that never ran have none.
    pub async fn run_log_files(&self, repo: &RepoContext, run: rbit_core::actions::RunId) -> Result<Vec<(String, TaskId)>, ServiceError> {
        let detail = self.run_by_id(repo, run).await?;
        Ok(detail
            .jobs
            .iter()
            .enumerate()
            .filter_map(|(index, job)| Some((format!("{index}_{}.txt", log_file_name(&job.name)), job.task_id?)))
            .collect())
    }

    /// Task `task` and its job, when the job is `repo`'s and the caller may read it.
    async fn readable_task(&self, repo: &RepoContext, task: TaskId) -> Result<(Task, Job), ServiceError> {
        if !repo.access.can_read() {
            return Err(ServiceError::NotFound);
        }
        let store = self.ctx().database.actions_runs();
        let task = store.task(task).await?.ok_or(ServiceError::NotFound)?;
        let job = store.find_job(repo.repo.repo.id, task.job_id).await?.ok_or(ServiceError::NotFound)?;
        Ok((task, job))
    }

    /// Lines `start..end` (`None`: to the end) of the log, windowed by `range` and limits.
    #[allow(clippy::too_many_arguments)]
    async fn window(
        &self,
        task: &Task,
        job: &Job,
        start: u64,
        end: Option<u64>,
        range: LogRange,
        max_lines: usize,
        max_bytes: usize,
    ) -> Result<LogWindow, ServiceError> {
        let mut window = LogWindow {
            first_line: start,
            next_line: start,
            live: task.log_state == LogState::Live && !task.status.is_done(),
            truncated: task.log_truncated,
            expired: task.log_state == LogState::Expired,
            ..LogWindow::default()
        };
        if window.expired {
            return Ok(window);
        }
        let files = LogFiles::new(&self.logs_dir(), job.repo_id, task.id);
        let gzip = task.log_state == LogState::Final;
        let lease = self.admit().map_err(ServiceError::internal)?;
        tokio::task::spawn_blocking(move || {
            let _lease = lease;
            read_window(&files, gzip, start, end, range, max_lines.max(1), max_bytes, &mut window).map(|()| window)
        })
        .await
        .map_err(ServiceError::internal)?
        .map_err(ServiceError::internal)
    }
}

/// A line as stored: masked, without a trailing `\r`, at most `MAX_LOG_LINE_BYTES`.
fn clean_line(masker: &Masker, line: &str) -> String {
    const MARKER: &str = " [line truncated]";
    let mut line = masker.mask(line.strip_suffix('\r').unwrap_or(line));
    if line.len() > MAX_LOG_LINE_BYTES {
        let mut end = MAX_LOG_LINE_BYTES - MARKER.len();
        while !line.is_char_boundary(end) {
            end -= 1;
        }
        line.truncate(end);
        line.push_str(MARKER);
    }
    line
}

/// Appends `lines` after `stored`, within `max_bytes`; with `recover`, first cuts the files
/// back to what the database recorded (a crash between writing them and the row).
fn append_lines(files: &LogFiles, mut progress: Progress, lines: &[String], max_bytes: u64, recover: bool) -> std::io::Result<Progress> {
    if let Some(dir) = files.log.parent() {
        std::fs::create_dir_all(dir)?;
    }
    if recover {
        // Cut with writable handles: an append-only handle may not change the length.
        let log = OpenOptions::new().create(true).truncate(false).write(true).open(&files.log)?;
        let length = log.metadata()?.len();
        if length > progress.bytes {
            log.set_len(progress.bytes)?;
        }
        progress.bytes = progress.bytes.min(length);
        let mut index = OpenOptions::new().create(true).truncate(false).read(true).write(true).open(&files.index)?;
        let entries = index.metadata()?.len() / 8;
        let mut keep = 0;
        let mut entry = [0u8; 8];
        while keep < entries {
            index.read_exact(&mut entry)?;
            if u64::from_le_bytes(entry) > progress.bytes {
                break;
            }
            keep += 1;
        }
        index.set_len(keep * 8)?;
    }
    let mut log = OpenOptions::new().create(true).append(true).open(&files.log)?;
    let mut index = OpenOptions::new().create(true).append(true).open(&files.index)?;
    let mut text = Vec::new();
    let mut ends = Vec::new();
    for line in lines {
        progress.lines += 1;
        if progress.truncated {
            continue;
        }
        let size = u64::try_from(line.len()).unwrap_or(u64::MAX).saturating_add(1);
        if progress.bytes.saturating_add(size) > max_bytes {
            let marker = format!("rbit: the log was cut here: a task may write at most {max_bytes} bytes\n");
            progress.bytes += u64::try_from(marker.len()).unwrap_or_default();
            text.extend_from_slice(marker.as_bytes());
            progress.truncated = true;
            continue;
        }
        text.extend_from_slice(line.as_bytes());
        text.push(b'\n');
        progress.bytes += size;
        ends.extend_from_slice(&progress.bytes.to_le_bytes());
    }
    log.write_all(&text)?;
    index.write_all(&ends)?;
    log.sync_data()?;
    index.sync_data()?;
    Ok(progress)
}

/// Gzips the plain log into place and removes it; the index stays.
fn compress(files: &LogFiles) -> std::io::Result<()> {
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

/// The end offsets of stored lines `from..to` (fewer when the index is shorter).
fn read_ends(index: &mut File, from: u64, to: u64) -> std::io::Result<Vec<u64>> {
    let mut ends = Vec::new();
    if to <= from {
        return Ok(ends);
    }
    index.seek(SeekFrom::Start(from * 8))?;
    let mut bytes = Vec::new();
    index.take((to - from) * 8).read_to_end(&mut bytes)?;
    ends.extend(bytes.as_chunks::<8>().0.iter().map(|chunk| u64::from_le_bytes(*chunk)));
    Ok(ends)
}

/// Fills `window` with lines `start..end` of the log, chosen by `range` and the limits.
#[allow(clippy::too_many_arguments)]
fn read_window(
    files: &LogFiles,
    gzip: bool,
    start: u64,
    end: Option<u64>,
    range: LogRange,
    max_lines: usize,
    max_bytes: usize,
    window: &mut LogWindow,
) -> std::io::Result<()> {
    let mut index = match File::open(&files.index) {
        Ok(index) => index,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    let stored = index.metadata()?.len() / 8;
    let end = end.map_or(stored, |end| end.min(stored));
    let max = u64::try_from(max_lines).unwrap_or(u64::MAX);
    let (first, last) = match range {
        LogRange::From(line) => {
            let first = line.clamp(start, end.max(start));
            (first, end.min(first.saturating_add(max)))
        }
        LogRange::Tail => (end.saturating_sub(max).max(start), end),
    };
    window.first_line = first;
    window.next_line = first;
    if first >= last {
        window.more_before = first > start;
        return Ok(());
    }
    // The end of the line before `first` is where `first` begins.
    let ends = read_ends(&mut index, first.saturating_sub(1), last)?;
    let (begin, ends) =
        if first == 0 { (0, &ends[..]) } else { (ends.first().copied().unwrap_or_default(), ends.get(1..).unwrap_or_default()) };
    let budget = u64::try_from(max_bytes).unwrap_or(u64::MAX);
    let (first, begin, ends) = match range {
        LogRange::From(_) => {
            let count = ends.iter().take_while(|end| **end - begin <= budget).count().max(1).min(ends.len());
            (first, begin, &ends[..count])
        }
        LogRange::Tail => {
            let finish = ends.last().copied().unwrap_or(begin);
            let start_of = |line: usize| if line == 0 { begin } else { ends[line - 1] };
            let mut skip = 0;
            while skip + 1 < ends.len() && finish - start_of(skip) > budget {
                skip += 1;
            }
            (first + u64::try_from(skip).unwrap_or_default(), start_of(skip), &ends[skip..])
        }
    };
    let finish = ends.last().copied().unwrap_or(begin);
    let mut bytes = vec![0u8; usize::try_from(finish - begin).unwrap_or_default()];
    if gzip {
        let mut decoder = flate2::read::GzDecoder::new(File::open(&files.gzip)?);
        std::io::copy(&mut (&mut decoder).take(begin), &mut std::io::sink())?;
        decoder.read_exact(&mut bytes)?;
    } else {
        let mut log = File::open(&files.log)?;
        log.seek(SeekFrom::Start(begin))?;
        log.read_exact(&mut bytes)?;
    }
    let mut offset = begin;
    for end in ends {
        let from = usize::try_from(offset - begin).unwrap_or_default();
        let to = usize::try_from(*end - begin).unwrap_or_default();
        let line = bytes.get(from..to).unwrap_or_default();
        window.lines.push(String::from_utf8_lossy(line.strip_suffix(b"\n").unwrap_or(line)).into_owned());
        offset = *end;
    }
    window.first_line = first;
    window.next_line = first + u64::try_from(ends.len()).unwrap_or_default();
    window.more_before = first > start;
    Ok(())
}

// Actions: run logs.
/// Adds `log` (a raw log's file: gzip or not) to `zip` as `name`, decompressed and read
/// once more to count its size first, so it never needs to be held in memory. A live log
/// is taken as far as it is written now.
///
/// # Errors
///
/// What reading the log or writing the zip fails with.
pub fn add_raw_log<W: Write>(zip: &mut super::downloads::ZipWriter<W>, name: &str, mut log: File, gzip: bool) -> std::io::Result<()> {
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
fn log_file_name(name: &str) -> String {
    let safe: String = name
        .chars()
        .map(|c| if c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') { '_' } else { c })
        .take(100)
        .collect();
    let safe = safe.trim().trim_matches('.').to_owned();
    if safe.is_empty() { "job".to_owned() } else { safe }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Actions: run logs.
    #[test]
    fn job_names_become_safe_file_names() {
        assert_eq!(log_file_name("build (ubuntu-latest, 1.80)"), "build (ubuntu-latest, 1.80)");
        assert_eq!(log_file_name("a/b\\c:d"), "a_b_c_d");
        assert_eq!(log_file_name(".."), "job");
    }

    #[test]
    fn secrets_and_their_lines_are_masked() {
        let masker = Masker::new(["hunter2-secret", "line-one\nline-two\nab", "abc"]);
        assert_eq!(masker.mask("password=hunter2-secret!"), "password=***!");
        assert_eq!(masker.mask("got line-two and ab"), "got *** and ab", "short lines stay");
        assert_eq!(masker.mask("abc is too short to mask"), "abc is too short to mask");
        assert_eq!(masker.mask("line-one\nline-two\nab"), "***", "the whole value at once");
        assert_eq!(masker.mask("line-one\nline-two"), "***\n***");
        assert_eq!(Masker::none().mask("hunter2-secret"), "hunter2-secret");
        assert_eq!(Masker::new(["x"]).mask("x"), "x");
    }

    fn log_files() -> (tempfile::TempDir, LogFiles) {
        let dir = tempfile::tempdir().unwrap();
        let files = LogFiles::new(dir.path(), RepoId(1), TaskId(2));
        (dir, files)
    }

    fn lines(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    fn window(
        files: &LogFiles,
        gzip: bool,
        start: u64,
        end: Option<u64>,
        range: LogRange,
        max_lines: usize,
        max_bytes: usize,
    ) -> LogWindow {
        let mut window = LogWindow::default();
        read_window(files, gzip, start, end, range, max_lines, max_bytes, &mut window).unwrap();
        window
    }

    #[test]
    fn lines_are_appended_indexed_and_read_by_range() {
        let (_dir, files) = log_files();
        let start = Progress { lines: 0, bytes: 0, truncated: false };
        let progress = append_lines(&files, start, &lines(&["one", "two", "three"]), 1024, true).unwrap();
        assert_eq!(progress, Progress { lines: 3, bytes: 14, truncated: false });
        let progress = append_lines(&files, progress, &lines(&["four"]), 1024, false).unwrap();
        assert_eq!(std::fs::read_to_string(&files.log).unwrap(), "one\ntwo\nthree\nfour\n");

        let all = window(&files, false, 0, None, LogRange::From(0), 100, 1 << 20);
        assert_eq!((all.lines.clone(), all.first_line, all.next_line), (lines(&["one", "two", "three", "four"]), 0, 4));
        let middle = window(&files, false, 1, Some(3), LogRange::From(0), 100, 1 << 20);
        assert_eq!((middle.lines.clone(), middle.first_line, middle.more_before), (lines(&["two", "three"]), 1, false));
        let tail = window(&files, false, 0, None, LogRange::Tail, 2, 1 << 20);
        assert_eq!((tail.lines.clone(), tail.first_line, tail.more_before), (lines(&["three", "four"]), 2, true));
        let budget = window(&files, false, 0, None, LogRange::Tail, 100, 6);
        assert_eq!(budget.lines, lines(&["four"]), "the tail keeps within the byte budget");
        let from = window(&files, false, 0, None, LogRange::From(1), 100, 5);
        assert_eq!((from.lines.clone(), from.next_line), (lines(&["two"]), 2));
        let one = window(&files, false, 0, None, LogRange::From(2), 100, 1);
        assert_eq!(one.lines, lines(&["three"]), "at least one line");
        assert_eq!(window(&files, false, 0, None, LogRange::From(9), 100, 100).lines, Vec::<String>::new());

        compress(&files).unwrap();
        assert!(!files.log.exists() && files.gzip.exists() && files.index.exists());
        let finished = window(&files, true, 0, None, LogRange::From(1), 2, 1 << 20);
        assert_eq!(finished.lines, lines(&["two", "three"]), "a finished log reads the same");
        assert_eq!(progress.lines, 4);
    }

    #[test]
    fn logs_stop_at_their_budget_and_recover_from_a_crash() {
        let (_dir, files) = log_files();
        let start = Progress { lines: 0, bytes: 0, truncated: false };
        let progress = append_lines(&files, start, &lines(&["12345", "67890", "abcde"]), 13, true).unwrap();
        assert_eq!((progress.lines, progress.truncated), (3, true));
        let text = std::fs::read_to_string(&files.log).unwrap();
        assert!(text.starts_with("12345\n67890\nrbit: the log was cut here"), "{text}");
        let progress = append_lines(&files, progress, &lines(&["more"]), 13, false).unwrap();
        assert_eq!(progress.lines, 4, "dropped lines still count");
        assert_eq!(window(&files, false, 0, None, LogRange::From(0), 100, 1 << 20).lines, lines(&["12345", "67890"]));

        // The files got ahead of the database: the next process cuts them back first.
        let (_dir, files) = log_files();
        let recorded = append_lines(&files, start, &lines(&["kept"]), 1024, true).unwrap();
        append_lines(&files, recorded, &lines(&["lost"]), 1024, false).unwrap();
        let resumed = append_lines(&files, recorded, &lines(&["again"]), 1024, true).unwrap();
        assert_eq!(resumed.lines, 2);
        assert_eq!(std::fs::read_to_string(&files.log).unwrap(), "kept\nagain\n");
        assert_eq!(window(&files, false, 0, None, LogRange::From(0), 100, 1 << 20).lines, lines(&["kept", "again"]));
    }

    #[test]
    fn lines_are_masked_trimmed_and_capped() {
        let masker = Masker::new(["s3cr3t-value"]);
        assert_eq!(clean_line(&masker, "key=s3cr3t-value\r"), "key=***");
        let long = clean_line(&masker, &"é".repeat(MAX_LOG_LINE_BYTES));
        assert!(long.len() <= MAX_LOG_LINE_BYTES && long.ends_with("[line truncated]"));
    }

    #[test]
    fn overlapping_secrets_mask_the_longest() {
        let masker = Masker::new(["token", "token-extended"]);
        assert_eq!(masker.mask("a token-extended b token"), "a *** b ***");
    }
}
