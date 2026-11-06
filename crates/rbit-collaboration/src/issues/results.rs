//! Selected page projections retain their original owned maps and vectors.
use crate::{
    labels::LabelSnapshot,
    milestones::MilestoneRef,
    threads::{Issue, IssueId},
};
use rbit_contracts::{AccountId, RepoId};
use rbit_identity_models::Account;
use rbit_repositories::OwnedRepository;
use std::collections::HashMap;
/// Threads per HTML list page.
pub const ISSUES_PER_PAGE: u32 = 25;
/// Labels, assignees and milestone of one issue or pull request, for lists, payloads and
/// the API.
#[derive(Debug, Clone, Default)]
pub struct IssueMetadata {
    pub labels: Vec<LabelSnapshot>,
    pub assignees: Vec<Account>,
    pub milestone: Option<MilestoneRef>,
}

/// The existing concurrent author and selected-page metadata projection.
pub type IssueHydration = (Vec<Account>, HashMap<IssueId, IssueMetadata>);

/// One page of a list with its tab counts.
#[derive(Debug, Clone)]
pub struct IssueSearchResults {
    pub issues: Vec<Issue>,
    /// Authors of the listed threads, by id.
    pub authors: HashMap<AccountId, Account>,
    pub metadata: HashMap<IssueId, IssueMetadata>,
    /// Threads matching the query, in its state.
    pub total: u64,
    /// The query's other filters, open and closed.
    pub open: u64,
    pub closed: u64,
    /// 1-based page actually served.
    pub page: u32,
    pub per_page: u32,
    /// Problems with the query (unknown qualifiers, `@me` signed out).
    pub hints: Vec<String>,
    /// The listed threads' repositories the viewer may read, by id: filled by
    /// the personal dashboard (whose rows come from many repositories), empty
    /// otherwise.
    pub repositories: HashMap<RepoId, OwnedRepository>,
}

/// Which of a listed thread's metadata a page shows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MetadataNeeds {
    /// Labels, assignees and milestone.
    #[default]
    All,
    /// Labels only (the pull request list): one statement instead of three.
    Labels,
}

impl IssueSearchResults {
    pub fn page_count(&self) -> u32 {
        u32::try_from(self.total.div_ceil(u64::from(self.per_page)).max(1)).unwrap_or(u32::MAX)
    }
}
