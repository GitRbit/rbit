//! The run layout shared by the run and job pages: the run's navigation beside them.
use super::fallback::{MessageFamily, MessageWriter};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RunLayoutMessage {
    /// The link to the run's summary page, first in its navigation.
    Summary,
    /// The accessible name of the run's navigation.
    Navigation,
    /// How long the run's jobs took together.
    Usage,
    /// The link to the workflow file at the run's commit.
    WorkflowFile,
    /// How many jobs a matrix made.
    JobCount { count: u64 },
}

impl MessageFamily for RunLayoutMessage {
    fn id(self) -> &'static str {
        match self {
            Self::Summary => "actions.run.layout.summary",
            Self::Navigation => "actions.run.layout.navigation",
            Self::Usage => "actions.run.layout.usage",
            Self::WorkflowFile => "actions.run.layout.workflow-file",
            Self::JobCount { .. } => "actions.run.layout.job-count",
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
        }
    }

    fn translate(self, locale: crate::locale::Locale, text: &mut MessageWriter) -> bool {
        crate::catalog::translations::write(self, locale, text)
    }
}
