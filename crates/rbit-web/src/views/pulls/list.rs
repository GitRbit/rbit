//! Pull request list view models.

use super::{IssueState, Page, Pagination, PullListItem, RepoHeader, StateBadge, Template, WebTemplate, badge};

/// One label of a list row (colour as an SVG fill).
#[derive(Debug, Clone)]
pub struct LabelView {
    pub name: String,
    pub color: String,
}

#[derive(Debug, Clone)]
pub struct PullRowView {
    pub url: String,
    pub title: String,
    pub number: String,
    pub badge: StateBadge,
    pub author: String,
    pub when: String,
    pub when_iso: String,
    pub labels: Vec<LabelView>,
    pub head_label: String,
    pub base_branch: String,
    pub review: Option<(&'static str, &'static str)>,
    pub comment_count: u32,
}

impl PullRowView {
    pub fn new(item: &PullListItem, owner: &str, repo: &str, labels: Vec<LabelView>) -> Self {
        let issue = &item.issue;
        let review = if item.changes_requested > 0 {
            Some(("Changes requested", "text-danger"))
        } else if item.approvals > 0 {
            Some(("Approved", "text-success"))
        } else {
            None
        };
        let at = if issue.state == IssueState::Open { issue.created_at } else { issue.closed_at.unwrap_or(issue.updated_at) };
        Self {
            url: format!("/{owner}/{repo}/pull/{}", issue.number.get()),
            title: issue.title.to_string(),
            number: issue.number.to_string(),
            badge: badge(issue, item.pull.is_draft, item.pull.is_merged()),
            author: item.author.as_ref().map_or_else(|| "ghost".to_owned(), |author| author.name.to_string()),
            when: at.relative(),
            when_iso: at.iso8601(),
            labels,
            head_label: item.pull.head_branch.clone(),
            base_branch: item.pull.base_branch.clone(),
            review,
            comment_count: issue.comment_count,
        }
    }

    /// "opened" / "merged" / "closed", for the row's metadata line.
    pub fn verb(&self) -> &'static str {
        match self.badge.label {
            "Merged" => "merged",
            "Closed" => "closed",
            _ => "opened",
        }
    }
}

/// The Open / Closed tabs of the list.
#[derive(Debug, Clone)]
pub struct ListTabs {
    pub open_url: String,
    pub closed_url: String,
    pub open: u64,
    pub closed: u64,
    /// `open`, `closed` or `merged`.
    pub state: &'static str,
}

#[derive(Debug, Template, WebTemplate)]
#[template(path = "pulls/list.html")]
pub struct PullListPage {
    pub page: Page,
    pub header: RepoHeader,
    pub tabs: ListTabs,
    pub rows: Vec<PullRowView>,
    pub pagination: Pagination,
    pub new_url: String,
    pub author_filter: Option<String>,
}
