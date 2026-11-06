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

mod human_comments;
mod lifecycle;
mod state_change;
mod task_comments;
mod text_edit;
mod threads;

use std::collections::{HashMap, HashSet};

use rbit_core::{
    Account, AccountId, Comment, CommentId, EventId, Issue, IssueId, IssueKind, IssueNumber, IssuePermissions, IssueState, RepoId,
    StateReason, TimelineEvent, Timestamp, User,
    issue::{CommentAction, IssueAction, comment_permissions, issue_permissions},
};
use rbit_storage::sqlite::issues::StoredTimelineItem;
pub use rbit_storage::sqlite::issues::{TimelineCursor, TimelineItemId};

use crate::{DomainEvent, RepoContext, ServiceContext, ServiceError, ThrottleScope, ThrottleService, actions::tokens::TaskGrant};

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

pub use rbit_collaboration::threads::OpenCounts;

pub use rbit_collaboration::threads::StateChange;

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

        let permissions = thread_permissions(viewer, &ctx.repo, &ctx.issue);
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

    /// Publishes what happened to `comment`; `actor` is `None` for rbit Actions.
    fn publish_comment(&self, ctx: &IssueContext, comment: CommentId, actor: Option<&User>, action: CommentAction) {
        let issue = &ctx.issue;
        self.ctx.events.publish(DomainEvent::IssueComment {
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

/// What `actor` may do with the thread, decided afresh from the context's access and
/// row, so a context opened for another viewer never lends its permissions. Changes
/// then pass the archive guard, so writers of an archived repository learn why they may
/// not ([`RepoContext::ensure_writable`]).
fn permissions(actor: &User, ctx: &IssueContext) -> IssuePermissions {
    issue_permissions(Some(actor), ctx.repo.access, &ctx.issue)
}

/// What `viewer` is offered on a thread of `repo`: [`issue_permissions`], or nothing at
/// all while the repository is archived (it is read-only).
pub(crate) fn thread_permissions(viewer: Option<&User>, repo: &RepoContext, issue: &Issue) -> IssuePermissions {
    if repo.repo.repo.is_archived() { IssuePermissions::default() } else { issue_permissions(viewer, repo.access, issue) }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use rbit_core::{Config, Visibility};
    use rbit_storage::{Database, KvStore, NewUser};

    use super::*;
    use crate::{CreateRepository, Services};

    pub(super) async fn fixture() -> (tempfile::TempDir, Services, User, RepoContext) {
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
}
