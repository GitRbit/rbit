//! The timeline shared by issues and pull requests: opening a thread for a viewer,
//! comments, state changes, locking and the merged list of comments and events.
//!
//! The integrator owns this file (T0 in `docs/design/milestone-2b.md`, frozen for the
//! issue and pull request workstreams). It decides permissions with the pure
//! [`issue_permissions`] and [`comment_permissions`] tables, writes through
//! `rbit_storage::sqlite::issues::IssueStore` and publishes, for both kinds,
//! `DomainEvent::IssueComment` (created, edited, deleted) and `DomainEvent::Issue` with
//! `Edited`, `Closed`, `Reopened`, `Locked` and `Unlocked`. Opening a thread
//! (`IssueAction::Opened`) is published by its creator (I or P) once their own data is in
//! place.
//!
//! Creating, editing and commenting spend [`ThrottleScope::Content`] for the actor.

use std::collections::{HashMap, HashSet};

use rbit_core::{
    Account, AccountId, Comment, CommentId, EventId, Issue, IssueId, IssueKind, IssueNumber, IssuePermissions, IssueState, IssueText,
    IssueTitle, RepoId, StateReason, TimelineEvent, Timestamp, User,
    issue::{CommentAction, IssueAction, comment_permissions, issue_permissions},
};
use rbit_storage::sqlite::issues::{NewIssue, StoredTimelineItem};
pub use rbit_storage::sqlite::issues::{TimelineCursor, TimelineItemId};

use crate::{DomainEvent, RepoContext, ServiceContext, ServiceError, ThrottleScope, ThrottleService};

/// Most entries one [`TimelineService::timeline`] call returns.
pub const MAX_TIMELINE_PAGE: u32 = 200;

/// An issue or pull request opened for one viewer: its repository (already
/// access-checked), the row, its author and what the viewer may do with it.
#[derive(Debug, Clone)]
pub struct IssueContext {
    pub repo: RepoContext,
    pub issue: Issue,
    /// `None` once the author's account is deleted.
    pub author: Option<Account>,
    pub permissions: IssuePermissions,
}

/// Open threads of a repository, for the Issues and Pull requests tabs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OpenCounts {
    pub issues: u64,
    pub pulls: u64,
}

/// A state change a person asks for. Closing as [`StateReason::Merged`] is refused: only
/// merging a pull request produces it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateChange {
    Close(StateReason),
    Reopen,
}

/// One item of a thread's conversation, shaped for a viewer.
#[derive(Debug, Clone)]
pub enum TimelineEntry {
    Comment {
        comment: Comment,
        /// `None` once the account is deleted (a ghost).
        author: Option<Account>,
        can_edit: bool,
        can_delete: bool,
    },
    Event {
        id: EventId,
        /// `None` for system actions and deleted accounts.
        actor: Option<Account>,
        event: TimelineEvent,
        at: Timestamp,
        subjects: EventSubjects,
    },
}

impl TimelineEntry {
    /// Where the entry sits in the timeline (for interleaving other items, such as reviews).
    pub fn cursor(&self) -> TimelineCursor {
        match self {
            Self::Comment { comment, .. } => TimelineCursor { at: comment.created_at, item: TimelineItemId::Comment(comment.id) },
            Self::Event { id, at, .. } => TimelineCursor { at: *at, item: TimelineItemId::Event(*id) },
        }
    }
}

/// What an event names besides its actor: the account (assignee, reviewer; `None` when
/// deleted or not applicable) and, for `CrossReferenced`, the source thread.
#[derive(Debug, Clone, Default)]
pub struct EventSubjects {
    pub account: Option<Account>,
    pub source: Option<CrossReferenceSource>,
}

/// The thread a cross-reference comes from, which the viewer may read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrossReferenceSource {
    /// `owner/name` of its repository.
    pub full_name: String,
    pub number: IssueNumber,
    pub kind: IssueKind,
    pub state: IssueState,
    pub title: String,
}

/// One page of a timeline; `next` continues it.
#[derive(Debug, Clone, Default)]
pub struct Timeline {
    pub entries: Vec<TimelineEntry>,
    pub next: Option<TimelineCursor>,
}

#[derive(Debug, Clone)]
pub struct TimelineService {
    ctx: ServiceContext,
    throttle: ThrottleService,
}

impl TimelineService {
    pub(crate) fn new(ctx: ServiceContext, throttle: ThrottleService) -> Self {
        Self { ctx, throttle }
    }

    /// Thread `number` of `repo` (already access-checked) for `viewer`. `NotFound` when
    /// it does not exist or the viewer may not read the repository.
    pub async fn open(&self, viewer: Option<&User>, repo: RepoContext, number: IssueNumber) -> Result<IssueContext, ServiceError> {
        if !repo.access.can_read() {
            return Err(ServiceError::NotFound);
        }
        let issue = self.ctx.database.issues().find_by_number(repo.repo.repo.id, number).await?.ok_or(ServiceError::NotFound)?;
        let author = match issue.author {
            Some(id) => self.ctx.database.accounts().find_accounts(&[id]).await?.pop(),
            None => None,
        };
        let permissions = issue_permissions(viewer, repo.access, &issue);
        Ok(IssueContext { repo, issue, author, permissions })
    }

    /// Open issues and pull requests of `repo`, in one indexed query.
    pub async fn open_counts(&self, repo: RepoId) -> Result<OpenCounts, ServiceError> {
        let (issues, pulls) = self.ctx.database.issues().open_counts(repo).await?;
        Ok(OpenCounts { issues, pulls })
    }

    /// Inserts a new thread by `actor` (who must be able to read `repo`) after validating
    /// the title and body and spending the actor's content budget. Creates the row only:
    /// the caller adds its own data and then publishes `IssueAction::Opened`.
    pub(crate) async fn create(
        &self,
        actor: &User,
        repo: &RepoContext,
        kind: IssueKind,
        title: &str,
        body: &str,
    ) -> Result<Issue, ServiceError> {
        if !repo.access.can_read() {
            return Err(ServiceError::NotFound);
        }
        let title = IssueTitle::parse(title)?;
        let body = IssueText::body(body)?;
        self.spend(actor)?;
        let new = NewIssue { repo: repo.repo.repo.id, kind, title, body, author: actor.id(), at: Timestamp::now() };
        Ok(self.ctx.database.issues().create(&new).await?)
    }

    /// Changes the title and/or the body (`None` keeps it). Publishes `Edited` with the
    /// previous values of what changed; a change to the same text publishes nothing.
    pub async fn edit(&self, actor: &User, ctx: &IssueContext, title: Option<&str>, body: Option<&str>) -> Result<Issue, ServiceError> {
        if !permissions(actor, ctx).can_edit {
            return Err(ServiceError::Forbidden);
        }
        let title = title.map(IssueTitle::parse).transpose()?;
        let body = body.map(IssueText::body).transpose()?;
        self.spend(actor)?;
        let issue = &ctx.issue;
        let updated = self
            .ctx
            .database
            .issues()
            .update_text(issue.id, title.as_ref(), body.as_ref(), actor.id(), Timestamp::now())
            .await?
            .ok_or(ServiceError::NotFound)?;
        let previous_title = (updated.title != issue.title).then(|| issue.title.to_string());
        let previous_body = (updated.body != issue.body).then(|| issue.body.to_string());
        if previous_title.is_some() || previous_body.is_some() {
            self.publish_issue(&updated, Some(actor.id()), IssueAction::Edited { previous_title, previous_body, previous_base: None });
        }
        Ok(updated)
    }

    /// Closes (as completed or not planned) or reopens the thread. Reopening a pull
    /// request first asks `pulls::lifecycle::check_reopen`. Asking for the state it is
    /// already in changes nothing and publishes nothing.
    pub async fn set_state(&self, actor: &User, ctx: &IssueContext, change: StateChange) -> Result<Issue, ServiceError> {
        if !permissions(actor, ctx).can_set_state {
            return Err(ServiceError::Forbidden);
        }
        let (state, reason, event, action) = match change {
            StateChange::Close(StateReason::Merged) => {
                return Err(ServiceError::Invalid("only merging a pull request closes it as merged".to_owned()));
            }
            StateChange::Close(reason) => (
                IssueState::Closed,
                Some(reason),
                TimelineEvent::Closed { reason, commit: None, pull: None },
                IssueAction::Closed { reason },
            ),
            StateChange::Reopen => {
                if ctx.issue.is_pull() {
                    crate::pulls::lifecycle::check_reopen(&self.ctx, &ctx.issue).await?;
                }
                (IssueState::Open, None, TimelineEvent::Reopened, IssueAction::Reopened)
            }
        };
        let store = self.ctx.database.issues();
        match store.set_state(ctx.issue.id, state, reason, Some(actor.id()), &event, Timestamp::now()).await? {
            Some(updated) => {
                self.publish_issue(&updated, Some(actor.id()), action);
                Ok(updated)
            }
            None => store.find(ctx.issue.id).await?.ok_or(ServiceError::NotFound),
        }
    }

    /// Closes a thread on behalf of the system: by a pushed commit (I) or by a merge (P).
    /// No permission check and no throttle; records `event` and publishes
    /// `IssueAction::Closed { reason }`. `false` when it was already closed (or is gone).
    pub(crate) async fn close_as_system(
        &self,
        issue: IssueId,
        actor: Option<AccountId>,
        reason: StateReason,
        event: TimelineEvent,
    ) -> Result<bool, ServiceError> {
        let store = self.ctx.database.issues();
        let Some(updated) = store.set_state(issue, IssueState::Closed, Some(reason), actor, &event, Timestamp::now()).await? else {
            return Ok(false);
        };
        self.publish_issue(&updated, actor, IssueAction::Closed { reason });
        Ok(true)
    }

    /// Locks or unlocks the conversation (writers only). Publishes `Locked`/`Unlocked`
    /// when it changed.
    pub async fn set_locked(&self, actor: &User, ctx: &IssueContext, locked: bool) -> Result<(), ServiceError> {
        if !permissions(actor, ctx).can_lock {
            return Err(ServiceError::Forbidden);
        }
        if self.ctx.database.issues().set_locked(ctx.issue.id, locked, actor.id(), Timestamp::now()).await? {
            let action = if locked { IssueAction::Locked } else { IssueAction::Unlocked };
            self.publish_issue(&ctx.issue, Some(actor.id()), action);
        }
        Ok(())
    }

    /// Adds a comment by `actor`.
    pub async fn comment(&self, actor: &User, ctx: &IssueContext, body: &str) -> Result<Comment, ServiceError> {
        if !permissions(actor, ctx).can_comment {
            return Err(ServiceError::Forbidden);
        }
        let body = IssueText::comment(body)?;
        self.spend(actor)?;
        let comment = self
            .ctx
            .database
            .issues()
            .add_comment(ctx.issue.id, actor.id(), &body, Timestamp::now())
            .await?
            .ok_or(ServiceError::NotFound)?;
        self.publish_comment(ctx, comment.id, actor, CommentAction::Created);
        Ok(comment)
    }

    /// Replaces the text of the actor's own comment `id` on this thread.
    pub async fn edit_comment(&self, actor: &User, ctx: &IssueContext, id: CommentId, body: &str) -> Result<Comment, ServiceError> {
        let store = self.ctx.database.issues();
        let comment = store.find_comment(ctx.issue.id, id).await?.ok_or(ServiceError::NotFound)?;
        if !comment_permissions(Some(actor), permissions(actor, ctx), &comment).can_edit {
            return Err(ServiceError::Forbidden);
        }
        let body = IssueText::comment(body)?;
        if body == comment.body {
            return Ok(comment);
        }
        self.spend(actor)?;
        let edited = store.edit_comment(id, &body, Timestamp::now()).await?.ok_or(ServiceError::NotFound)?;
        self.publish_comment(ctx, id, actor, CommentAction::Edited { previous_body: comment.body.into() });
        Ok(edited)
    }

    /// Deletes comment `id` on this thread: the actor's own, or anyone's for moderators.
    pub async fn delete_comment(&self, actor: &User, ctx: &IssueContext, id: CommentId) -> Result<(), ServiceError> {
        let store = self.ctx.database.issues();
        let comment = store.find_comment(ctx.issue.id, id).await?.ok_or(ServiceError::NotFound)?;
        if !comment_permissions(Some(actor), permissions(actor, ctx), &comment).can_delete {
            return Err(ServiceError::Forbidden);
        }
        if let Some(snapshot) = store.delete_comment(id).await? {
            self.publish_comment(ctx, id, actor, CommentAction::Deleted { snapshot });
        }
        Ok(())
    }

    /// Comment `id` on this thread and its author (`None` once the account is deleted),
    /// as [`Self::timeline`] shows it. `ctx` must have been opened for the viewer.
    pub async fn find_comment(&self, ctx: &IssueContext, id: CommentId) -> Result<(Comment, Option<Account>), ServiceError> {
        let comment = self.ctx.database.issues().find_comment(ctx.issue.id, id).await?.ok_or(ServiceError::NotFound)?;
        let author = match comment.author {
            Some(author) => self.ctx.database.accounts().find_accounts(&[author]).await?.into_iter().next(),
            None => None,
        };
        Ok((comment, author))
    }

    /// At most `limit` (1..=[`MAX_TIMELINE_PAGE`]) entries after `cursor`, with what
    /// `viewer` may do with each comment and the accounts and sources events name.
    /// `CrossReferenced` events whose source the viewer cannot read are dropped here.
    /// `ctx` must have been opened for `viewer` (its access decides the permissions).
    pub async fn timeline(
        &self,
        viewer: Option<&User>,
        ctx: &IssueContext,
        cursor: Option<TimelineCursor>,
        limit: u32,
    ) -> Result<Timeline, ServiceError> {
        let store = self.ctx.database.issues();
        let page = store.timeline(ctx.issue.id, cursor, limit.clamp(1, MAX_TIMELINE_PAGE)).await?;

        let mut account_ids = HashSet::new();
        let mut source_ids = HashSet::new();
        for item in &page.items {
            if let StoredTimelineItem::Event { event, .. } = item {
                account_ids.extend(event.subject_account());
                if let TimelineEvent::CrossReferenced { source } = event {
                    source_ids.insert(*source);
                }
            }
        }
        let account_ids: Vec<AccountId> = account_ids.into_iter().collect();
        let source_ids: Vec<IssueId> = source_ids.into_iter().collect();
        let account_store = self.ctx.database.accounts();
        let (accounts, sources) = tokio::try_join!(
            account_store.find_accounts(&account_ids),
            store.readable_sources(&source_ids, viewer.map(User::id), viewer.is_some_and(|v| v.is_admin)),
        )?;
        let accounts: HashMap<AccountId, Account> = accounts.into_iter().map(|account| (account.id, account)).collect();
        let sources: HashMap<IssueId, CrossReferenceSource> = sources
            .into_iter()
            .map(|source| {
                let issue = source.issue;
                let reference = CrossReferenceSource {
                    full_name: source.full_name,
                    number: issue.number,
                    kind: issue.kind,
                    state: issue.state,
                    title: issue.title,
                };
                (issue.id, reference)
            })
            .collect();

        let permissions = issue_permissions(viewer, ctx.repo.access, &ctx.issue);
        let entries = page
            .items
            .into_iter()
            .filter_map(|item| match item {
                StoredTimelineItem::Comment { comment, author } => {
                    let allowed = comment_permissions(viewer, permissions, &comment);
                    Some(TimelineEntry::Comment { comment, author, can_edit: allowed.can_edit, can_delete: allowed.can_delete })
                }
                StoredTimelineItem::Event { id, actor, event, at, .. } => {
                    let source = match &event {
                        TimelineEvent::CrossReferenced { source } => Some(sources.get(source).cloned()?),
                        _ => None,
                    };
                    let account = event.subject_account().and_then(|id| accounts.get(&id).cloned());
                    Some(TimelineEntry::Event { id, actor, event, at, subjects: EventSubjects { account, source } })
                }
            })
            .collect();
        Ok(Timeline { entries, next: page.next })
    }

    /// Records `event` on a thread for the workstreams' own changes (labels, pushes, …).
    pub(crate) async fn record(&self, issue: IssueId, actor: Option<AccountId>, event: TimelineEvent) -> Result<EventId, ServiceError> {
        Ok(self.ctx.database.issues().record_event(issue, actor, &event, Timestamp::now()).await?)
    }

    fn spend(&self, actor: &User) -> Result<(), ServiceError> {
        self.throttle.check(ThrottleScope::Content, &actor.id().0.to_string())
    }

    fn publish_issue(&self, issue: &Issue, actor: Option<AccountId>, action: IssueAction) {
        self.ctx.events.publish(DomainEvent::Issue {
            repo: issue.repo_id,
            issue: issue.id,
            number: issue.number,
            kind: issue.kind,
            actor,
            action,
        });
    }

    fn publish_comment(&self, ctx: &IssueContext, comment: CommentId, actor: &User, action: CommentAction) {
        let issue = &ctx.issue;
        self.ctx.events.publish(DomainEvent::IssueComment {
            repo: issue.repo_id,
            issue: issue.id,
            number: issue.number,
            kind: issue.kind,
            comment,
            actor: actor.id(),
            action,
        });
    }
}

/// What `actor` may do with the thread, decided afresh from the context's access and
/// row, so a context opened for another viewer never lends its permissions.
fn permissions(actor: &User, ctx: &IssueContext) -> IssuePermissions {
    issue_permissions(Some(actor), ctx.repo.access, &ctx.issue)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use rbit_core::{Config, Visibility};
    use rbit_storage::{Database, KvStore, NewUser};

    use super::*;
    use crate::{CreateRepository, Services};

    async fn fixture() -> (tempfile::TempDir, Services, User, RepoContext) {
        let dir = tempfile::tempdir().unwrap();
        let database = Database::open(&dir.path().join("rbit.db")).await.unwrap();
        let kv = KvStore::open(&dir.path().join("rbit.redb")).unwrap();
        let mut config = Config::default();
        config.storage.data_dir = dir.path().to_path_buf();
        let services = Services::new(Arc::new(config), &database, &kv).await.unwrap();
        let alice = database
            .accounts()
            .create_user(NewUser {
                name: "alice".parse().unwrap(),
                email: "alice@example.com".parse().unwrap(),
                display_name: None,
                password_hash: "not-a-real-hash".to_owned(),
                is_admin: false,
            })
            .await
            .unwrap();
        let input = CreateRepository { name: "demo".to_owned(), description: String::new(), visibility: Visibility::Public };
        services.repositories.create(&alice, input).await.unwrap();
        let repo = services.repositories.open(Some(&alice), "alice", "demo").await.unwrap();
        (dir, services, alice, repo)
    }

    #[tokio::test]
    async fn creating_validates_and_numbers_without_publishing() {
        let (_dir, services, alice, repo) = fixture().await;
        let mut events = services.events.subscribe();
        let timeline = &services.timeline;
        assert!(matches!(timeline.create(&alice, &repo, IssueKind::Issue, "  ", "").await, Err(ServiceError::Invalid(_))));
        let too_long = "x".repeat(rbit_core::issue::MAX_TEXT_BYTES + 1);
        assert!(matches!(timeline.create(&alice, &repo, IssueKind::Issue, "Title", &too_long).await, Err(ServiceError::Invalid(_))));
        let issue = timeline.create(&alice, &repo, IssueKind::Issue, " Crash ", "a\r\nb").await.unwrap();
        let pull = timeline.create(&alice, &repo, IssueKind::Pull, "Fix", "").await.unwrap();
        assert_eq!((issue.number.get(), issue.title.as_str(), issue.body.as_str()), (1, "Crash", "a\nb"));
        assert_eq!((pull.number.get(), pull.kind), (2, IssueKind::Pull));
        assert!(events.try_recv().is_err(), "the creator publishes `Opened` once its own data is in place");
        assert_eq!(timeline.open_counts(repo.repo.repo.id).await.unwrap(), OpenCounts { issues: 1, pulls: 1 });
    }

    #[tokio::test]
    async fn system_closes_record_their_event_once() {
        let (_dir, services, alice, repo) = fixture().await;
        let timeline = &services.timeline;
        let issue = timeline.create(&alice, &repo, IssueKind::Issue, "Crash", "").await.unwrap();
        let pull = timeline.create(&alice, &repo, IssueKind::Pull, "Fix", "").await.unwrap();
        let mut events = services.events.subscribe();

        let by_commit = TimelineEvent::Closed { reason: StateReason::Completed, commit: Some("abc1234".to_owned()), pull: None };
        assert!(timeline.close_as_system(issue.id, None, StateReason::Completed, by_commit.clone()).await.unwrap());
        assert!(!timeline.close_as_system(issue.id, None, StateReason::Completed, by_commit.clone()).await.unwrap(), "already closed");
        let merged = TimelineEvent::Merged { commit: "def5678".to_owned(), strategy: Some(rbit_core::issue::MergeStrategy::Squash) };
        assert!(timeline.close_as_system(pull.id, Some(alice.id()), StateReason::Merged, merged).await.unwrap());
        assert!(!timeline.close_as_system(IssueId(999), None, StateReason::Completed, by_commit.clone()).await.unwrap());
        let event_id = timeline.record(issue.id, None, TimelineEvent::Locked).await.unwrap();
        assert!(event_id.0 > 0);

        let published: Vec<_> = std::iter::from_fn(|| events.try_recv().ok()).collect();
        assert_eq!(published.len(), 2);
        assert!(matches!(
            &published[0],
            DomainEvent::Issue { actor: None, kind: IssueKind::Issue, action: IssueAction::Closed { reason: StateReason::Completed }, .. }
        ));
        assert!(matches!(&published[1], DomainEvent::Issue { action: IssueAction::Closed { reason: StateReason::Merged }, .. }));

        let ctx = timeline.open(Some(&alice), repo.clone(), issue.number).await.unwrap();
        assert_eq!((ctx.issue.state_reason, ctx.issue.closed_by), (Some(StateReason::Completed), None));
        let entries = timeline.timeline(Some(&alice), &ctx, None, 10).await.unwrap().entries;
        let recorded: Vec<_> = entries
            .iter()
            .filter_map(|entry| if let TimelineEntry::Event { event, .. } = entry { Some(event.clone()) } else { None })
            .collect();
        assert_eq!(recorded, [by_commit, TimelineEvent::Locked]);
        let merged_pull = timeline.open(Some(&alice), repo, pull.number).await.unwrap();
        assert_eq!(merged_pull.issue.state_reason, Some(StateReason::Merged));
    }
}
