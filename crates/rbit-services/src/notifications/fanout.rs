//! Runtime composition of the feature-owned fan-out: domain events become activities,
//! and concrete stores answer its content and access ports. No recipient policy here.
use super::can_read_with_grants;
use crate::{DomainEvent, ServiceError};
use rbit_collaboration::{comments::CommentId, references::scan_markdown, threads::IssueText};
use rbit_core::{
    AccountId, IssueId, RepoId, Timestamp, User,
    issue::{CommentAction, IssueAction, ReviewAction},
};
use rbit_notifications::{
    Activity, ActivityEvent, MentionedAccount, ThreadText,
    fanout::{FanoutApplication, Notified},
    port::{ActivitySource, RecipientAccess},
};
use rbit_storage::{Database, NotificationInboxStore, StorageError};

/// The activity an event carries, or `None` for events that never notify anyone.
pub(super) fn activity(event: &DomainEvent) -> Option<ActivityEvent> {
    let (repo, issue, actor, activity) = match event {
        DomainEvent::Issue { repo, issue, actor, action, .. } => {
            let activity = match action {
                IssueAction::Opened => Activity::Opened,
                IssueAction::Edited { previous_body: Some(_), .. } => Activity::BodyEdited,
                IssueAction::Assigned { assignee } => Activity::Assigned(*assignee),
                IssueAction::ReviewRequested { reviewer } => Activity::ReviewRequested(*reviewer),
                IssueAction::Closed { .. } | IssueAction::Reopened => Activity::StateChanged,
                _ => return None,
            };
            (*repo, *issue, *actor, activity)
        }
        DomainEvent::IssueComment { repo, issue, comment, actor, action, .. } => {
            let activity = match action {
                CommentAction::Created => Activity::CommentCreated(*comment),
                CommentAction::Edited { .. } => Activity::CommentEdited(*comment),
                CommentAction::Deleted { .. } => return None,
            };
            (*repo, *issue, *actor, activity)
        }
        DomainEvent::PullRequestReview { repo, pull, actor, action: ReviewAction::Submitted, .. } => {
            (*repo, *pull, Some(*actor), Activity::ReviewSubmitted)
        }
        _ => return None,
    };
    Some(ActivityEvent { repo, issue, actor, activity })
}

/// Records `event` through the feature fan-out over the concrete stores.
pub(super) async fn deliver(database: &Database, event: ActivityEvent, now: Timestamp) -> Result<Option<Notified>, ServiceError> {
    let store = NotificationInboxStore::new(database.notifications());
    let view = DatabaseView(database);
    Ok(FanoutApplication::new(&store, &view, &view).deliver(event, now).await?)
}

/// Content and access reads over one database handle; holds no viewer or policy.
struct DatabaseView<'a>(&'a Database);

impl ActivitySource for DatabaseView<'_> {
    type Error = StorageError;
    async fn thread(&self, issue: IssueId) -> Result<Option<ThreadText>, StorageError> {
        Ok(self.0.issues().find(issue).await?.map(|row| ThreadText { author: row.author, body: row.body }))
    }
    async fn assignees(&self, issue: IssueId) -> Result<Vec<AccountId>, StorageError> {
        Ok(self.0.issue_meta().assignees_of(&[issue]).await?.into_iter().map(|(_, account)| account.id).collect())
    }
    async fn comment_body(&self, issue: IssueId, comment: CommentId) -> Result<Option<IssueText>, StorageError> {
        Ok(self.0.issues().find_comment(issue, comment).await?.map(|comment| comment.body))
    }
    async fn mentioned(&self, text: &str) -> Result<Vec<MentionedAccount>, StorageError> {
        let mentions = scan_markdown(text).mentions;
        if mentions.is_empty() {
            return Ok(Vec::new());
        }
        let accounts = self.0.issue_meta().accounts_named(&mentions).await?;
        Ok(accounts.iter().map(|account| MentionedAccount { id: account.id, organization: account.is_organization() }).collect())
    }
}

impl RecipientAccess for DatabaseView<'_> {
    type Error = StorageError;
    /// Ownership, visibility, active users and grants are read afresh for each call.
    async fn readers(&self, repo: RepoId, users: &[AccountId]) -> Result<Option<Vec<AccountId>>, StorageError> {
        let Some(owned) = self.0.admin().find_repository(repo).await? else { return Ok(None) };
        let users = self.0.accounts().find_active_users(users).await?;
        let grant_ids: Vec<_> = users.iter().filter(|user| !user.is_admin && user.id() != owned.repo.owner_id).map(User::id).collect();
        let grants = self.0.repositories().grants_for(repo, owned.repo.owner_id, &grant_ids).await?;
        Ok(Some(users.iter().filter(|user| can_read_with_grants(user, &owned, grants.get(&user.id()))).map(User::id).collect()))
    }
}
