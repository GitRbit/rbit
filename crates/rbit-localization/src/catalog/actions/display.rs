//! Borrowed Actions labels retain the ordinary plain-text escaping boundary.
use super::{ActionsMessage, Catalog, RERUN_PREFIX, SUMMARY_SUFFIX};
use crate::Locale;
use std::fmt;

/// Deferred typed Actions text. It holds borrowed arguments and has no trusted-HTML
/// marker or owned text buffer. Other locales write the catalog's localized text.
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
impl ActionsDisplay<'_> {
    /// The `lang` a template declares around this label, as [`crate::Localized::marked_lang`].
    pub fn marked_lang(&self) -> Option<&'static str> {
        match self.catalog.locale() {
            Locale::English => None,
            _ => self.catalog.localized(self.message).marked_lang(),
        }
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
        fmt::Display::fmt(&self.catalog.localized(self.message), output)
    }
}
