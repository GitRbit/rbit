//! Static adapters for the participant and thread activity applications: store
//! lookups, repository access of other people, mentions and cross-references.

use rbit_core::access::repo_access;
use rbit_collaboration::issues::{AccountNamePort, ParticipantError, ParticipantPort, ReviewText, ThreadActivityPort};
use rbit_core::{
    Account, AccountId, CommentId, Issue, IssueId, IssueNumber, LabelId, RepoId, User, issue::ReviewId, issue::ReviewState,
    repository::OwnedRepository,
};

use super::{IssueService, Label};
use crate::{ServiceError, repositories::viewer_grants};

pub(super) fn participant_error(error: ParticipantError<ServiceError>) -> ServiceError {
    match error {
        ParticipantError::Store(error) => error,
        ParticipantError::Invalid(message) => ServiceError::Invalid(message),
        ParticipantError::NotFound => ServiceError::NotFound,
    }
}

/// Lookups for one repository; `can_push` uses the person's current grants.
pub(super) struct ParticipantView<'a> {
    pub service: &'a IssueService,
    pub repo: &'a OwnedRepository,
}

/// Account names, for mentions and filters that name people.
pub(super) struct NamesView<'a> {
    pub service: &'a IssueService,
}

impl AccountNamePort for NamesView<'_> {
    type Error = ServiceError;

    async fn accounts_named(&self, names: &[String]) -> Result<Vec<Account>, Self::Error> {
        Ok(self.service.ctx.database.issue_meta().accounts_named(names).await?)
    }
}

impl ParticipantPort for ParticipantView<'_> {
    type Error = ServiceError;

    async fn find_label(&self, repo: RepoId, id: LabelId) -> Result<Option<Label>, Self::Error> {
        Ok(self.service.ctx.database.issue_meta().find_label(repo, id).await?)
    }

    async fn find_active_user(&self, id: AccountId) -> Result<Option<User>, Self::Error> {
        Ok(self.service.ctx.database.accounts().find_active_user(id).await?)
    }

    async fn can_push(&self, user: &User) -> Result<bool, Self::Error> {
        let grants = viewer_grants(&self.service.ctx.database.repositories(), Some(user), self.repo).await?;
        Ok(repo_access(Some(user), self.repo, &grants).can_write())
    }

    async fn assignable(&self, repo: RepoId, prefix: &str, limit: u32) -> Result<Vec<Account>, Self::Error> {
        Ok(self.service.ctx.database.issue_meta().assignable(repo, prefix, limit).await?)
    }

    async fn comment_thread(&self, repo: RepoId, id: CommentId) -> Result<Option<IssueNumber>, Self::Error> {
        Ok(self.service.ctx.database.issue_meta().comment_thread(repo, id).await?.map(|(_, number)| number))
    }
}

/// The subscriber's reads, mention index and cross-reference recorder.
pub(super) struct ActivityView<'a> {
    pub service: &'a IssueService,
}

impl ThreadActivityPort for ActivityView<'_> {
    type Error = ServiceError;
    type Review = ReviewId;

    async fn thread(&self, issue: IssueId) -> Result<Option<Issue>, Self::Error> {
        Ok(self.service.ctx.database.issues().find(issue).await?)
    }

    async fn comment_body(&self, issue: IssueId, comment: CommentId) -> Result<Option<String>, Self::Error> {
        let comment = self.service.ctx.database.issues().find_comment(issue, comment).await?;
        Ok(comment.map(|comment| comment.body.to_string()))
    }

    async fn review(&self, pull: IssueId, review: ReviewId) -> Result<Option<ReviewText>, Self::Error> {
        let row = self.service.ctx.database.reviews().find(pull, review).await?;
        Ok(row.map(|row| ReviewText { submitted: row.state != ReviewState::Pending, body: row.body }))
    }

    async fn review_comments(&self, pull: IssueId, review: ReviewId) -> Result<Vec<String>, Self::Error> {
        let comments = self.service.ctx.database.reviews().comments(pull).await?;
        Ok(comments.into_iter().filter(|comment| comment.review_id == review).map(|comment| comment.body).collect())
    }

    async fn record_mentions(&self, issue: IssueId, text: &str) -> Result<(), Self::Error> {
        Ok(self.service.mentions().record(issue, text).await?)
    }

    async fn record_cross_references(&self, source: &Issue, actor: Option<AccountId>, text: &str) -> Result<(), Self::Error> {
        self.service.references.record_cross_references(source, actor, text).await
    }
}
