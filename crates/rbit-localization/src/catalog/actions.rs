//! Typed text for the Actions run presentation; values are plain Unicode text.
use super::{Catalog, LocalizedText};
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
}

impl Catalog {
    /// Uses the same compiled locale and argument-isolation rules as other messages.
    pub fn actions(self, message: ActionsMessage<'_>) -> LocalizedText {
        if self.locale == crate::Locale::Spanish {
            return super::spanish::actions(self, message);
        }
        let literal = match message {
            ActionsMessage::Actions => "Actions",
            ActionsMessage::Workflow => "Workflow",
            ActionsMessage::Commit => "Commit",
            ActionsMessage::Duration => "Duration",
            ActionsMessage::Attempt => "Attempt",
            ActionsMessage::TriggeredBy => "Triggered by",
            ActionsMessage::Jobs => "Jobs",
            ActionsMessage::Artifacts => "Artifacts",
            ActionsMessage::RunDetails => "Run details",
            ActionsMessage::CancelRun => "Cancel run",
            ActionsMessage::RerunFailed => "Re-run failed",
            ActionsMessage::RerunAll => "Re-run all",
            ActionsMessage::DownloadTar => "Download as tar.gz",
            ActionsMessage::Expires => "expires",
            ActionsMessage::RerunJob { name } => {
                let mut output = String::new();
                self.literal(&mut output, RERUN_PREFIX);
                self.argument(&mut output, name);
                return LocalizedText(output.into());
            }
            ActionsMessage::Summary { job } => {
                let mut output = String::new();
                self.argument(&mut output, job);
                self.literal(&mut output, SUMMARY_SUFFIX);
                return LocalizedText(output.into());
            }
        };
        self.literal_text(literal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Locale;

    #[test]
    fn messages_have_stable_ids_and_dynamic_values_remain_text() {
        let attack = "<img src=x onerror=alert(1)>";
        for locale in [Locale::English, Locale::PseudoExpanded, Locale::PseudoRtl] {
            let catalog = Catalog::new(locale);
            let label = catalog.actions(ActionsMessage::RerunJob { name: attack });
            assert!(label.as_str().contains(attack));
            if locale == Locale::PseudoRtl {
                assert!(label.as_str().contains(&format!("\u{2068}{attack}\u{2069}")));
            }
            assert_eq!(ActionsMessage::RerunJob { name: attack }.id(), "actions.job.rerun");
            assert!(catalog.actions(ActionsMessage::Summary { job: "build" }).as_str().contains("build"));
        }
        assert_eq!(Catalog::new(Locale::English).actions(ActionsMessage::RunDetails).as_str(), "Run details");
    }
}
