//! Typed text for the Actions run presentation; values are plain Unicode text.
use super::Catalog;
use super::fallback::{MessageFamily, MessageWriter};
use crate::Locale;
mod display;
pub use display::ActionsDisplay;
const RERUN_PREFIX: &str = "Re-run ";
const SUMMARY_SUFFIX: &str = " summary";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActionsMessage<'a> {
    Actions,
    Workflow,
    Commit,
    Duration,
    Attempt,
    TriggeredBy,
    Jobs,
    Artifacts,
    RunDetails,
    CancelRun,
    RerunFailed,
    RerunAll,
    DownloadTar,
    Expires,
    RerunJob { name: &'a str },
    Summary { job: &'a str },
}

impl ActionsMessage<'_> {
    pub const fn id(self) -> &'static str {
        match self {
            Self::Actions => "actions.title",
            Self::Workflow => "actions.run.workflow",
            Self::Commit => "actions.run.commit",
            Self::Duration => "actions.run.duration",
            Self::Attempt => "actions.run.attempt",
            Self::TriggeredBy => "actions.run.triggered-by",
            Self::Jobs => "actions.run.jobs",
            Self::Artifacts => "actions.run.artifacts",
            Self::RunDetails => "actions.run.details",
            Self::CancelRun => "actions.run.cancel",
            Self::RerunFailed => "actions.run.rerun-failed",
            Self::RerunAll => "actions.run.rerun-all",
            Self::DownloadTar => "actions.run.download-tar",
            Self::Expires => "actions.run.expires",
            Self::RerunJob { .. } => "actions.job.rerun",
            Self::Summary { .. } => "actions.job.summary",
        }
    }

    /// The English text of a fixed label.
    const fn english_label(self) -> Option<&'static str> {
        Some(match self {
            Self::Actions => "Actions",
            Self::Workflow => "Workflow",
            Self::Commit => "Commit",
            Self::Duration => "Duration",
            Self::Attempt => "Attempt",
            Self::TriggeredBy => "Triggered by",
            Self::Jobs => "Jobs",
            Self::Artifacts => "Artifacts",
            Self::RunDetails => "Run details",
            Self::CancelRun => "Cancel run",
            Self::RerunFailed => "Re-run failed",
            Self::RerunAll => "Re-run all",
            Self::DownloadTar => "Download as tar.gz",
            Self::Expires => "expires",
            Self::RerunJob { .. } | Self::Summary { .. } => return None,
        })
    }
}

impl MessageFamily for ActionsMessage<'_> {
    fn id(self) -> &'static str {
        Self::id(self)
    }

    fn english(self, text: &mut MessageWriter) {
        match self {
            Self::RerunJob { name } => {
                text.literal(RERUN_PREFIX);
                text.argument(name);
            }
            Self::Summary { job } => {
                text.argument(job);
                text.literal(SUMMARY_SUFFIX);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_have_stable_ids_and_dynamic_values_remain_text() {
        let attack = "<img src=x onerror=alert(1)>";
        for locale in [Locale::English, Locale::PseudoExpanded, Locale::PseudoRtl] {
            let catalog = Catalog::new(locale);
            let label = catalog.localized(ActionsMessage::RerunJob { name: attack });
            assert!(label.as_str().contains(attack));
            if locale == Locale::PseudoRtl {
                assert!(label.as_str().contains(&format!("\u{2068}{attack}\u{2069}")));
            }
            assert_eq!(ActionsMessage::RerunJob { name: attack }.id(), "actions.job.rerun");
            assert!(catalog.localized(ActionsMessage::Summary { job: "build" }).as_str().contains("build"));
        }
        assert_eq!(Catalog::new(Locale::English).localized(ActionsMessage::RunDetails).as_str(), "Run details");
    }
}
