//! Commit statuses on pages: the combined status mark of a commit (commit lists, the
//! commit page) and the list of checks (the commit page, the pull request merge box).
//!
//! Milestone 3 workstream S (commit statuses and required checks) owns this file and
//! `templates/statuses/**`.
//!
//! `statuses/mark.html` expects `mark: StatusMark`, `statuses/checks.html` expects
//! `checks: ChecksView`. Colour marks the state only (`text-success`, `text-danger`,
//! `text-warning`); everything else stays monochrome. Target URLs were validated as
//! absolute `http`/`https` URLs when the status was posted and are linked with
//! `rel="nofollow noopener noreferrer"`.

use rbit_core::status::{CommitStatus, StatusContext, StatusState};
use rbit_services::statuses::CombinedStatus;

/// The in-page anchor of the checks on the commit page.
pub const CHECKS_ANCHOR: &str = "checks";

/// How a state is shown: `(key, label, tone)`; the key picks the icon in the templates.
const fn look(state: StatusState) -> (&'static str, &'static str, &'static str) {
    match state {
        StatusState::Success => ("success", "Success", "text-success"),
        StatusState::Failure => ("failure", "Failure", "text-danger"),
        StatusState::Error => ("error", "Error", "text-danger"),
        StatusState::Pending => ("pending", "Pending", "text-warning"),
    }
}

/// "2 successful, 1 failing and 1 pending check" for the latest statuses.
pub fn summary(statuses: &[CommitStatus]) -> String {
    let count = |wanted: &[StatusState]| statuses.iter().filter(|status| wanted.contains(&status.state)).count();
    let parts: Vec<String> = [
        (count(&[StatusState::Success]), "successful"),
        (count(&[StatusState::Failure, StatusState::Error]), "failing"),
        (count(&[StatusState::Pending]), "pending"),
    ]
    .into_iter()
    .filter(|(count, _)| *count > 0)
    .map(|(count, word)| format!("{count} {word}"))
    .collect();
    let noun = if statuses.len() == 1 { "check" } else { "checks" };
    match parts.as_slice() {
        [] => "No checks".to_owned(),
        [only] => format!("{only} {noun}"),
        [first @ .., last] => format!("{} and {last} {noun}", first.join(", ")),
    }
}

/// A commit's combined status as one small icon linking to its checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusMark {
    /// `success`, `failure` or `pending` (the combined state).
    pub key: &'static str,
    pub tone: &'static str,
    /// "Success: 2 successful checks", for the tooltip and screen readers.
    pub title: String,
    pub url: String,
}

impl StatusMark {
    /// The mark of `combined`, linking to `url`; `None` when nothing reported.
    pub fn new(combined: &CombinedStatus, url: String) -> Option<Self> {
        if combined.statuses.is_empty() {
            return None;
        }
        let (key, label, tone) = look(combined.state);
        Some(Self { key, tone, title: format!("{label}: {}", summary(&combined.statuses)), url })
    }
}

/// One check: a context's latest status, or a required context that has not reported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckRow {
    /// `success`, `failure`, `error`, `pending` or `expected` (required, nothing reported).
    pub key: &'static str,
    pub label: &'static str,
    pub tone: &'static str,
    pub context: String,
    pub description: String,
    pub target_url: Option<String>,
    pub required: bool,
}

/// The checks of a commit, by context, with a one-line summary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChecksView {
    pub anchor: &'static str,
    pub summary: String,
    pub rows: Vec<CheckRow>,
}

impl ChecksView {
    /// The latest `statuses` (one per context) and the `required` contexts; `None` when
    /// there is neither.
    pub fn new(statuses: &[CommitStatus], required: &[StatusContext]) -> Option<Self> {
        if statuses.is_empty() && required.is_empty() {
            return None;
        }
        let mut rows: Vec<CheckRow> = statuses
            .iter()
            .map(|status| {
                let (key, label, tone) = look(status.state);
                CheckRow {
                    key,
                    label,
                    tone,
                    context: status.context.to_string(),
                    description: status.description.clone().unwrap_or_default(),
                    target_url: status.target_url.clone(),
                    required: required.contains(&status.context),
                }
            })
            .collect();
        for context in required.iter().filter(|context| !statuses.iter().any(|status| &status.context == *context)) {
            rows.push(CheckRow {
                key: "expected",
                label: "Expected",
                tone: "text-ink-faint",
                context: context.to_string(),
                description: "Waiting for this status to be reported".to_owned(),
                target_url: None,
                required: true,
            });
        }
        // Required checks first, then by name.
        rows.sort_by(|a, b| b.required.cmp(&a.required).then_with(|| a.context.cmp(&b.context)));
        let missing = rows.iter().filter(|row| row.key == "expected").count();
        let mut summary = summary(statuses);
        if missing > 0 {
            summary = format!("{summary}, {missing} expected");
        }
        Some(Self { anchor: CHECKS_ANCHOR, summary, rows })
    }
}

#[cfg(test)]
mod tests {
    use rbit_core::{RepoId, Timestamp, status::StatusId};

    use super::*;

    fn status(context: &str, state: StatusState) -> CommitStatus {
        CommitStatus {
            id: StatusId(1),
            repo_id: RepoId(1),
            sha: "a".repeat(40),
            state,
            context: StatusContext::parse(context).unwrap(),
            description: Some(format!("{context} says {state}")),
            target_url: Some("https://ci.example/1".to_owned()),
            creator: None,
            source: rbit_core::status::StatusSource::User,
            created_at: Timestamp(0),
            updated_at: Timestamp(0),
        }
    }

    #[test]
    fn summaries_count_by_state() {
        assert_eq!(summary(&[]), "No checks");
        assert_eq!(summary(&[status("a", StatusState::Success)]), "1 successful check");
        let mixed = [
            status("a", StatusState::Success),
            status("b", StatusState::Error),
            status("c", StatusState::Failure),
            status("d", StatusState::Pending),
        ];
        assert_eq!(summary(&mixed), "1 successful, 2 failing and 1 pending checks");
    }

    #[test]
    fn marks_show_the_combined_state() {
        let empty = CombinedStatus { sha: "a".repeat(40), state: StatusState::Pending, statuses: Vec::new() };
        assert_eq!(StatusMark::new(&empty, "/x".to_owned()), None, "no mark without statuses");
        let statuses = vec![status("a", StatusState::Success), status("b", StatusState::Failure)];
        let failed = CombinedStatus { sha: "a".repeat(40), state: StatusState::Failure, statuses };
        let mark = StatusMark::new(&failed, "/c#checks".to_owned()).unwrap();
        assert_eq!((mark.key, mark.tone, mark.title.as_str()), ("failure", "text-danger", "Failure: 1 successful and 1 failing checks"));
    }

    /// Renders both templates the way the pages include them.
    #[derive(askama::Template)]
    #[template(
        source = r#"{% if let Some(mark) = mark %}{% include "statuses/mark.html" %}{% endif %}{% include "statuses/checks.html" %}"#,
        ext = "html"
    )]
    struct Fragments {
        mark: Option<StatusMark>,
        checks: ChecksView,
    }

    #[test]
    fn the_templates_render_icons_links_and_no_inline_styles() {
        use askama::Template as _;
        let required = [StatusContext::parse("deploy").unwrap()];
        let mut failing = status("lint <b>", StatusState::Error);
        failing.target_url = None;
        let statuses = vec![status("build", StatusState::Success), failing];
        let combined = CombinedStatus { sha: "a".repeat(40), state: StatusState::Failure, statuses };
        let html = Fragments {
            mark: StatusMark::new(&combined, "/acme/rocket/commit/aaa#checks".to_owned()),
            checks: ChecksView::new(&combined.statuses, &required).unwrap(),
        }
        .render()
        .unwrap();
        assert!(html.contains(r#"href="/acme/rocket/commit/aaa#checks""#) && html.contains("text-danger"));
        assert!(html.contains(r#"title="Failure: 1 successful and 1 failing checks""#));
        assert!(html.contains(r#"id="checks""#) && html.contains(">deploy<") && html.contains(">Required<") && html.contains("Expected"));
        assert!(html.contains(r#"href="https://ci.example/1" rel="nofollow noopener noreferrer""#));
        assert_eq!(html.matches(">Details<").count(), 1, "only statuses with a target URL link out");
        assert!(html.contains("lint &#60;b&#62;") || html.contains("lint &lt;b&gt;"), "contexts are escaped: {html}");
        assert!(!html.contains("style="), "no inline styles under the CSP");
    }

    #[test]
    fn checks_list_required_ones_first_and_the_missing_ones() {
        assert_eq!(ChecksView::new(&[], &[]), None);
        let required = [StatusContext::parse("deploy").unwrap(), StatusContext::parse("lint").unwrap()];
        let statuses = [status("build", StatusState::Pending), status("lint", StatusState::Success)];
        let checks = ChecksView::new(&statuses, &required).unwrap();
        let rows: Vec<_> = checks.rows.iter().map(|row| (row.context.as_str(), row.key, row.required)).collect();
        assert_eq!(rows, [("deploy", "expected", true), ("lint", "success", true), ("build", "pending", false)]);
        assert_eq!(checks.summary, "1 successful and 1 pending checks, 1 expected");
        assert_eq!(checks.rows[1].target_url.as_deref(), Some("https://ci.example/1"));
        assert_eq!(checks.rows[0].description, "Waiting for this status to be reported");
    }
}
