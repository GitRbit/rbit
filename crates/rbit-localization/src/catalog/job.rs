//! Compiled job/log labels. English static labels borrow; counts format lazily.
use super::Catalog;
use super::fallback::{MessageFamily, MessageWriter};
use crate::Locale;
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActionsJobMessage {
    Details,
    Steps,
    RunsOn,
    Duration,
    RawLog,
    Rerun,
    NotStarted,
    NotStartedDescription,
    RetainedWindow,
    Expired,
    NoOutput,
    EarlierLines,
    ShowMore,
    Or,
    DownloadAll,
    Truncated,
    StepNotStarted,
    Success,
    Failure,
    Cancelled,
    Skipped,
    Queued,
    Running,
    Waiting,
    Approval,
    WorkflowError,
    StepCount { count: u64 },
}

impl ActionsJobMessage {
    pub const fn id(self) -> &'static str {
        match self {
            Self::Details => "actions.job.details",
            Self::Steps => "actions.job.steps",
            Self::RunsOn => "actions.job.runs-on",
            Self::Duration => "actions.job.duration",
            Self::RawLog => "actions.job.raw-log",
            Self::Rerun => "actions.job.rerun-control",
            Self::NotStarted => "actions.job.not-started",
            Self::NotStartedDescription => "actions.job.not-started-description",
            Self::RetainedWindow => "actions.job.retained-window",
            Self::Expired => "actions.log.expired",
            Self::NoOutput => "actions.log.no-output",
            Self::EarlierLines => "actions.log.earlier-lines",
            Self::ShowMore => "actions.log.show-more",
            Self::Or => "actions.log.or",
            Self::DownloadAll => "actions.log.download-all",
            Self::Truncated => "actions.log.truncated",
            Self::StepNotStarted => "actions.log.not-started",
            Self::Success => "actions.status.success",
            Self::Failure => "actions.status.failure",
            Self::Cancelled => "actions.status.cancelled",
            Self::Skipped => "actions.status.skipped",
            Self::Queued => "actions.status.queued",
            Self::Running => "actions.status.running",
            Self::Waiting => "actions.status.waiting",
            Self::Approval => "actions.status.approval",
            Self::WorkflowError => "actions.status.workflow-error",
            Self::StepCount { .. } => "actions.job.step-count",
        }
    }
}

impl ActionsJobMessage {
    /// The English text of a fixed label.
    const fn english_label(self) -> Option<&'static str> {
        Some(match self {
            Self::Details => "Job details",
            Self::Steps => "Steps and logs",
            Self::RunsOn => "runs on",
            Self::Duration => "Duration",
            Self::RawLog => "Raw log",
            Self::Rerun => "Re-run job",
            Self::NotStarted => "Not started yet",
            Self::NotStartedDescription => "The steps and their logs appear here once a runner takes this job.",
            Self::RetainedWindow => "Step previews are bounded. Open a step for more lines or download the raw log.",
            Self::Expired => "This log was removed by the log retention.",
            Self::NoOutput => "No output.",
            Self::EarlierLines => "Earlier lines are left out.",
            Self::ShowMore => "Show more of this step",
            Self::Or => ", or",
            Self::DownloadAll => "download the raw log for all of it.",
            Self::Truncated => "The job wrote more than the instance keeps; later lines were dropped.",
            Self::StepNotStarted => "This step has not started.",
            Self::Success => "Success",
            Self::Failure => "Failure",
            Self::Cancelled => "Cancelled",
            Self::Skipped => "Skipped",
            Self::Queued => "Queued",
            Self::Running => "In progress",
            Self::Waiting => "Waiting",
            Self::Approval => "Awaiting approval",
            Self::WorkflowError => "Workflow error",
            Self::StepCount { .. } => return None,
        })
    }
}

impl MessageFamily for ActionsJobMessage {
    fn id(self) -> &'static str {
        Self::id(self)
    }

    fn english(self, text: &mut MessageWriter) {
        match self {
            Self::StepCount { count } => {
                text.number(count);
                text.literal(if count == 1 { " step" } else { " steps" });
            }
            _ => text.literal(self.english_label().unwrap_or_default()),
        }
    }

    fn english_literal(self) -> Option<&'static str> {
        self.english_label()
    }

    fn translate(self, locale: Locale, text: &mut MessageWriter) -> bool {
        crate::catalog::translations::write(self, locale, text)
    }
}

impl Catalog {
    pub const fn job_display(self, message: ActionsJobMessage) -> JobDisplay {
        JobDisplay { catalog: self, message }
    }
}

/// A typed lazy label, never trusted HTML or a stored per-row string.
#[derive(Clone, Copy, Debug)]
pub struct JobDisplay {
    catalog: Catalog,
    message: ActionsJobMessage,
}
impl JobDisplay {
    /// The `lang` a template declares around this label, as [`super::Localized::marked_lang`].
    pub fn marked_lang(&self) -> Option<&'static str> {
        match self.catalog.locale() {
            Locale::English => None,
            _ => self.catalog.localized(self.message).marked_lang(),
        }
    }
}
impl fmt::Display for JobDisplay {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.catalog.localized(self.message), output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_english_is_borrowed_and_counts_match_the_compiled_catalog() {
        let english = Catalog::new(Locale::English);
        assert!(english.localized(ActionsJobMessage::RawLog).borrows_static());
        for locale in [Locale::English, Locale::Spanish, Locale::PseudoExpanded, Locale::PseudoRtl] {
            let catalog = Catalog::new(locale);
            for count in [0, 1, 2, 512, u64::MAX] {
                let message = ActionsJobMessage::StepCount { count };
                assert_eq!(catalog.job_display(message).to_string(), catalog.localized(message).as_str());
            }
        }
        assert_eq!(ActionsJobMessage::Expired.id(), "actions.log.expired");
        assert_ne!(ActionsJobMessage::Expired.id(), ActionsJobMessage::Truncated.id());
    }
}
