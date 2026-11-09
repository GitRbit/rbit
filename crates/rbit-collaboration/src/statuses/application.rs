//! Who may post a status and in which order a submission is checked, with the legacy
//! service's error text. Event publication and logging stay with the caller.

use std::{collections::HashMap, future::Future};

use rbit_contracts::{AccountId, RepoId};

use super::{
    CombinedStatus, CommitStatus, CommitStatusRepository, MAX_STATUSES_PER_CONTEXT, StatusAuthor, StatusError, StatusId, StatusPage,
    parse_sha,
};

/// Access already resolved for this request; hidden repositories never reach this.
#[derive(Debug, Clone, Copy)]
pub struct StatusAccess {
    pub repo: RepoId,
    pub can_write: bool,
    pub archived: bool,
}

/// Policy errors stay distinct from persistence (`E`) and commit lookup (`G`) errors.
#[derive(Debug)]
pub enum StatusApplicationError<E, G = E> {
    Store(E),
    Commit(G),
    Invalid(String),
    Forbidden,
    Archived,
}

/// A commit and context that already have [`MAX_STATUSES_PER_CONTEXT`] statuses.
fn at_cap<E, G>() -> StatusApplicationError<E, G> {
    StatusApplicationError::Invalid(format!("this SHA and context has reached the maximum number of statuses ({MAX_STATUSES_PER_CONTEXT})"))
}

/// Statically dispatched status workflows for one repository.
#[derive(Debug)]
pub struct StatusApplication<R> {
    repository: R,
    access: StatusAccess,
}

impl<R: CommitStatusRepository> StatusApplication<R> {
    pub fn new(repository: R, access: StatusAccess) -> Self {
        Self { repository, access }
    }

    /// Records a status of `sha` (a full id) by `author`, who needs write access to a
    /// repository that is not archived. `report` parses the submitted values and
    /// `commit` finds the full id of `sha` (`None` when there is no such commit).
    pub async fn create<G, Fut>(
        &self,
        author: AccountId,
        sha: &str,
        report: impl FnOnce() -> Result<R::Report, StatusError>,
        commit: impl FnOnce(String) -> Fut,
    ) -> Result<CommitStatus, StatusApplicationError<R::Error, G>>
    where
        Fut: Future<Output = Result<Option<String>, G>>,
    {
        let (sha, report) = self.checked(sha, report, commit).await?;
        self.store(&sha, &report, StatusAuthor::Account(author)).await
    }

    /// Records a status a workflow task of `task_repo` posted with its token, with the
    /// checks of [`create`](Self::create), as rbit Actions (no creator). A token of
    /// another repository is refused before anything else.
    pub async fn create_for_task<G, Fut>(
        &self,
        task_repo: RepoId,
        sha: &str,
        report: impl FnOnce() -> Result<R::Report, StatusError>,
        commit: impl FnOnce(String) -> Fut,
    ) -> Result<CommitStatus, StatusApplicationError<R::Error, G>>
    where
        Fut: Future<Output = Result<Option<String>, G>>,
    {
        if self.access.repo != task_repo {
            return Err(StatusApplicationError::Forbidden);
        }
        let (sha, report) = self.checked(sha, report, commit).await?;
        self.store(&sha, &report, StatusAuthor::Actions).await
    }

    /// Write access, a repository that is not archived, a full commit id, valid values
    /// and a commit that exists, in that order.
    async fn checked<G, Fut>(
        &self,
        sha: &str,
        report: impl FnOnce() -> Result<R::Report, StatusError>,
        commit: impl FnOnce(String) -> Fut,
    ) -> Result<(String, R::Report), StatusApplicationError<R::Error, G>>
    where
        Fut: Future<Output = Result<Option<String>, G>>,
    {
        if !self.access.can_write {
            return Err(StatusApplicationError::Forbidden);
        }
        if self.access.archived {
            return Err(StatusApplicationError::Archived);
        }
        let invalid = |error: StatusError| StatusApplicationError::Invalid(error.to_string());
        let sha = parse_sha(sha).map_err(invalid)?;
        let report = report().map_err(invalid)?;
        match commit(sha.clone()).await.map_err(StatusApplicationError::Commit)? {
            Some(found) if found == sha => Ok((sha, report)),
            _ => Err(StatusApplicationError::Invalid(format!("no commit found for SHA {sha}"))),
        }
    }

    async fn store<G>(
        &self,
        sha: &str,
        report: &R::Report,
        author: StatusAuthor,
    ) -> Result<CommitStatus, StatusApplicationError<R::Error, G>> {
        let stored = self.repository.create(self.access.repo, sha, report, author, MAX_STATUSES_PER_CONTEXT).await;
        stored.map_err(StatusApplicationError::Store)?.ok_or_else(at_cap)
    }

    /// Records `report` on `sha` (a full id, not checked) as rbit Actions reporting its
    /// own jobs: no permission check. `None` when the context is at its cap.
    pub async fn record_system(&self, sha: &str, report: &R::Report) -> Result<Option<CommitStatus>, R::Error> {
        self.repository.create(self.access.repo, sha, report, StatusAuthor::Actions, MAX_STATUSES_PER_CONTEXT).await
    }

    /// Report `id` of this repository.
    pub async fn find(&self, id: StatusId) -> Result<Option<CommitStatus>, R::Error> {
        self.repository.find(self.access.repo, id).await
    }

    /// The statuses of commit `sha`, newest first.
    pub async fn page(&self, sha: String, offset: u64, limit: u32) -> Result<StatusPage, R::Error> {
        let (statuses, total) = self.repository.page(self.access.repo, &sha, offset, limit).await?;
        Ok(StatusPage { sha, statuses, total })
    }

    /// The combined status of commit `sha`.
    pub async fn combined(&self, sha: String) -> Result<CombinedStatus, R::Error> {
        let statuses = self.repository.latest(self.access.repo, &sha).await?;
        Ok(CombinedStatus::new(sha, statuses))
    }

    /// The combined status of every commit of `shas` that has statuses (one query).
    pub async fn combined_of(&self, shas: &[String]) -> Result<HashMap<String, CombinedStatus>, R::Error> {
        Ok(CombinedStatus::by_commit(self.repository.latest_of(self.access.repo, shas).await?))
    }
}
