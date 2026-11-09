//! Persistence owns the capped insert (count and insert in one statement) and ordering.

use std::future::Future;

use rbit_contracts::{AccountId, RepoId};

use super::{CommitStatus, StatusId};

/// Who a stored status is recorded for: a person (`source = user`, with a creator) or
/// rbit Actions (`source = actions`, no creator).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusAuthor {
    Account(AccountId),
    Actions,
}

/// Commit status storage. `Report` is the validated report as the caller parsed it; the
/// application only hands it to [`create`](Self::create).
pub trait CommitStatusRepository: Sync {
    type Error;
    type Report: Sync;

    /// Stores `report` on `sha` unless the commit already has `cap` reports for its
    /// context; `None` (and nothing stored) then.
    fn create(
        &self,
        repo: RepoId,
        sha: &str,
        report: &Self::Report,
        author: StatusAuthor,
        cap: u32,
    ) -> impl Future<Output = Result<Option<CommitStatus>, Self::Error>> + Send;
    fn find(&self, repo: RepoId, id: StatusId) -> impl Future<Output = Result<Option<CommitStatus>, Self::Error>> + Send;
    /// Reports on `sha`, newest first, and the total across all pages.
    fn page(
        &self,
        repo: RepoId,
        sha: &str,
        offset: u64,
        limit: u32,
    ) -> impl Future<Output = Result<(Vec<CommitStatus>, u64), Self::Error>> + Send;
    /// The latest report of each context of `sha`, by context.
    fn latest(&self, repo: RepoId, sha: &str) -> impl Future<Output = Result<Vec<CommitStatus>, Self::Error>> + Send;
    /// The latest report of each context of every commit of `shas`, in one query.
    fn latest_of(&self, repo: RepoId, shas: &[String]) -> impl Future<Output = Result<Vec<CommitStatus>, Self::Error>> + Send;
}

// Borrowed stores keep their identity without cloning the handle.
impl<R: CommitStatusRepository + ?Sized> CommitStatusRepository for &R {
    type Error = R::Error;
    type Report = R::Report;

    async fn create(
        &self,
        repo: RepoId,
        sha: &str,
        report: &Self::Report,
        author: StatusAuthor,
        cap: u32,
    ) -> Result<Option<CommitStatus>, Self::Error> {
        R::create(*self, repo, sha, report, author, cap).await
    }

    async fn find(&self, repo: RepoId, id: StatusId) -> Result<Option<CommitStatus>, Self::Error> {
        R::find(*self, repo, id).await
    }

    async fn page(&self, repo: RepoId, sha: &str, offset: u64, limit: u32) -> Result<(Vec<CommitStatus>, u64), Self::Error> {
        R::page(*self, repo, sha, offset, limit).await
    }

    async fn latest(&self, repo: RepoId, sha: &str) -> Result<Vec<CommitStatus>, Self::Error> {
        R::latest(*self, repo, sha).await
    }

    async fn latest_of(&self, repo: RepoId, shas: &[String]) -> Result<Vec<CommitStatus>, Self::Error> {
        R::latest_of(*self, repo, shas).await
    }
}
