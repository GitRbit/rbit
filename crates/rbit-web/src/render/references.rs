//! Markdown for issues, pull requests and comments, with references linked: `#12` (when
//! it exists in this repository), `owner/repo#12` and `@name` (when the account exists).
//!
//! Milestone 2 workstream I (issues) owns this file (and, in wave B, `markdown.rs`).
//!
//! Hook (`docs/design/milestone-2b.md` §3): T0's timeline views, I and P render every
//! issue text through [`render_issue_markdown`], after resolving its references with
//! `ReferenceService::resolve`.

use rbit_services::references::ReferenceSet;

use crate::{render::render_markdown, urls::DocumentBase};

/// Where a text was written and what its references resolve to.
#[derive(Debug, Clone)]
pub struct RefContext {
    /// Relative links and images resolve against this (the repository's default branch).
    pub base: DocumentBase,
    pub owner: String,
    pub repo: String,
    pub refs: ReferenceSet,
}

/// Sanitised HTML for `source`. Stub: plain Markdown, references left as text.
pub fn render_issue_markdown(source: &str, ctx: &RefContext) -> String {
    render_markdown(source, Some(&ctx.base))
}
