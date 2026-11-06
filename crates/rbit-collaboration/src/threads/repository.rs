//! The existing SQL thread transaction and joined read remain adapter authority.

use std::future::Future;

use rbit_contracts::RepoId;
use rbit_identity_models::Account;

use super::{Issue, IssueNumber, NewIssue};

pub trait ThreadRepository: Sync {
    type Error;
    fn create(&self, new: &NewIssue) -> impl Future<Output = Result<Issue, Self::Error>> + Send;
    fn find_by_number_with_author(
        &self,
        repo: RepoId,
        number: IssueNumber,
    ) -> impl Future<Output = Result<Option<(Issue, Option<Account>)>, Self::Error>> + Send;
    fn open_counts(&self, repo: RepoId) -> impl Future<Output = Result<(u64, u64), Self::Error>> + Send;
}
