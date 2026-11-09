//! Static adapter for `rbit_collaboration::pulls::PullCreation`: the fork network, the
//! mirror flag, branch tips, the counted comparison and the capped insert.

use rbit_collaboration::pulls::{BranchComparison, PullCreationError, PullCreationPort, PullDraft, PullSide};
use rbit_core::{Issue, IssueKind, IssueNumber, Timestamp, User, pull::PullRequest};
use rbit_git::DiffLimits;
use rbit_storage::sqlite::{
    issues::NewIssue,
    pulls::{CreatedPull, NewPull},
};

use super::PullService;
use crate::{RepoContext, ServiceError};

pub(super) fn creation_error(error: PullCreationError<ServiceError>) -> ServiceError {
    match error {
        PullCreationError::Port(error) => error,
        PullCreationError::Text(error) => error.into(),
        PullCreationError::Invalid(message) => ServiceError::Invalid(message),
        PullCreationError::NotFound => ServiceError::NotFound,
        PullCreationError::Archived => ServiceError::Archived,
    }
}

/// One `actor` opening a pull request from `head` into `base`.
pub(super) struct CreationView<'a> {
    pub service: &'a PullService,
    pub actor: &'a User,
    pub base: &'a RepoContext,
    pub head: &'a RepoContext,
}

impl PullCreationPort for CreationView<'_> {
    type Error = ServiceError;
    type Created = (Issue, PullRequest);

    async fn head_in_network(&self) -> Result<bool, Self::Error> {
        let network = self.service.network(Some(self.actor), self.base).await?;
        Ok(network.iter().any(|repo| repo.repo.id == self.head.repo.repo.id))
    }

    async fn base_is_mirror(&self) -> Result<bool, Self::Error> {
        Ok(self.service.ctx().database.mirrors().is_mirror(self.base.repo.repo.id).await?)
    }

    async fn tip(&self, side: PullSide, reference: String) -> Option<String> {
        let repo = match side {
            PullSide::Base => self.base,
            PullSide::Head => self.head,
        };
        repo.reader.commit_id(reference).await.ok()
    }

    async fn compare(&self, base_tip: &str, head_tip: &str) -> Result<BranchComparison, Self::Error> {
        let no_files = DiffLimits { max_files: 0, ..DiffLimits::default() };
        let comparison = self.service.compare_counting(self.base, base_tip, self.head, head_tip, no_files, 2, false).await?;
        Ok(BranchComparison {
            related: comparison.merge_base.is_some(),
            ahead: comparison.ahead,
            newest_summary: comparison.commits.into_iter().next().map(|commit| commit.summary),
        })
    }

    async fn create(&self, draft: PullDraft) -> Result<Result<Self::Created, IssueNumber>, Self::Error> {
        let now = Timestamp::now();
        let issue = NewIssue {
            repo: self.base.repo.repo.id,
            kind: IssueKind::Pull,
            title: draft.title,
            body: draft.body,
            author: self.actor.id(),
            at: now,
        };
        let new = NewPull {
            issue,
            head_repo: self.head.repo.repo.id,
            head_branch: draft.head_branch,
            base_branch: draft.base_branch,
            head_sha: draft.head_sha,
            is_draft: draft.is_draft,
        };
        Ok(match self.service.ctx().database.pulls().create(&new).await? {
            CreatedPull::Created(pair) => Ok(*pair),
            CreatedPull::Duplicate(number) => Err(number),
        })
    }
}
