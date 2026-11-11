//! The filters of the runs list ([`super::actions_runs::ActionsRunStore::list_runs`]).

use rbit_core::{
    AccountId,
    actions::{RunEvent, RunId, RunStatus},
};

/// Filters of the runs list. `before` pages by id (newest first), `offset` by position
/// (the API's pages).
#[derive(Debug, Clone, Default)]
pub struct RunFilter<'a> {
    pub workflow_path: Option<&'a str>,
    pub event: Option<RunEvent>,
    pub status: Option<RunStatus>,
    pub actor: Option<AccountId>,
    /// The full ref; a branch's (`refs/heads/…`) also matches its pull requests' runs.
    pub reference: Option<&'a str>,
    /// The commit the run tested: a full id, or a prefix of 7 or more hex digits.
    pub sha: Option<&'a str>,
    pub before: Option<RunId>,
    pub limit: u32,
    // Actions: integration (I).
    /// One of these statuses, when not empty (a GitHub status such as `completed` stands
    /// for several); applies together with `status`.
    pub statuses: &'a [RunStatus],
    /// Runs skipped after the filters apply.
    pub offset: u32,
}
