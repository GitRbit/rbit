//! A commit's checks, as listed on the commit page and in the merge box.
use super::super::fallback::{MessageFamily, MessageWriter};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChecksMessage {
    /// The one-line summary: how many reported checks succeeded, fail and are pending,
    /// and how many required ones have not reported yet.
    Summary {
        successful: u64,
        failing: u64,
        pending: u64,
        expected: u64,
    },
    Success,
    Failure,
    Error,
    Pending,
    /// A required check that has not reported.
    Expected,
    ExpectedDescription,
    Required,
    Details,
}

impl MessageFamily for ChecksMessage {
    fn id(self) -> &'static str {
        match self {
            Self::Summary { .. } => "checks.summary",
            Self::Success => "checks.state.success",
            Self::Failure => "checks.state.failure",
            Self::Error => "checks.state.error",
            Self::Pending => "checks.state.pending",
            Self::Expected => "checks.state.expected",
            Self::ExpectedDescription => "checks.expected-description",
            Self::Required => "checks.required",
            Self::Details => "checks.details",
        }
    }

    fn english(self, text: &mut MessageWriter) {
        let Self::Summary { successful, failing, pending, expected } = self else {
            text.literal(match self {
                Self::Success => "Success",
                Self::Failure => "Failure",
                Self::Error => "Error",
                Self::Pending => "Pending",
                Self::Expected => "Expected",
                Self::ExpectedDescription => "Waiting for this status to be reported",
                Self::Required => "Required",
                _ => "Details",
            });
            return;
        };
        let parts: Vec<(u64, &str)> = [(successful, " successful"), (failing, " failing"), (pending, " pending")]
            .into_iter()
            .filter(|(count, _)| *count > 0)
            .collect();
        if parts.is_empty() {
            text.literal("No checks");
        }
        for (index, (count, word)) in parts.iter().enumerate() {
            if index > 0 {
                text.literal(if index + 1 == parts.len() { " and " } else { ", " });
            }
            text.number(*count);
            text.literal(word);
        }
        if !parts.is_empty() {
            text.literal(if successful + failing + pending == 1 { " check" } else { " checks" });
        }
        if expected > 0 {
            text.literal(", ");
            text.number(expected);
            text.literal(" expected");
        }
    }

    fn translate(self, locale: crate::locale::Locale, text: &mut MessageWriter) -> bool {
        crate::catalog::translations::write(self, locale, text)
    }
}
