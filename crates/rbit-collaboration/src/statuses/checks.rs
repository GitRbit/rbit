//! Decisions over the latest status of each context: a commit's combined state and how
//! the required contexts of a protected branch stand.

use std::collections::HashMap;

use rbit_contracts::AccountId;

use super::{CommitStatus, StatusState};

/// GitHub's combined state of a commit from the **latest** status of each context.
pub fn combined_state(latest: &[CommitStatus]) -> StatusState {
    if latest.iter().any(|status| status.state.is_failed()) {
        StatusState::Failure
    } else if latest.is_empty() || latest.iter().any(|status| status.state == StatusState::Pending) {
        StatusState::Pending
    } else {
        StatusState::Success
    }
}

/// A commit's combined status: the overall state and the latest status of each context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CombinedStatus {
    pub sha: String,
    pub state: StatusState,
    /// The latest status of each context, by context.
    pub statuses: Vec<CommitStatus>,
}

impl CombinedStatus {
    /// The combined status of `sha` from the latest status of each of its contexts.
    pub fn new(sha: String, statuses: Vec<CommitStatus>) -> Self {
        Self { state: combined_state(&statuses), sha, statuses }
    }

    /// One combined status per commit of `latest` (the latest statuses of several
    /// commits). Commits without statuses are absent.
    pub fn by_commit(latest: Vec<CommitStatus>) -> HashMap<String, Self> {
        let mut by_sha: HashMap<String, Vec<CommitStatus>> = HashMap::new();
        for status in latest {
            by_sha.entry(status.sha.clone()).or_default().push(status);
        }
        by_sha.into_iter().map(|(sha, statuses)| (sha.clone(), Self::new(sha, statuses))).collect()
    }
}

/// One page of a commit's statuses, newest first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusPage {
    pub sha: String,
    pub statuses: Vec<CommitStatus>,
    /// Statuses across all pages.
    pub total: u64,
}

/// The accounts that created `statuses`, once each, ascending. Statuses rbit Actions
/// posted and those of deleted accounts have no creator.
pub fn creator_ids(statuses: &[CommitStatus]) -> Vec<AccountId> {
    let mut ids: Vec<_> = statuses.iter().filter_map(|status| status.creator).collect();
    ids.sort_unstable();
    ids.dedup();
    ids
}

/// How the required contexts of a branch stand on a commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RequiredChecks {
    /// Required contexts.
    pub required: usize,
    /// Required contexts whose latest status is `success`.
    pub passing: usize,
    /// Required contexts whose latest status is `failure` or `error`.
    pub failing: usize,
}

impl RequiredChecks {
    /// Every required context passes.
    pub const fn all_pass(self) -> bool {
        self.passing == self.required
    }

    /// Required contexts that are pending or have not reported yet.
    pub const fn waiting(self) -> usize {
        self.required - self.passing - self.failing
    }
}

/// Counts `required` contexts by the state of their latest status in `latest`.
/// Duplicated contexts count once.
pub fn required_checks<S: AsRef<str>>(required: &[S], latest: &[CommitStatus]) -> RequiredChecks {
    let mut contexts: Vec<&str> = required.iter().map(AsRef::as_ref).collect();
    contexts.sort_unstable();
    contexts.dedup();
    let mut checks = RequiredChecks { required: contexts.len(), ..RequiredChecks::default() };
    for context in contexts {
        let newest = latest.iter().filter(|status| status.context.as_str() == context).max_by_key(|status| status.id);
        match newest.map(|status| status.state) {
            Some(StatusState::Success) => checks.passing += 1,
            Some(StatusState::Failure | StatusState::Error) => checks.failing += 1,
            Some(StatusState::Pending) | None => {}
        }
    }
    checks
}
