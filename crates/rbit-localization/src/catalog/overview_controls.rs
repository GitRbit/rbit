//! Typed overview controls: display text is separate from machine filter/form values.
use super::Catalog;
use super::fallback::{MessageFamily, MessageWriter};
use crate::Locale;
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OverviewControlMessage {
    Push,
    PullRequest,
    Manual,
    Schedule,
    WorkflowRun,
    Dispatch,
    Success,
    Failure,
    Cancelled,
    Skipped,
    Queued,
    Running,
    Waiting,
    Approval,
    RunWorkflow,
    WorkflowFrom,
    Change,
}
impl OverviewControlMessage {
    /// Catalog identifier, independent of the text.
    pub const fn id(self) -> &'static str {
        match self {
            Self::Push => "actions.control.push",
            Self::PullRequest => "actions.control.pull-request",
            Self::Manual => "actions.control.manual",
            Self::Schedule => "actions.control.schedule",
            Self::WorkflowRun => "actions.control.workflow-run",
            Self::Dispatch => "actions.control.dispatch",
            Self::Success => "actions.control.success",
            Self::Failure => "actions.control.failure",
            Self::Cancelled => "actions.control.cancelled",
            Self::Skipped => "actions.control.skipped",
            Self::Queued => "actions.control.queued",
            Self::Running => "actions.control.running",
            Self::Waiting => "actions.control.waiting",
            Self::Approval => "actions.control.approval",
            Self::RunWorkflow => "actions.control.run-workflow",
            Self::WorkflowFrom => "actions.control.workflow-from",
            Self::Change => "actions.control.change",
        }
    }

    const fn english_label(self) -> &'static str {
        match self {
            Self::Push => "push",
            Self::PullRequest => "pull request",
            Self::Manual => "manual",
            Self::Schedule => "schedule",
            Self::WorkflowRun => "after a workflow",
            Self::Dispatch => "dispatch",
            Self::Success => "Success",
            Self::Failure => "Failure",
            Self::Cancelled => "Cancelled",
            Self::Skipped => "Skipped",
            Self::Queued => "Queued",
            Self::Running => "In progress",
            Self::Waiting => "Waiting",
            Self::Approval => "Awaiting approval",
            Self::RunWorkflow => "Run workflow",
            Self::WorkflowFrom => "Use the workflow from",
            Self::Change => "Change",
        }
    }
}

impl MessageFamily for OverviewControlMessage {
    fn id(self) -> &'static str {
        Self::id(self)
    }

    fn english(self, text: &mut MessageWriter) {
        text.literal(self.english_label());
    }

    fn english_literal(self) -> Option<&'static str> {
        Some(self.english_label())
    }

    fn translate(self, locale: Locale, text: &mut MessageWriter) -> bool {
        crate::catalog::translations::write(self, locale, text)
    }
}

/// Deferred plain text, with no trusted HTML or per-option owned text buffer.
#[derive(Clone, Copy, Debug)]
pub struct OverviewControlDisplay {
    catalog: Catalog,
    message: OverviewControlMessage,
}
impl Catalog {
    pub const fn overview_control(self, message: OverviewControlMessage) -> OverviewControlDisplay {
        OverviewControlDisplay { catalog: self, message }
    }
}
impl OverviewControlDisplay {
    /// The `lang` a template declares around this label, as [`super::Localized::marked_lang`].
    pub fn marked_lang(&self) -> Option<&'static str> {
        match self.catalog.locale() {
            Locale::English => None,
            _ => self.catalog.localized(self.message).marked_lang(),
        }
    }
}
impl fmt::Display for OverviewControlDisplay {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.catalog.locale() {
            Locale::English => output.write_str(self.message.english_label()),
            _ => fmt::Display::fmt(&self.catalog.localized(self.message), output),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fmt::Write;

    struct BorrowedLiteral {
        expected: &'static str,
        writes: usize,
    }
    impl Write for BorrowedLiteral {
        fn write_str(&mut self, text: &str) -> fmt::Result {
            assert!(std::ptr::eq(text, self.expected), "real catalog labels must write their static literal directly");
            self.writes += 1;
            Ok(())
        }
    }
    #[test]
    fn real_controls_write_static_literals_and_diagnostics_retain_existing_catalog_transform() {
        let messages = [
            OverviewControlMessage::Push,
            OverviewControlMessage::PullRequest,
            OverviewControlMessage::Manual,
            OverviewControlMessage::Schedule,
            OverviewControlMessage::WorkflowRun,
            OverviewControlMessage::Dispatch,
            OverviewControlMessage::Success,
            OverviewControlMessage::Failure,
            OverviewControlMessage::Cancelled,
            OverviewControlMessage::Skipped,
            OverviewControlMessage::Queued,
            OverviewControlMessage::Running,
            OverviewControlMessage::Waiting,
            OverviewControlMessage::Approval,
            OverviewControlMessage::RunWorkflow,
            OverviewControlMessage::WorkflowFrom,
            OverviewControlMessage::Change,
        ];
        for message in messages {
            let mut output = BorrowedLiteral { expected: message.english_label(), writes: 0 };
            write!(output, "{}", Catalog::new(Locale::English).overview_control(message)).unwrap();
            assert_eq!(output.writes, 1);
            let spanish = Catalog::new(Locale::Spanish);
            assert_eq!(spanish.overview_control(message).to_string(), spanish.localized(message).as_str());
            assert!(!spanish.localized(message).fallback(), "{}", message.id());
            for locale in [Locale::PseudoExpanded, Locale::PseudoRtl] {
                let catalog = Catalog::new(locale);
                assert_eq!(catalog.overview_control(message).to_string(), catalog.localized(message).as_str());
            }
        }
    }
}
