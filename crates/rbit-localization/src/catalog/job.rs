//! Compiled job/log labels. English static labels borrow; counts format lazily.
use super::{Catalog, LocalizedText};
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

impl Catalog {
    pub fn job(self, message: ActionsJobMessage) -> LocalizedText {
        let literal = match message {
            ActionsJobMessage::Details => "Job details",
            ActionsJobMessage::Steps => "Steps and logs",
            ActionsJobMessage::RunsOn => "runs on",
            ActionsJobMessage::Duration => "Duration",
            ActionsJobMessage::RawLog => "Raw log",
            ActionsJobMessage::Rerun => "Re-run job",
            ActionsJobMessage::NotStarted => "Not started yet",
            ActionsJobMessage::NotStartedDescription => "The steps and their logs appear here once a runner takes this job.",
            ActionsJobMessage::RetainedWindow => "Step previews are bounded. Open a step for more lines or download the raw log.",
            ActionsJobMessage::Expired => "This log was removed by the log retention.",
            ActionsJobMessage::NoOutput => "No output.",
            ActionsJobMessage::EarlierLines => "Earlier lines are left out.",
            ActionsJobMessage::ShowMore => "Show more of this step",
            ActionsJobMessage::Or => ", or",
            ActionsJobMessage::DownloadAll => "download the raw log for all of it.",
            ActionsJobMessage::Truncated => "The job wrote more than the instance keeps; later lines were dropped.",
            ActionsJobMessage::StepNotStarted => "This step has not started.",
            ActionsJobMessage::Success => "Success",
            ActionsJobMessage::Failure => "Failure",
            ActionsJobMessage::Cancelled => "Cancelled",
            ActionsJobMessage::Skipped => "Skipped",
            ActionsJobMessage::Queued => "Queued",
            ActionsJobMessage::Running => "In progress",
            ActionsJobMessage::Waiting => "Waiting",
            ActionsJobMessage::Approval => "Awaiting approval",
            ActionsJobMessage::WorkflowError => "Workflow error",
            ActionsJobMessage::StepCount { count } => {
                let mut output = count.to_string();
                self.literal(&mut output, if count == 1 { " step" } else { " steps" });
                return LocalizedText(output.into());
            }
        };
        self.literal_text(literal)
    }

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
impl fmt::Display for JobDisplay {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.catalog.locale() == Locale::English
            && let ActionsJobMessage::StepCount { count } = self.message
        {
            return write!(output, "{count} {}", if count == 1 { "step" } else { "steps" });
        }
        fmt::Display::fmt(&self.catalog.job(self.message), output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::borrow::Cow;

    #[test]
    fn static_english_is_borrowed_and_counts_match_the_compiled_catalog() {
        let english = Catalog::new(Locale::English);
        assert!(matches!(english.job(ActionsJobMessage::RawLog).0, Cow::Borrowed("Raw log")));
        for locale in [Locale::English, Locale::PseudoExpanded, Locale::PseudoRtl] {
            let catalog = Catalog::new(locale);
            for count in [0, 1, 2, 512, u64::MAX] {
                let message = ActionsJobMessage::StepCount { count };
                assert_eq!(catalog.job_display(message).to_string(), catalog.job(message).as_str());
            }
        }
        assert_eq!(ActionsJobMessage::Expired.id(), "actions.log.expired");
        assert_ne!(ActionsJobMessage::Expired.id(), ActionsJobMessage::Truncated.id());
    }
}
