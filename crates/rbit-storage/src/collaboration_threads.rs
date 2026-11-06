//! Static shared-thread row port; original SQL/row validation is unchanged.

use crate::{StorageError, sqlite::issues::IssueStore};
use rbit_collaboration::threads::{Issue, IssueNumber, NewIssue, ThreadRepository};
use rbit_core::{Account, RepoId};

impl ThreadRepository for IssueStore {
    type Error = StorageError;
    async fn create(&self, new: &NewIssue) -> Result<Issue, Self::Error> {
        Self::create(self, new).await
    }
    async fn find_by_number_with_author(&self, repo: RepoId, number: IssueNumber) -> Result<Option<(Issue, Option<Account>)>, Self::Error> {
        Self::find_by_number_with_author(self, repo, number).await
    }
    async fn open_counts(&self, repo: RepoId) -> Result<(u64, u64), Self::Error> {
        Self::open_counts(self, repo).await
    }
}
