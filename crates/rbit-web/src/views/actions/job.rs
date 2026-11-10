//! Bounded job and step log presentation; storage windows remain service-owned.
use super::log_lines::Console;
use super::{RunSummary, StateLook, elapsed};
use crate::views::when::{Took, When};
use crate::views::{layout::Page, repo::RepoHeader};
use askama::Template;
use askama_web::WebTemplate;
use rbit_core::actions::{Status, Step};
use rbit_services::actions::logs::LogWindow;

// ------------------------------------------------------------------ the job page

/// Lines of one step shown on the job page; the step's log page shows up to
/// `rbit_services::actions::logs::STEP_VIEW_LINES`, the raw log everything.
pub const JOB_PAGE_STEP_LINES: usize = 1000;
/// Bytes of one step shown on the job page.
pub const JOB_PAGE_STEP_BYTES: usize = 256 * 1024;

/// A step's log window, rendered as the log console (see [`super::log_lines`]).
#[derive(Debug, Clone, Default)]
#[expect(clippy::struct_excessive_bools, reason = "independent facts a page shows")]
pub struct StepLog {
    /// Escaped rows and groups.
    pub html: String,
    /// The window holds no line.
    pub empty: bool,
    /// The step-relative number of the first line, from 1.
    pub first: usize,
    /// The task-log line after the window: where a live tail continues.
    pub next_line: u64,
    /// Earlier lines were left out (see the step's log page or the raw log).
    pub more_before: bool,
    pub truncated: bool,
    pub expired: bool,
    /// The last error line's text and step-relative number.
    pub last_error: Option<(String, usize)>,
}

impl StepLog {
    /// `window` of the step `key` (its number, `setup` or `complete`) whose log starts at
    /// task-log line `step_start`; `running` while lines may still come.
    pub fn new(window: &LogWindow, step_start: u64, key: &str, running: bool) -> Self {
        let first = usize::try_from(window.first_line.saturating_sub(step_start)).unwrap_or(0) + 1;
        let rendered = Console { step: key, first, running, live: false }.render(&window.lines);
        Self {
            html: rendered.html,
            empty: window.lines.is_empty(),
            first,
            next_line: window.next_line,
            more_before: window.more_before,
            truncated: window.truncated,
            expired: window.expired,
            last_error: rendered.last_error.zip(rendered.last_error_line),
        }
    }
}

/// One step on the job page.
#[derive(Debug, Clone)]
pub struct StepView {
    pub number: i64,
    /// The step in line anchors and `data-step`: its number.
    pub key: String,
    pub name: String,
    pub look: StateLook,
    pub duration: Option<Took>,
    /// Shown expanded: the running step, else the first that failed.
    pub open: bool,
    /// `None` for a step that never started.
    pub log: Option<StepLog>,
    /// The step's own log page.
    pub log_url: String,
    /// When a running step started (seconds since the epoch): its duration ticks.
    pub started: Option<i64>,
    /// The step whose log grows: the page appends its new lines (`…/logs/live`).
    pub tail: bool,
}

impl StepView {
    pub fn new(step: &Step, log: Option<StepLog>, log_url: String) -> Self {
        Self {
            number: step.number,
            key: step.number.to_string(),
            name: step.name.clone(),
            look: StateLook::of(step.status),
            duration: elapsed(step.started_at, step.stopped_at),
            open: false,
            started: (step.status == Status::Running).then(|| step.started_at.map(rbit_core::Timestamp::seconds)).flatten(),
            tail: step.status == Status::Running && log.is_some(),
            log,
            log_url,
        }
    }
}

/// Opens the running step, or else the first failed one.
pub fn open_one(steps: &mut [StepView]) {
    let running = steps.iter().position(|step| step.look.key == "running");
    let failed = steps.iter().position(|step| step.look.key == "failure");
    if let Some(index) = running.or(failed) {
        steps[index].open = true;
    }
}

#[derive(Debug, Template, WebTemplate)]
#[template(path = "actions/job.html")]
pub struct JobPage {
    pub page: Page,
    pub header: RepoHeader,
    pub run: RunSummary,
    pub run_url: String,
    pub name: String,
    pub look: StateLook,
    pub duration: Option<Took>,
    pub labels: String,
    /// Not finished: the page tails the log and reloads the job when a step changes.
    pub active: bool,
    pub self_url: String,
    /// The job's and steps' states (`routes::actions::job_live::live_state`).
    pub live_state: String,
    /// The server's clock, so running durations tick from the right start.
    pub now: i64,
    pub steps: Vec<StepView>,
    /// `None` until a runner took the job.
    pub raw_url: Option<String>,
    pub rerun_url: Option<String>,
    /// The name of the runner that took the job, unless it was removed.
    pub runner: Option<String>,
    pub attempt: i64,
    pub queued: Option<When>,
    pub started: Option<When>,
    /// When a running job started (seconds since the epoch): its duration ticks.
    pub ticking: Option<i64>,
    /// Where a failed job failed: the first failed step and its last error.
    pub failure: Option<Failure>,
}

/// The first failed step of a job and its last error line, linked.
#[derive(Debug, Clone)]
pub struct Failure {
    pub step: String,
    /// Log text, cut to [`Failure::MAX_CHARS`]; escaped like any other text.
    pub message: Option<String>,
    /// `#step-<key>-line-<n>`, or the step itself.
    pub anchor: String,
}

impl Failure {
    pub const MAX_CHARS: usize = 300;

    /// The first failed step of `steps`, if any.
    pub fn of(steps: &[StepView]) -> Option<Self> {
        let step = steps.iter().find(|step| step.look.key == "failure")?;
        let error = step.log.as_ref().and_then(|log| log.last_error.clone());
        let anchor = match &error {
            Some((_, line)) => format!("step-{}-line-{line}", step.key),
            None => format!("step-{}", step.key),
        };
        let message = error.map(|(text, _)| {
            let mut cut: String = text.chars().take(Self::MAX_CHARS).collect();
            if cut.len() < text.len() {
                cut.push('…');
            }
            cut
        });
        Some(Self { step: step.name.clone(), message, anchor })
    }
}

#[derive(Debug, Template, WebTemplate)]
#[template(path = "actions/step_log.html")]
pub struct StepLogPage {
    pub page: Page,
    pub header: RepoHeader,
    pub job_name: String,
    pub job_url: String,
    pub step: StepView,
    pub raw_url: String,
    pub active: bool,
    pub self_url: String,
}

impl JobPage {
    pub fn labels(&self) -> super::JobText {
        super::JobText::new(self.page.locale, self.steps.iter().filter(|step| step.number >= 0).count())
    }
}

impl StepLogPage {
    pub fn labels(&self) -> super::JobText {
        super::JobText::new(self.page.locale, 1)
    }
}
