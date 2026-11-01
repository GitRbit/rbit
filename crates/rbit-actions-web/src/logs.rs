//! Passive, borrowed rendering of host-authorized log windows.
use askama::Template;
use rbit_ui::blocks::{Block, blocks, column_class, markup_columns};

/// Host-projected log lines; every line is already escaped by the trusted host's
/// ANSI/workflow-marker formatter. This compiled first-party contract does not
/// sanitize or attest arbitrary external HTML, or authorize/fetch task logs.
/// Runtime windows and limits remain host-owned.
#[derive(Debug, Clone, Default)]
pub struct StepLog {
    pub lines: Vec<String>,
    pub first: usize,
    pub more_before: bool,
    pub truncated: bool,
    pub expired: bool,
}
/// One borrowed line for the shared code-row layout.
#[derive(Debug, Clone)]
pub struct LogLine<'a> {
    pub number: usize,
    pub html: &'a str,
}
impl StepLog {
    pub fn blocks(&self) -> Vec<Block<LogLine<'_>>> {
        blocks(self.lines.iter().enumerate().map(|(index, html)| LogLine { number: self.first + index, html }))
    }
    pub fn columns(&self) -> String {
        column_class(self.lines.iter().map(|line| markup_columns(line)).max().unwrap_or(0))
    }
}
/// The same component renders inside both job preview and standalone step shells.
/// It borrows existing buffers and URLs; no intermediary HTML String is rendered.
#[derive(Debug, Template)]
#[template(path = "actions/step-body.html")]
pub struct StepLogBody<'a> {
    pub log: Option<&'a StepLog>,
    pub log_url: &'a str,
}
impl askama::filters::HtmlSafe for StepLogBody<'_> {}

#[cfg(test)]
mod tests;
