//! One task from claim to final report (§12.5.4 and §12.6 "Lifecycle").
//!
//! [`run`] gives the task a directory under `<work_dir>/tasks/<id>`, hands the job to an
//! [`Executor`] and, while it runs, reports a heartbeat with the step states at least every
//! [`MAX_HEARTBEAT_INTERVAL`], flushing the log first. The answer to every state call can
//! cancel the task; so can a `404`, which means the server no longer knows it. When the
//! executor returns, the rest of the log and the final state are sent, retried with the
//! reconnect backoff for up to [`FINAL_REPORT_DEADLINE`], and the directory is removed.
//!
//! The executor sees the task only through its [`Reporter`]: it appends log lines, moves
//! steps through their states, records outputs and step summaries, and watches
//! [`Reporter::cancellation`]. Summaries are masked like the log and kept within
//! [`MAX_STEP_SUMMARY_BYTES`] each and [`MAX_TASK_SUMMARY_BYTES`] in all, counted as JSON
//! encodes them, so a final state call always fits its cap.

use std::{
    collections::{BTreeMap, BTreeSet},
    future::Future,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, SystemTime},
};

use rbit_workflow::{
    protocol::{
        MAX_HEARTBEAT_INTERVAL, MAX_STEP_SUMMARY_BYTES, MAX_TASK_SUMMARY_BYTES, StepState, StepStatus, TaskId, TaskResult, TaskStateRequest,
    },
    spec::JobSpec,
};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use crate::{
    client::{Backoff, Client, Disposition},
    logs::TaskLog,
    mask::Masker,
};

/// How long the end of a task is retried while the server is unreachable. The server's
/// sweeper fails a task it has not heard from long before that.
pub const FINAL_REPORT_DEADLINE: Duration = Duration::from_mins(15);
/// The shortest gap between two state calls, however often steps change.
const MIN_REPORT_GAP: Duration = Duration::from_millis(500);
// Actions: step summaries.
/// What ends a summary that had to be cut.
const SUMMARY_CUT: &str = "\n\n*rbit: this summary was cut to fit its size limit.*\n";

/// Runs the jobs of claimed tasks.
pub trait Executor: Send + Sync + 'static {
    /// Runs `spec` to its end and says how it ended. Step states, log lines and outputs go
    /// through `reporter`; a cancelled reporter means stop.
    fn execute(&self, spec: JobSpec, reporter: Arc<Reporter>) -> impl Future<Output = TaskResult> + Send;
}

/// Where a task's files live: `<work_dir>/tasks/<id>`.
pub fn task_dir(work_dir: &Path, task: TaskId) -> PathBuf {
    work_dir.join("tasks").join(task.to_string())
}

/// A sink for log lines.
struct Echo(Box<dyn Fn(&str) + Send + Sync>);

impl std::fmt::Debug for Echo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Echo")
    }
}

/// What the executor tells the server about a task.
#[derive(Debug)]
pub struct Reporter {
    task_id: TaskId,
    dir: PathBuf,
    log: TaskLog,
    /// Everything logged or reported as an output passes through it.
    masker: Masker,
    /// Where `rbit-runner exec` prints the log as it is written.
    echo: Option<Echo>,
    progress: Mutex<Progress>,
    changed: Notify,
    cancel: CancellationToken,
}

#[derive(Debug)]
struct Progress {
    steps: Vec<StepState>,
    /// The step whose lines are being written.
    current: Option<usize>,
    outputs: BTreeMap<String, String>,
    // Actions: step summaries.
    /// Step summaries, masked and within their limits, by step index.
    summaries: BTreeMap<u32, String>,
    /// The steps whose summary the server accepted: it keeps the first one it sees, so they
    /// are not sent again.
    acknowledged: BTreeSet<u32>,
    /// The server answered `404`: nothing more is reported.
    gone: bool,
}

impl Reporter {
    /// A reporter for a task of `steps` steps, with its spool in `dir`.
    pub fn new(task_id: TaskId, dir: &Path, steps: usize) -> std::io::Result<Self> {
        std::fs::create_dir_all(dir)?;
        let log = TaskLog::create(&dir.join("log.spool"))?;
        let steps = (0..steps)
            .map(|index| StepState {
                index: u32::try_from(index).unwrap_or(u32::MAX),
                status: StepStatus::Waiting,
                started_at: None,
                stopped_at: None,
                log_index: 0,
                log_length: 0,
            })
            .collect();
        Ok(Self {
            task_id,
            dir: dir.to_path_buf(),
            log,
            masker: Masker::default(),
            echo: None,
            progress: Mutex::new(Progress {
                steps,
                current: None,
                outputs: BTreeMap::new(),
                summaries: BTreeMap::new(),
                acknowledged: BTreeSet::new(),
                gone: false,
            }),
            changed: Notify::new(),
            cancel: CancellationToken::new(),
        })
    }

    /// The task log so far, for tests.
    #[cfg(test)]
    pub fn lines(&self) -> Vec<String> {
        self.log.lines()
    }

    /// The step states and outputs so far, for tests.
    #[cfg(test)]
    pub fn state(&self) -> TaskStateRequest {
        self.snapshot(None)
    }

    /// The task's directory, `<work_dir>/tasks/<id>`.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Masks `value` in every later log line and output (secrets, the task token,
    /// `::add-mask::`).
    pub fn add_mask(&self, value: &str) {
        if !self.masker.add(value) {
            self.log.push("##[warning]this task masks too many values; `::add-mask::` was refused");
        }
    }

    /// `text` as it may leave the runner.
    pub fn mask(&self, text: &str) -> String {
        self.masker.mask(text).into_owned()
    }

    /// The same reporter, also printing each log line, masked, to `sink`.
    #[must_use]
    pub fn echo_to(mut self, sink: impl Fn(&str) + Send + Sync + 'static) -> Self {
        self.echo = Some(Echo(Box::new(sink)));
        self
    }

    /// The job's outputs as reported.
    pub fn outputs(&self) -> BTreeMap<String, String> {
        self.lock().outputs.clone()
    }

    /// The same reporter, also cancelled when `parent` is (the runner's shutdown).
    #[must_use]
    pub fn cancelled_by(mut self, parent: &CancellationToken) -> Self {
        self.cancel = parent.child_token();
        self
    }

    /// Cancelled when the server cancels the task or forgets it.
    pub fn cancellation(&self) -> &CancellationToken {
        &self.cancel
    }

    /// Appends a line (or several, at newlines) to the task log, counted to the current
    /// step.
    pub fn log(&self, text: &str) {
        let masked = self.masker.mask(text);
        if let Some(Echo(sink)) = &self.echo {
            for line in masked.split('\n') {
                sink(line.strip_suffix('\r').unwrap_or(line));
            }
        }
        let kept = self.log.push(&masked);
        let mut progress = self.lock();
        if let Some(index) = progress.current {
            progress.steps[index].log_length += kept;
        }
    }

    /// Step `index` starts: its log begins at the next line.
    pub fn start_step(&self, index: usize) {
        let line = self.log.len();
        let mut progress = self.lock();
        let Some(step) = progress.steps.get_mut(index) else { return };
        step.status = StepStatus::Running;
        step.started_at = Some(now());
        step.log_index = line;
        step.log_length = 0;
        progress.current = Some(index);
        drop(progress);
        self.changed.notify_one();
    }

    /// Step `index` ended with `status`. A step that never started gets an empty log at
    /// the current line.
    pub fn finish_step(&self, index: usize, status: StepStatus) {
        let line = self.log.len();
        let mut progress = self.lock();
        let Some(step) = progress.steps.get_mut(index) else { return };
        if step.started_at.is_none() {
            step.log_index = line;
            if status != StepStatus::Skipped {
                step.started_at = Some(now());
            }
        }
        step.status = status;
        step.stopped_at = step.started_at.map(|started| now().max(started));
        if progress.current == Some(index) {
            progress.current = None;
        }
        drop(progress);
        self.changed.notify_one();
    }

    /// Records the job's outputs, masked.
    pub fn set_outputs(&self, outputs: BTreeMap<String, String>) {
        let outputs = outputs.into_iter().map(|(name, value)| (name, self.mask(&value))).collect();
        self.lock().outputs = outputs;
        self.changed.notify_one();
    }

    // Actions: step summaries.
    /// Records what step `index` wrote to `GITHUB_STEP_SUMMARY`, masked. A summary longer
    /// than its share of the limits is cut with a note; once the task's budget is spent,
    /// later summaries are dropped with a warning in the log. A blank one is nothing.
    pub fn set_summary(&self, index: usize, text: &str) {
        let Ok(index) = u32::try_from(index) else { return };
        if text.trim().is_empty() {
            return;
        }
        let masked = self.mask(text);
        let mut progress = self.lock();
        let used: usize = progress.summaries.values().map(|summary| encoded_len(summary)).sum();
        let budget = MAX_STEP_SUMMARY_BYTES.min(MAX_TASK_SUMMARY_BYTES.saturating_sub(used));
        let fitted = fit_summary(&masked, budget);
        let cut = fitted.as_ref().is_some_and(|fitted| fitted.len() != masked.len());
        if let Some(fitted) = fitted {
            progress.summaries.insert(index, fitted);
        }
        drop(progress);
        if cut {
            self.log("##[warning]the step summary was cut to fit its size limit");
        } else if !self.lock().summaries.contains_key(&index) {
            self.log("##[warning]the task's step summaries reached their size limit; this step's summary was dropped");
        }
        self.changed.notify_one();
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Progress> {
        self.progress.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn snapshot(&self, result: Option<TaskResult>) -> TaskStateRequest {
        let progress = self.lock();
        let mut steps = progress.steps.clone();
        if result.is_some() {
            // A final state has no running or waiting steps left.
            for step in &mut steps {
                match step.status {
                    StepStatus::Running => {
                        step.status = StepStatus::Cancelled;
                        step.stopped_at = step.started_at.map(|started| now().max(started));
                    }
                    StepStatus::Waiting => step.status = StepStatus::Skipped,
                    _ => {}
                }
            }
        }
        let summaries = progress
            .summaries
            .iter()
            .filter(|(index, _)| !progress.acknowledged.contains(index))
            .map(|(index, summary)| (*index, summary.clone()))
            .collect();
        TaskStateRequest { steps, outputs: progress.outputs.clone(), result, summaries }
    }

    // Actions: step summaries.
    /// The server accepted a state call carrying `sent`: those summaries are not sent again.
    fn acknowledge(&self, sent: &BTreeMap<u32, String>) {
        self.lock().acknowledged.extend(sent.keys().copied());
    }

    fn is_gone(&self) -> bool {
        self.lock().gone
    }

    /// The server no longer knows this task: stop it, and stop reporting.
    fn forget(&self, why: &str) {
        tracing::warn!(task = %self.task_id, "{why}; cancelling the task");
        self.lock().gone = true;
        self.cancel.cancel();
    }

    /// One heartbeat: the log so far, then the state. `Err` when the call should be retried.
    async fn report(&self, client: &Client, result: Option<TaskResult>) -> Result<(), ()> {
        if self.is_gone() {
            return Ok(());
        }
        if let Err(error) = self.log.flush(client, self.task_id).await {
            match error.disposition() {
                Disposition::Retry => return Err(()),
                Disposition::Gone | Disposition::Fatal => {
                    self.forget(&format!("the server refused the task's log ({error})"));
                    return Ok(());
                }
                Disposition::Refused => {
                    tracing::error!(task = %self.task_id, %error, "the server refused the task's log; the rest of it is not sent");
                    self.log.give_up();
                }
            }
        }
        let state = self.snapshot(result);
        if let Err(error) = state.validate() {
            tracing::error!(task = %self.task_id, %error, "the task's state is invalid and was not sent");
            return Ok(());
        }
        match client.task_state(self.task_id, &state).await {
            Ok(answer) => {
                self.acknowledge(&state.summaries);
                if answer.cancel && !self.cancel.is_cancelled() {
                    tracing::info!(task = %self.task_id, "the server cancelled the task");
                    self.cancel.cancel();
                }
                Ok(())
            }
            Err(error) => match error.disposition() {
                Disposition::Retry => {
                    tracing::debug!(task = %self.task_id, %error, "state report failed; retrying");
                    Err(())
                }
                Disposition::Gone | Disposition::Fatal => {
                    self.forget(&format!("the server no longer accepts this task ({error})"));
                    Ok(())
                }
                Disposition::Refused => {
                    tracing::error!(task = %self.task_id, %error, "the server refused the task's state");
                    Ok(())
                }
            },
        }
    }
}

fn now() -> i64 {
    SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |elapsed| i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX))
}

/// How often a running task reports, and what cancels every task at once.
#[derive(Debug, Clone)]
pub struct Pacing {
    pub heartbeat: Duration,
    pub backoff: Backoff,
    pub final_deadline: Duration,
    /// The runner's shutdown: it cancels every running task, and then a final report is
    /// retried for [`SHUTDOWN_REPORT_DEADLINE`] only.
    pub cancel_all: CancellationToken,
}

/// How long the end of a task is retried once the runner is shutting down.
pub const SHUTDOWN_REPORT_DEADLINE: Duration = Duration::from_secs(30);

impl Pacing {
    /// The server's heartbeat interval, never longer than the protocol allows.
    pub fn new(heartbeat_seconds: u32) -> Self {
        Self {
            heartbeat: Duration::from_secs(heartbeat_seconds.into()).clamp(Duration::from_secs(1), MAX_HEARTBEAT_INTERVAL),
            backoff: Backoff::RECONNECT,
            final_deadline: FINAL_REPORT_DEADLINE,
            cancel_all: CancellationToken::new(),
        }
    }
}

/// Runs a claimed task with `executor` and reports it to the end.
pub async fn run<E: Executor>(client: Client, work_dir: &Path, spec: JobSpec, executor: Arc<E>, pacing: Pacing) {
    let task = spec.task_id;
    let dir = task_dir(work_dir, task);
    let reporter = match Reporter::new(task, &dir, spec.template.steps.len()) {
        Ok(reporter) => {
            let reporter = reporter.cancelled_by(&pacing.cancel_all);
            // Before anything is logged: every secret, and the token.
            for value in spec.secrets.values().chain([&spec.token]) {
                reporter.add_mask(value.expose());
            }
            Arc::new(reporter)
        }
        Err(error) => {
            tracing::error!(task = %task, %error, dir = %dir.display(), "cannot create the task directory");
            return;
        }
    };
    tracing::info!(task = %task, job = %spec.template.name, "task started");

    let stop = CancellationToken::new();
    let heartbeat = tokio::spawn(heartbeat(client.clone(), Arc::clone(&reporter), pacing.clone(), stop.clone()));
    let result = executor.execute(spec, Arc::clone(&reporter)).await;
    stop.cancel();
    let _ = heartbeat.await;

    finish(&client, &reporter, result, pacing).await;
    tracing::info!(task = %task, ?result, "task finished");
    if let Err(error) = std::fs::remove_dir_all(&dir) {
        tracing::warn!(task = %task, %error, dir = %dir.display(), "cannot remove the task directory");
    }
}

/// Fails a task the runner could not even read, so the server need not wait for the
/// sweeper.
pub async fn refuse(client: Client, work_dir: &Path, task: TaskId, error: &str, pacing: Pacing) {
    let dir = task_dir(work_dir, task);
    match Reporter::new(task, &dir, 0) {
        Ok(reporter) => {
            reporter.log(&format!("rbit-runner cannot run this job: {error}"));
            finish(&client, &reporter, TaskResult::Failure, pacing).await;
        }
        Err(io) => tracing::error!(task = %task, error = %io, "cannot create the task directory"),
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// Reports `reporter` until `stop`: every heartbeat interval, soon after a change, and with
/// the reconnect backoff while calls fail.
async fn heartbeat(client: Client, reporter: Arc<Reporter>, pacing: Pacing, stop: CancellationToken) {
    let mut backoff = pacing.backoff;
    let mut wait = Duration::ZERO;
    loop {
        tokio::select! {
            () = stop.cancelled() => return,
            () = tokio::time::sleep(wait) => {}
        }
        if reporter.is_gone() {
            return;
        }
        if reporter.report(&client, None).await.is_ok() {
            backoff.reset();
            // The next report waits for the interval or a change, but leaves a gap.
            tokio::select! {
                () = stop.cancelled() => return,
                () = tokio::time::sleep(pacing.heartbeat) => {}
                () = reporter.changed.notified() => {}
            }
            wait = MIN_REPORT_GAP;
        } else {
            wait = backoff.next_delay();
        }
    }
}

/// Sends the rest of the log and the final state, retrying until `pacing.final_deadline`.
async fn finish(client: &Client, reporter: &Reporter, result: TaskResult, pacing: Pacing) {
    let mut backoff = pacing.backoff;
    let mut deadline = tokio::time::Instant::now() + pacing.final_deadline;
    loop {
        let sent = reporter.report(client, Some(result)).await.is_ok();
        if sent && (reporter.log.is_flushed() || reporter.is_gone()) {
            return;
        }
        if pacing.cancel_all.is_cancelled() {
            deadline = deadline.min(tokio::time::Instant::now() + SHUTDOWN_REPORT_DEADLINE);
        }
        if tokio::time::Instant::now() >= deadline {
            tracing::error!(task = %reporter.task_id, "giving up on reporting the task's end; the server will time it out");
            return;
        }
        if sent {
            backoff.reset();
        } else {
            tokio::time::sleep(backoff.next_delay()).await;
        }
    }
}

// Actions: step summaries.
/// Bytes of `text` as a JSON string, without its quotes.
fn encoded_len(text: &str) -> usize {
    serde_json::to_string(text).map_or(usize::MAX, |encoded| encoded.len().saturating_sub(2))
}

/// `text` within `budget` encoded bytes: whole when it fits, else its longest beginning (on
/// a character boundary) followed by [`SUMMARY_CUT`]; `None` when not even the note fits.
fn fit_summary(text: &str, budget: usize) -> Option<String> {
    if encoded_len(text) <= budget {
        return Some(text.to_owned());
    }
    let room = budget.checked_sub(encoded_len(SUMMARY_CUT))?;
    // The longest beginning whose encoding fits: encoding never shrinks text, so start
    // from `room` bytes and search down.
    let boundaries: Vec<usize> = text.char_indices().map(|(at, _)| at).chain([text.len()]).take_while(|at| *at <= room).collect();
    let fits = boundaries.partition_point(|at| encoded_len(&text[..*at]) <= room);
    let end = boundaries[..fits].last().copied().unwrap_or(0);
    Some(format!("{}{SUMMARY_CUT}", &text[..end]))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Actions: step summaries.
    #[test]
    fn summaries_are_masked_cut_and_capped() {
        let dir = tempfile::tempdir().unwrap();
        let reporter = Reporter::new(TaskId(1), dir.path(), 8).unwrap();
        reporter.add_mask("s3cr3t-value");
        reporter.set_summary(0, "# Deployed\nwith s3cr3t-value\n");
        reporter.set_summary(1, "   \n");
        let state = reporter.snapshot(None);
        assert_eq!(state.summaries, BTreeMap::from([(0, "# Deployed\nwith ***\n".to_owned())]), "masked; a blank one is nothing");

        // Too long for a step: cut with a note, on a character boundary, within the limit as encoded.
        reporter.set_summary(2, &"é\"".repeat(MAX_STEP_SUMMARY_BYTES));
        let cut = &reporter.snapshot(None).summaries[&2];
        assert!(cut.ends_with(SUMMARY_CUT) && encoded_len(cut) <= MAX_STEP_SUMMARY_BYTES && encoded_len(cut) > MAX_STEP_SUMMARY_BYTES - 8);
        assert!(reporter.lines().iter().any(|line| line.contains("was cut")));

        // The task's budget: later summaries are dropped once it is spent.
        for index in 3..8 {
            reporter.set_summary(index, &"x".repeat(MAX_STEP_SUMMARY_BYTES));
        }
        let last = reporter.snapshot(Some(TaskResult::Success));
        last.validate().unwrap();
        let total: usize = last.summaries.values().map(|summary| encoded_len(summary)).sum();
        assert!(total <= MAX_TASK_SUMMARY_BYTES, "{total}");
        assert!(!last.summaries.contains_key(&7) && reporter.lines().iter().any(|line| line.contains("dropped")));
        assert!(serde_json::to_vec(&last).unwrap().len() < rbit_workflow::protocol::Endpoint::TaskState.body_cap());

        // Once the server accepted them, they are not sent again; the budget still counts them.
        reporter.acknowledge(&last.summaries);
        assert!(reporter.snapshot(None).summaries.is_empty());
        reporter.set_summary(7, &"x".repeat(MAX_STEP_SUMMARY_BYTES));
        assert!(reporter.snapshot(None).summaries.is_empty(), "the budget stays spent");
    }

    #[test]
    fn fitting_keeps_what_fits() {
        assert_eq!(fit_summary("short", 10).as_deref(), Some("short"));
        assert_eq!(fit_summary("long enough", 3), None, "not even the note fits");
        let room = encoded_len(SUMMARY_CUT) + 4;
        let long = format!("abcd{}", "x".repeat(200));
        assert_eq!(fit_summary(&long, room).as_deref(), Some(format!("abcd{SUMMARY_CUT}").as_str()));
        // A control character takes six bytes encoded (`\u0001`): one fits in seven, two do not.
        let controls = "\u{1}".repeat(40);
        assert_eq!(fit_summary(&controls, encoded_len(SUMMARY_CUT) + 7).as_deref(), Some(format!("\u{1}{SUMMARY_CUT}").as_str()));
    }

    #[test]
    fn steps_record_their_log_ranges() {
        let dir = tempfile::tempdir().unwrap();
        let reporter = Reporter::new(TaskId(1), dir.path(), 3).unwrap();
        reporter.log("set up");
        reporter.start_step(0);
        reporter.log("one\ntwo");
        reporter.finish_step(0, StepStatus::Success);
        reporter.finish_step(1, StepStatus::Skipped);
        reporter.start_step(2);
        reporter.log("three");

        let state = reporter.snapshot(None);
        state.validate().unwrap();
        let ranges: Vec<_> = state.steps.iter().map(|step| (step.status, step.log_index, step.log_length)).collect();
        assert_eq!(ranges, [(StepStatus::Success, 1, 2), (StepStatus::Skipped, 3, 0), (StepStatus::Running, 3, 1)], "{:?}", state.steps);
        assert!(state.steps[1].started_at.is_none());

        // A final state never leaves a step running.
        let last = reporter.snapshot(Some(TaskResult::Cancelled));
        last.validate().unwrap();
        assert_eq!(last.steps[2].status, StepStatus::Cancelled);
    }
}
