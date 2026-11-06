//! Production creation adapter preserves existing grants, SQL and cross-feature effects.

use rbit_collaboration::issues::{IssueCreation, IssueCreationAccess, IssueCreationError, IssueCreationPort, MetadataCreated};
use rbit_core::{AccountId, Issue, IssueId, IssueKind, LabelId, MilestoneId, TimelineEvent, Timestamp, User};

use super::{IssueService, Label, Milestone, NewIssueInput};
use crate::{RepoContext, ServiceError};

struct Creation<'a> {
    service: &'a IssueService,
    actor: &'a User,
    repo: &'a RepoContext,
}

impl IssueCreationPort for Creation<'_> {
    type Error = ServiceError;

    async fn labels(&self, ids: &[LabelId]) -> Result<Vec<Label>, Self::Error> {
        self.service.repo_labels(self.repo.repo.repo.id, ids).await
    }
    async fn assignees(&self, ids: &[AccountId]) -> Result<Vec<User>, Self::Error> {
        self.service.validated_assignees(&self.repo.repo, ids).await
    }
    async fn milestone(&self, id: MilestoneId) -> Result<Option<Milestone>, Self::Error> {
        Ok(self.service.ctx.database.issue_meta().find_milestone(self.repo.repo.repo.id, id).await?)
    }
    async fn create_thread(&self, title: &str, body: &str) -> Result<Issue, Self::Error> {
        self.service.timeline.writer().create(self.actor, self.repo, IssueKind::Issue, title, body).await
    }
    async fn attach(
        &self,
        issue: IssueId,
        labels: &[LabelId],
        assignees: &[AccountId],
        milestone: Option<MilestoneId>,
    ) -> Result<(), Self::Error> {
        self.service.ctx.database.issue_meta().attach(issue, labels, assignees, milestone, self.actor.id(), Timestamp::now()).await?;
        Ok(())
    }
    async fn record(&self, issue: IssueId, event: MetadataCreated) -> Result<(), Self::Error> {
        let event = match event {
            MetadataCreated::Labeled(label) => TimelineEvent::Labeled { label },
            MetadataCreated::Assigned(assignee) => TimelineEvent::Assigned { assignee },
            MetadataCreated::Milestoned { id, title } => TimelineEvent::Milestoned { milestone: id, title },
        };
        self.service.timeline.writer().record(issue, Some(self.actor.id()), event).await?;
        Ok(())
    }
    fn publish_opened(&self, issue: &Issue) {
        self.service.publish(issue, Some(self.actor.id()), rbit_core::issue::IssueAction::Opened);
    }
}

impl IssueService {
    /// Opens a plain issue with ordered metadata validation and existing post-commit effects.
    pub async fn create(&self, actor: &User, repo: &RepoContext, input: NewIssueInput) -> Result<Issue, ServiceError> {
        let bridge = Creation { service: self, actor, repo };
        let access = IssueCreationAccess {
            can_read: repo.access.can_read(),
            can_triage: repo.access.can_write(),
            archived: repo.repo.repo.is_archived(),
        };
        IssueCreation::new(&bridge, access).create(input).await.map_err(|error| match error {
            IssueCreationError::Port(error) => error,
            IssueCreationError::NotFound => ServiceError::NotFound,
            IssueCreationError::Archived => ServiceError::Archived,
            IssueCreationError::Invalid(message) => ServiceError::Invalid(message),
        })
    }
}
