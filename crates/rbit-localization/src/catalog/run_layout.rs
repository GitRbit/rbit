//! The run layout shared by the run and job pages: the run's navigation beside them.
use super::fallback::{MessageFamily, MessageWriter};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RunLayoutMessage<'a> {
    /// The link to the run's summary page, first in its navigation.
    Summary,
    /// The accessible name of the run's navigation.
    Navigation,
    /// How long the run's jobs took together.
    Usage,
    /// The link to the workflow file at the run's commit.
    WorkflowFile,
    /// How many jobs a matrix made.
    JobCount {
        count: u64,
    },
    /// The accessible name of the run's job graph.
    Graph,
    /// "+4 more": the jobs of a matrix its box does not list.
    MoreJobs {
        count: u64,
    },
    /// The heading of the run's annotations.
    Annotations,
    /// "2 errors", "1 warning", "3 notices": how many of each a run's jobs reported.
    Errors {
        count: u64,
    },
    Warnings {
        count: u64,
    },
    Notices {
        count: u64,
    },
    /// An annotation's level, for assistive technology.
    ErrorLevel,
    WarningLevel,
    NoticeLevel,
    /// A failed step that reported no error of its own; `step` is its name.
    StepFailed {
        step: &'a str,
    },
    /// Annotations the run page does not list.
    MoreAnnotations {
        count: u64,
    },
}

impl MessageFamily for RunLayoutMessage<'_> {
    fn id(self) -> &'static str {
        match self {
            Self::Summary => "actions.run.layout.summary",
            Self::Navigation => "actions.run.layout.navigation",
            Self::Usage => "actions.run.layout.usage",
            Self::WorkflowFile => "actions.run.layout.workflow-file",
            Self::JobCount { .. } => "actions.run.layout.job-count",
            Self::Graph => "actions.run.layout.graph",
            Self::MoreJobs { .. } => "actions.run.layout.more-jobs",
            Self::Annotations => "actions.run.layout.annotations",
            Self::Errors { .. } => "actions.run.layout.errors",
            Self::Warnings { .. } => "actions.run.layout.warnings",
            Self::Notices { .. } => "actions.run.layout.notices",
            Self::ErrorLevel => "actions.run.layout.error-level",
            Self::WarningLevel => "actions.run.layout.warning-level",
            Self::NoticeLevel => "actions.run.layout.notice-level",
            Self::StepFailed { .. } => "actions.run.layout.step-failed",
            Self::MoreAnnotations { .. } => "actions.run.layout.more-annotations",
        }
    }

    fn english(self, text: &mut MessageWriter) {
        match self {
            Self::Summary => text.literal("Summary"),
            Self::Navigation => text.literal("Run navigation"),
            Self::Usage => text.literal("Usage"),
            Self::WorkflowFile => text.literal("Workflow file"),
            Self::JobCount { count } => {
                text.number(count);
                text.literal(if count == 1 { " job" } else { " jobs" });
            }
            Self::Graph => text.literal("Job graph"),
            Self::MoreJobs { count } => {
                text.literal("+");
                text.number(count);
                text.literal(" more");
            }
            Self::Annotations => text.literal("Annotations"),
            Self::Errors { count } => counted(text, count, " error", " errors"),
            Self::Warnings { count } => counted(text, count, " warning", " warnings"),
            Self::Notices { count } => counted(text, count, " notice", " notices"),
            Self::ErrorLevel => text.literal("Error"),
            Self::WarningLevel => text.literal("Warning"),
            Self::NoticeLevel => text.literal("Notice"),
            Self::StepFailed { step } => {
                text.literal("The step ");
                text.argument(step);
                text.literal(" failed.");
            }
            Self::MoreAnnotations { count } => {
                text.literal("And ");
                text.number(count);
                text.literal(" more in the jobs' logs.");
            }
        }
    }

    fn translate(self, locale: crate::locale::Locale, text: &mut MessageWriter) -> bool {
        crate::catalog::translations::write(self, locale, text)
    }
}

/// `count` and its noun in English.
fn counted(text: &mut MessageWriter, count: u64, one: &str, other: &str) {
    text.number(count);
    text.literal(if count == 1 { one } else { other });
}
