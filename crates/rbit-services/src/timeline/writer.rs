//! Narrow borrowed write composition; stores, events and throttles are explicit.
use super::{
    AccountId, CommentAction, CommentId, DomainEvent, EventId, Issue, IssueAction, IssueContext, IssueId, ServiceError, ThrottleScope,
    ThrottleService, TimelineEvent, Timestamp, User,
};
use crate::EventBus;
use rbit_git::RepositoryStorage;
use rbit_storage::Database;
#[derive(Debug, Clone, Copy)]
pub struct ThreadWriter<'a> {
    pub(super) database: &'a Database,
    pub(super) repositories: &'a RepositoryStorage,
    pub(super) events: &'a EventBus,
    pub(super) throttle: &'a ThrottleService,
}
impl ThreadWriter<'_> {
    /// Records `event` on a thread for the workstreams' own changes (labels, pushes, …).
    pub(crate) async fn record(&self, issue: IssueId, actor: Option<AccountId>, event: TimelineEvent) -> Result<EventId, ServiceError> {
        Ok(self.database.issues().record_event(issue, actor, &event, Timestamp::now()).await?)
    }

    pub(super) fn spend(&self, actor: &User) -> Result<(), ServiceError> {
        self.throttle.check(ThrottleScope::Content, &actor.id().0.to_string())
    }

    pub(super) fn publish_issue(&self, issue: &Issue, actor: Option<AccountId>, action: IssueAction) {
        self.events.publish(DomainEvent::Issue {
            repo: issue.repo_id,
            issue: issue.id,
            number: issue.number,
            kind: issue.kind,
            actor,
            action,
        });
    }

    /// Publishes what happened to `comment`; `actor` is `None` for rbit Actions.
    pub(super) fn publish_comment(&self, ctx: &IssueContext, comment: CommentId, actor: Option<&User>, action: CommentAction) {
        let issue = &ctx.issue;
        self.events.publish(DomainEvent::IssueComment {
            repo: issue.repo_id,
            issue: issue.id,
            number: issue.number,
            kind: issue.kind,
            comment,
            actor: actor.map(User::id),
            action,
        });
    }
}
