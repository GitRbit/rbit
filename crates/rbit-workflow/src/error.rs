//! Why a workflow produced no run: a message for the workflow's author, the place in the
//! file it refers to, and a coarse kind for metrics.

use std::fmt;

/// A position in a workflow file, 1-based as editors show it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Location {
    pub line: u32,
    pub column: u32,
}

impl Location {
    pub fn new(line: u64, column: u64) -> Self {
        Self { line: u32::try_from(line).unwrap_or(u32::MAX), column: u32::try_from(column).unwrap_or(u32::MAX) }
    }
}

impl fmt::Display for Location {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}, column {}", self.line, self.column)
    }
}

/// The `reason` label of `rbit_actions_workflow_errors_total`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorKind {
    /// Not YAML, or not a workflow.
    Parse,
    /// Valid, but over one of the limits (size, jobs, matrix combinations).
    Limit,
    /// The jobs cannot be planned (unknown `needs`, an expression rbit cannot evaluate).
    Plan,
    /// Evaluation ran out of time.
    Timeout,
}

impl ErrorKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Parse => "parse",
            Self::Limit => "limit",
            Self::Plan => "plan",
            Self::Timeout => "timeout",
        }
    }
}

/// Longest message kept, in bytes; the run's stored error is capped at 2 KiB with room for
/// the file name and location.
const MAX_MESSAGE_BYTES: usize = 1536;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowError {
    pub kind: ErrorKind,
    pub message: String,
    pub at: Option<Location>,
}

impl WorkflowError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        let mut message = message.into();
        if message.len() > MAX_MESSAGE_BYTES {
            let mut end = MAX_MESSAGE_BYTES;
            while !message.is_char_boundary(end) {
                end -= 1;
            }
            message.truncate(end);
            message.push('…');
        }
        Self { kind, message, at: None }
    }

    pub fn parse(message: impl Into<String>, at: Location) -> Self {
        Self::new(ErrorKind::Parse, message).at(at)
    }

    pub fn limit(message: impl Into<String>, at: Option<Location>) -> Self {
        Self::new(ErrorKind::Limit, message).at_location(at)
    }

    pub fn plan(message: impl Into<String>, at: Option<Location>) -> Self {
        Self::new(ErrorKind::Plan, message).at_location(at)
    }

    #[must_use]
    pub fn at(self, at: Location) -> Self {
        Self { at: Some(at), ..self }
    }

    #[must_use]
    pub fn at_location(self, at: Option<Location>) -> Self {
        Self { at, ..self }
    }
}

impl fmt::Display for WorkflowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.at {
            Some(at) => write!(f, "{at}: {}", self.message),
            None => f.write_str(&self.message),
        }
    }
}

impl std::error::Error for WorkflowError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_show_their_location_and_stay_short() {
        let error = WorkflowError::parse("jobs must be a mapping", Location::new(3, 1));
        assert_eq!(error.to_string(), "line 3, column 1: jobs must be a mapping");
        assert_eq!(WorkflowError::plan("cycle", None).to_string(), "cycle");
        let long = WorkflowError::new(ErrorKind::Limit, "é".repeat(2000));
        assert!(long.message.len() <= MAX_MESSAGE_BYTES + '…'.len_utf8() && long.message.ends_with('…'));
        assert_eq!(Location::new(u64::MAX, 2), Location { line: u32::MAX, column: 2 });
        let kinds = [ErrorKind::Parse, ErrorKind::Limit, ErrorKind::Plan, ErrorKind::Timeout].map(ErrorKind::as_str);
        assert_eq!(kinds, ["parse", "limit", "plan", "timeout"]);
    }
}
