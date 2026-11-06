//! Resolved search filters; SQL remains authoritative in the storage adapter.
use super::query::IssueSort;
use crate::{IssueState, threads::IssueKind};
use rbit_contracts::{AccountId, RepoId};
/// Personal relationship used by the cross-repository dashboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DashboardRelation {
    Created,
    Assigned,
    Mentioned,
    ReviewRequested,
}

impl DashboardRelation {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Assigned => "assigned",
            Self::Mentioned => "mentioned",
            Self::ReviewRequested => "review-requested",
        }
    }
}

/// The signed-in viewer and their selected relationship; visibility is evaluated in SQL.
#[derive(Debug, Clone, Copy)]
pub struct DashboardScope {
    pub viewer: AccountId,
    pub is_admin: bool,
    pub relation: DashboardRelation,
}

/// The filters of one list. Accounts and names are already resolved by the caller: an
/// author or assignee that does not exist is an id no row has (such as `AccountId(0)`),
/// so the list is empty.
#[derive(Debug, Clone)]
#[expect(clippy::struct_excessive_bools, reason = "independent filters")]
pub struct SearchFilter {
    pub repo: RepoId,
    /// When present, search all readable repositories instead of `repo`.
    pub dashboard: Option<DashboardScope>,
    pub kind: Option<IssueKind>,
    pub state: Option<IssueState>,
    /// Closed as merged (pull requests).
    pub merged: bool,
    /// Draft pull requests (`pull_requests.is_draft`).
    pub draft: bool,
    pub locked: bool,
    pub author: Option<AccountId>,
    pub assignee: Option<AccountId>,
    pub no_assignee: bool,
    /// Label names that must all be present (case-insensitive).
    pub labels: Vec<String>,
    /// Label names that must be absent.
    pub not_labels: Vec<String>,
    pub no_label: bool,
    /// A milestone title (case-insensitive).
    pub milestone: Option<String>,
    pub no_milestone: bool,
    /// Free-text words (title and body), the last one matched as a prefix.
    pub text: Vec<String>,
    pub sort: IssueSort,
    /// Only threads with at least one assignee (the API's `assignee=*`).
    pub any_assignee: bool,
    /// Only threads in some milestone (the API's `milestone=*`).
    pub any_milestone: bool,
    /// Only threads updated at or after this time (the API's `since`).
    pub updated_since: Option<rbit_contracts::Timestamp>,
}

impl SearchFilter {
    /// Everything in `repo`, newest first.
    pub fn new(repo: RepoId) -> Self {
        Self {
            repo,
            dashboard: None,
            kind: None,
            state: None,
            merged: false,
            draft: false,
            locked: false,
            author: None,
            assignee: None,
            no_assignee: false,
            labels: Vec::new(),
            not_labels: Vec::new(),
            no_label: false,
            milestone: None,
            no_milestone: false,
            text: Vec::new(),
            sort: IssueSort::default(),
            any_assignee: false,
            any_milestone: false,
            updated_since: None,
        }
    }
}
