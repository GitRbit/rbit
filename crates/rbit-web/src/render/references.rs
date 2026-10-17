//! Markdown for issues, pull requests and comments, with references linked: `#12` (when
//! it exists in this repository), `owner/repo#12` and `@name` (when the account exists).
//!
//! Milestone 2 workstream I (issues) owns this file (and, in wave B, `markdown.rs`).
//!
//! Hook (`docs/design/milestone-2b.md` §3): T0's timeline views, I and P render every
//! issue text through [`render_issue_markdown`], after resolving its references with
//! `ReferenceService::resolve`.
//!
//! References are found only in plain text runs (never in code, code blocks or link
//! text) by `rbit_core::references::find`, the same parser the services use.
//! `owner/repo#12` always links, without checking that it exists, so nothing leaks; a
//! `#12` that is not a thread of this repository and an `@name` that is no account stay
//! plain text. Ammonia sanitisation stays the last step.

use rbit_core::references::{Reference, find};
use rbit_services::references::ReferenceSet;

use crate::{
    render::{Rewritten, escape_html, render_markdown_rewriting},
    urls::{DocumentBase, issue_url},
};

/// Where a text was written and what its references resolve to.
#[derive(Debug, Clone)]
pub struct RefContext {
    /// Relative links and images resolve against this (the repository's default branch).
    pub base: DocumentBase,
    pub owner: String,
    pub repo: String,
    pub refs: ReferenceSet,
}

/// Sanitised HTML for `source`, with its references linked.
pub fn render_issue_markdown(source: &str, ctx: &RefContext) -> String {
    render_markdown_rewriting(source, Some(&ctx.base), &|text| link_references(text, ctx))
}

/// `text` as HTML with its resolvable references turned into links, or `None` when
/// nothing in it links.
fn link_references(text: &str, ctx: &RefContext) -> Option<Rewritten> {
    let mut html = String::with_capacity(text.len() + 64);
    let mut hrefs = Vec::new();
    let mut at = 0;
    for found in find(text) {
        let written = &text[found.start..found.end];
        let link = match &found.reference {
            Reference::Local(number) => ctx.refs.issue(*number).map(|issue| {
                let url = issue_url(&ctx.owner, &ctx.repo, issue.kind, *number);
                let title = format!("{} ({})", issue.title, issue.state.as_str());
                (url, Some(title))
            }),
            Reference::Remote(remote) => Some((format!("/{}/{}/issues/{}", remote.owner, remote.repo, remote.number.get()), None)),
            Reference::Mention(name) => ctx.refs.account(name).map(|stored| (format!("/{stored}"), None)),
        };
        let Some((url, title)) = link else { continue };
        html.push_str(&escape_html(&text[at..found.start]));
        html.push_str("<a href=\"");
        html.push_str(&escape_html(&url));
        html.push('"');
        if let Some(title) = title {
            html.push_str(" title=\"");
            html.push_str(&escape_html(&title));
            html.push('"');
        }
        html.push('>');
        html.push_str(&escape_html(written));
        html.push_str("</a>");
        hrefs.push(url);
        at = found.end;
    }
    if hrefs.is_empty() {
        return None;
    }
    html.push_str(&escape_html(&text[at..]));
    Some(Rewritten { html, hrefs })
}

#[cfg(test)]
mod tests {
    use rbit_core::{IssueKind, IssueNumber, IssueState};
    use rbit_services::references::ReferencedIssue;

    use super::*;
    use crate::urls::RepoUrls;

    fn ctx() -> RefContext {
        let mut refs = ReferenceSet::default();
        let n = |value| IssueNumber::new(value).unwrap();
        refs.issues
            .insert(n(1), ReferencedIssue { number: n(1), kind: IssueKind::Issue, state: IssueState::Open, title: "Crash <boom>".into() });
        refs.issues.insert(n(2), ReferencedIssue { number: n(2), kind: IssueKind::Pull, state: IssueState::Closed, title: "Fix".into() });
        refs.accounts.insert("bob".into(), "Bob".into());
        RefContext { base: DocumentBase::new(RepoUrls::new("alice", "demo"), "main", ""), owner: "alice".into(), repo: "demo".into(), refs }
    }

    #[test]
    fn links_existing_references_only() {
        let html = render_issue_markdown("Fixes #1, see #2 and #3; thanks @BOB and @nobody. acme/web#9", &ctx());
        assert!(
            html.contains(r#"<a href="/alice/demo/issues/1" title="Crash &lt;boom&gt; (open)" rel="noopener noreferrer nofollow">#1</a>"#),
            "{html}"
        );
        assert!(html.contains(r#"<a href="/alice/demo/pull/2""#), "pull requests link to their page: {html}");
        assert!(!html.contains("issues/3"), "#3 does not exist: {html}");
        assert!(html.contains(r#"<a href="/Bob" rel="noopener noreferrer nofollow">@BOB</a>"#), "{html}");
        assert!(html.contains("@nobody"), "{html}");
        assert!(!html.contains("/nobody"), "{html}");
        assert!(html.contains(r#"<a href="/acme/web/issues/9""#), "cross-repository references always link: {html}");
    }

    #[test]
    fn ignores_code_and_link_text_and_keeps_markup_safe() {
        let html = render_issue_markdown("`#1` and\n\n```\n#1 @bob\n```\n\n[#1](https://example.com) <script>#1</script> **#1**", &ctx());
        assert_eq!(html.matches("/alice/demo/issues/1").count(), 1, "only the bold one: {html}");
        assert!(html.contains("<code>#1</code>"), "{html}");
        assert!(!html.contains("<script"), "{html}");
        assert!(html.contains(r#"href="https://example.com""#), "{html}");
    }

    #[test]
    fn plain_text_is_escaped_around_links() {
        let html = render_issue_markdown("a < b & #1 > c", &ctx());
        assert!(html.contains("a &lt; b &amp; <a"), "{html}");
        assert!(html.contains("</a> &gt; c"), "{html}");
        let relative = render_issue_markdown("[guide](docs/guide.md) and #1", &ctx());
        assert!(relative.contains(r#"href="/alice/demo/blob/main/docs/guide.md""#), "relative links still resolve: {relative}");
    }
}
