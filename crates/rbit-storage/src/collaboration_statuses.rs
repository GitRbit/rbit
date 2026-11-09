//! Static commit status port adapter; the capped insert and latest-only SQL are unchanged.

use rbit_collaboration::statuses::{CommitStatus, CommitStatusRepository, StatusAuthor, StatusId};
use rbit_core::{RepoId, status::StatusReport};

use crate::{StorageError, sqlite::statuses::CommitStatusStore};

impl CommitStatusRepository for CommitStatusStore {
    type Error = StorageError;
    type Report = StatusReport;

    async fn create(
        &self,
        repo: RepoId,
        sha: &str,
        report: &StatusReport,
        author: StatusAuthor,
        cap: u32,
    ) -> Result<Option<CommitStatus>, Self::Error> {
        match author {
            StatusAuthor::Account(creator) => Self::create(self, repo, sha, report, creator, cap).await,
            StatusAuthor::Actions => Self::create_as_system(self, repo, sha, report, cap).await,
        }
    }

    async fn find(&self, repo: RepoId, id: StatusId) -> Result<Option<CommitStatus>, Self::Error> {
        Self::find(self, repo, id).await
    }

    async fn page(&self, repo: RepoId, sha: &str, offset: u64, limit: u32) -> Result<(Vec<CommitStatus>, u64), Self::Error> {
        tokio::try_join!(self.list(repo, sha, offset, limit), self.count(repo, sha))
    }

    async fn latest(&self, repo: RepoId, sha: &str) -> Result<Vec<CommitStatus>, Self::Error> {
        Self::latest(self, repo, sha).await
    }

    async fn latest_of(&self, repo: RepoId, shas: &[String]) -> Result<Vec<CommitStatus>, Self::Error> {
        Self::latest_of(self, repo, shas).await
    }
}
