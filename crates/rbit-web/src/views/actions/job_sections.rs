//! "Set up job" and "Complete job": the lines of a task log before its first step and after
//! its last ([`rbit_services::actions::logs::sections`]), shown like steps, as on GitHub.

use super::{StateLook, StepLog, StepView, elapsed};
use rbit_core::actions::{Job, Status, Step};
use rbit_services::actions::logs::{LogWindow, has_log, sections};

/// Which section a window is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Setup,
    Complete,
}

impl Section {
    /// The key in line anchors (`#step-setup-line-3`) and `data-step`.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Setup => "setup",
            Self::Complete => "complete",
        }
    }
}

/// The section whose log grows now, when no step's does, and where it starts: what the
/// live tail follows.
pub fn growing(job: &Job, steps: &[Step]) -> Option<(Section, u64)> {
    if job.status.is_done() || steps.iter().any(|step| step.status == Status::Running) {
        return None;
    }
    let sections = sections(steps, false);
    match (sections.setup, sections.complete) {
        (Some((start, None)), _) => Some((Section::Setup, start)),
        (_, Some(start)) => Some((Section::Complete, start)),
        _ => None,
    }
}

/// The section `section` of `job` as a step, from its `window`; `None` when it has nothing
/// to show.
pub fn section_view(section: Section, job: &Job, steps: &[Step], window: Option<LogWindow>) -> Option<StepView> {
    let window = window?;
    let growing = growing(job, steps).is_some_and(|(grows, _)| grows == section);
    if window.lines.is_empty() && !window.expired && !growing {
        return None;
    }
    let started_steps = || steps.iter().filter(|step| has_log(step));
    let (start, end) = match section {
        Section::Setup => (job.started_at, started_steps().filter_map(|step| step.started_at).min().or(job.finished_at)),
        Section::Complete => (started_steps().filter_map(|step| step.stopped_at).max(), job.finished_at),
    };
    let look = if growing {
        StateLook::of(Status::Running)
    } else if section == Section::Complete || started_steps().next().is_none() {
        StateLook::of(job.status)
    } else {
        StateLook::of(Status::Success)
    };
    let first = match (section, sections(steps, job.status.is_done())) {
        (Section::Setup, _) => 0,
        (Section::Complete, found) => found.complete.unwrap_or_default(),
    };
    let log = StepLog::new(&window, first, section.key(), growing);
    Some(StepView {
        number: -1,
        key: section.key().to_owned(),
        name: String::new(),
        look,
        duration: elapsed(start, end),
        open: false,
        log: Some(log),
        log_url: String::new(),
        started: if growing { start.map(rbit_core::Timestamp::seconds) } else { None },
        tail: growing,
    })
}
