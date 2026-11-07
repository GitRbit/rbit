//! Borrowed Actions labels retain the ordinary plain-text escaping boundary.
use super::{ActionsMessage, Catalog, RERUN_PREFIX, SUMMARY_SUFFIX};
use crate::Locale;
use std::fmt;

/// Deferred typed Actions text. It holds borrowed arguments and has no trusted-HTML
/// marker or owned text buffer. Diagnostic catalogs retain `Catalog::actions` output.
#[derive(Clone, Copy, Debug)]
pub struct ActionsDisplay<'a> {
    catalog: Catalog,
    message: ActionsMessage<'a>,
}
impl Catalog {
    /// English dynamic labels write directly into the caller's destination;
    /// other messages/locales retain the existing compiled catalog semantics.
    pub const fn actions_display(self, message: ActionsMessage<'_>) -> ActionsDisplay<'_> {
        ActionsDisplay { catalog: self, message }
    }
}
impl fmt::Display for ActionsDisplay<'_> {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.catalog.locale() == Locale::English {
            match self.message {
                ActionsMessage::RerunJob { name } => return write!(output, "{RERUN_PREFIX}{name}"),
                ActionsMessage::Summary { job } => return write!(output, "{job}{SUMMARY_SUFFIX}"),
                _ => {}
            }
        }
        if self.catalog.locale() == Locale::Spanish {
            use crate::catalog::spanish::{RERUN_PREFIX, SUMMARY_PREFIX};
            match self.message {
                ActionsMessage::RerunJob { name } => return write!(output, "{RERUN_PREFIX}{name}"),
                ActionsMessage::Summary { job } => return write!(output, "{SUMMARY_PREFIX}{job}"),
                _ => {}
            }
        }
        fmt::Display::fmt(&self.catalog.actions(self.message), output)
    }
}
