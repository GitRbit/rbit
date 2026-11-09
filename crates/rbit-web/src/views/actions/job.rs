//! Bounded job and step log presentation; storage windows remain service-owned.
use super::{RunSummary, StateLook, elapsed};
use crate::views::when::Took;
use crate::{
    render::escape_html,
    views::{
        blocks::{Block, blocks, column_class, markup_columns},
        layout::Page,
        repo::{CodeLine, RepoHeader},
    },
};
use askama::Template;
use askama_web::WebTemplate;
use rbit_core::actions::Step;
use rbit_services::actions::logs::LogWindow;

// ------------------------------------------------------------------ the job page

/// Lines of one step shown on the job page; the step's log page shows up to
/// `rbit_services::actions::logs::STEP_VIEW_LINES`, the raw log everything.
pub const JOB_PAGE_STEP_LINES: usize = 1000;
/// Bytes of one step shown on the job page.
pub const JOB_PAGE_STEP_BYTES: usize = 256 * 1024;

/// `line` without ANSI escape sequences (colours, cursor moves, OSC titles and links):
/// logs are shown as text, never as terminal output.
pub fn strip_ansi(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        match chars.peek() {
            // CSI: parameters and intermediates, then one final byte in `@`..=`~`.
            Some('[') => {
                chars.next();
                for c in chars.by_ref() {
                    if ('@'..='~').contains(&c) {
                        break;
                    }
                }
            }
            // OSC: until BEL or ESC `\`.
            Some(']') => {
                chars.next();
                while let Some(c) = chars.next() {
                    if c == '\u{7}' || (c == '\u{1b}' && chars.next_if_eq(&'\\').is_some()) {
                        break;
                    }
                }
            }
            // Any other escape is two characters long.
            Some(_) => {
                chars.next();
            }
            None => {}
        }
    }
    out
}

/// One log line as HTML: escaped, without ANSI escapes, with the runner's workflow command
/// markers (`##[group]`, `##[error]` …, see rbit-runner's `commands`) as text and tone.
pub fn log_line_html(line: &str) -> String {
    let line = strip_ansi(line);
    let markers: [(&str, &str, &str); 6] = [
        ("##[group]", "font-semibold text-ink", ""),
        ("##[endgroup]", "text-ink-faint", ""),
        ("##[error]", "text-danger", "Error: "),
        ("##[warning]", "text-warning", "Warning: "),
        ("##[notice]", "text-ink", "Notice: "),
        ("##[debug]", "text-ink-faint", ""),
    ];
    for (marker, tone, label) in markers {
        if let Some(rest) = line.strip_prefix(marker) {
            return format!(r#"<span class="{tone}">{label}{}</span>"#, escape_html(rest));
        }
    }
    escape_html(&line)
}

/// A step's log lines, ready for `repo/code` blocks.
#[derive(Debug, Clone, Default)]
pub struct StepLog {
    /// Escaped HTML, one per line.
    pub lines: Vec<String>,
    /// The step-relative number of `lines[0]`, from 1.
    pub first: usize,
    /// Earlier lines were left out (see the step's log page or the raw log).
    pub more_before: bool,
    pub truncated: bool,
    pub expired: bool,
}

impl StepLog {
    /// `window` of a step whose log starts at task-log line `step_start`.
    pub fn new(window: &LogWindow, step_start: u64) -> Self {
        let first = usize::try_from(window.first_line.saturating_sub(step_start)).unwrap_or(0) + 1;
        Self {
            lines: window.lines.iter().map(|line| log_line_html(line)).collect(),
            first,
            more_before: window.more_before,
            truncated: window.truncated,
            expired: window.expired,
        }
    }

    pub fn blocks(&self) -> Vec<Block<CodeLine<'_>>> {
        blocks(self.lines.iter().enumerate().map(|(index, html)| CodeLine { number: self.first + index, html }))
    }

    pub fn columns(&self) -> String {
        column_class(self.lines.iter().map(|line| markup_columns(line)).max().unwrap_or(0))
    }
}

/// One step on the job page.
#[derive(Debug, Clone)]
pub struct StepView {
    pub number: i64,
    pub name: String,
    pub look: StateLook,
    pub duration: Option<Took>,
    /// Shown expanded: the running step, else the first that failed.
    pub open: bool,
    /// `None` for a step that never started.
    pub log: Option<StepLog>,
    /// The step's own log page.
    pub log_url: String,
}

impl StepView {
    pub fn new(step: &Step, log: Option<StepLog>, log_url: String) -> Self {
        Self {
            number: step.number,
            name: step.name.clone(),
            look: StateLook::of(step.status),
            duration: elapsed(step.started_at, step.stopped_at),
            open: false,
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
    /// Not finished: the page refreshes itself.
    pub active: bool,
    pub self_url: String,
    pub steps: Vec<StepView>,
    /// `None` until a runner took the job.
    pub raw_url: Option<String>,
    pub rerun_url: Option<String>,
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
        super::JobText::new(self.page.locale, self.steps.len())
    }
}

impl StepLogPage {
    pub fn labels(&self) -> super::JobText {
        super::JobText::new(self.page.locale, 1)
    }
}
