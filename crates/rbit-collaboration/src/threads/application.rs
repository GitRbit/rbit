//! Shared row creation/read policy for numbered threads.

use rbit_contracts::{AccountId, RepoId, Timestamp};
use rbit_identity_models::Account;

use super::{Issue, IssueKind, IssueNumber, IssueText, IssueTextError, IssueTitle, NewIssue, ThreadRepository};

#[derive(Debug, Clone, Copy)]
pub struct ThreadAccess {
    pub repo: RepoId,
    pub can_read: bool,
    pub archived: bool,
}

/// Borrowed creation fields retain caller ownership until validation finishes.
#[derive(Debug, Clone, Copy)]
pub struct ThreadDraft<'a> {
    pub author: AccountId,
    pub kind: IssueKind,
    pub title: &'a str,
    pub body: &'a str,
}

#[derive(Debug)]
pub enum ThreadError<E, P = E> {
    Store(E),
    Throttle(P),
    Text(IssueTextError),
    NotFound,
    Archived,
}

/// Open threads of a repository, for the Issues and Pull requests tabs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OpenCounts {
    pub issues: u64,
    pub pulls: u64,
}

#[derive(Debug)]
pub struct ThreadApplication<'a, R> {
    repository: &'a R,
    access: ThreadAccess,
}

impl<'a, R: ThreadRepository> ThreadApplication<'a, R> {
    pub fn new(repository: &'a R, access: ThreadAccess) -> Self {
        Self { repository, access }
    }

    pub async fn open(&self, number: IssueNumber) -> Result<(Issue, Option<Account>), ThreadError<R::Error>> {
        if !self.access.can_read {
            return Err(ThreadError::NotFound);
        }
        self.repository.find_by_number_with_author(self.access.repo, number).await.map_err(ThreadError::Store)?.ok_or(ThreadError::NotFound)
    }

    /// Counts retain their existing caller-owned visibility gate; no new check/read.
    pub async fn counts(&self) -> Result<OpenCounts, ThreadError<R::Error>> {
        let (issues, pulls) = self.repository.open_counts(self.access.repo).await.map_err(ThreadError::Store)?;
        Ok(OpenCounts { issues, pulls })
    }

    pub async fn create<P>(
        &self,
        draft: ThreadDraft<'_>,
        spend: impl FnOnce() -> Result<(), P>,
        now: impl FnOnce() -> Timestamp,
    ) -> Result<Issue, ThreadError<R::Error, P>> {
        if !self.access.can_read {
            return Err(ThreadError::NotFound);
        }
        if self.access.archived {
            return Err(ThreadError::Archived);
        }
        let title = IssueTitle::parse(draft.title).map_err(ThreadError::Text)?;
        let body = IssueText::body(draft.body).map_err(ThreadError::Text)?;
        spend().map_err(ThreadError::Throttle)?;
        let new = NewIssue { repo: self.access.repo, kind: draft.kind, title, body, author: draft.author, at: now() };
        self.repository.create(&new).await.map_err(ThreadError::Store)
    }
}
