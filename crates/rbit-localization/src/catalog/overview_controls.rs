//! Typed overview controls: display text is separate from machine filter/form values.
use super::Catalog;
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
    const fn english(self) -> &'static str {
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
    const fn spanish(self) -> &'static str {
        match self {
            Self::Push => "envío",
            Self::PullRequest => "solicitud de incorporación",
            Self::Manual => "manual",
            Self::Schedule => "programación",
            Self::WorkflowRun => "tras un flujo de trabajo",
            Self::Dispatch => "despacho",
            Self::Success => "Correcto",
            Self::Failure => "Error",
            Self::Cancelled => "Cancelado",
            Self::Skipped => "Omitido",
            Self::Queued => "En cola",
            Self::Running => "En curso",
            Self::Waiting => "En espera",
            Self::Approval => "Pendiente de aprobación",
            Self::RunWorkflow => "Ejecutar flujo",
            Self::WorkflowFrom => "Usar el flujo de",
            Self::Change => "Cambiar",
        }
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
impl fmt::Display for OverviewControlDisplay {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.catalog.locale() {
            Locale::English => output.write_str(self.message.english()),
            Locale::Spanish => output.write_str(self.message.spanish()),
            _ => fmt::Display::fmt(&self.catalog.literal_text(self.message.english()), output),
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
            for (locale, expected) in [(Locale::English, message.english()), (Locale::Spanish, message.spanish())] {
                let mut output = BorrowedLiteral { expected, writes: 0 };
                write!(output, "{}", Catalog::new(locale).overview_control(message)).unwrap();
                assert_eq!(output.writes, 1);
            }
            for locale in [Locale::PseudoExpanded, Locale::PseudoRtl] {
                let catalog = Catalog::new(locale);
                assert_eq!(catalog.overview_control(message).to_string(), catalog.literal_text(message.english()).as_str());
            }
        }
    }
}
