//! The timeline shared by issues and pull requests: opening a thread for a viewer,
//! comments, state changes, locking and the merged list of comments and events.
//!
//! The integrator owns this file (T0 in `docs/design/milestone-2b.md`, frozen for the
//! issue and pull request workstreams). It decides permissions with the pure
//! `rbit_collaboration::threads` permission tables, writes through
//! `rbit_storage::sqlite::issues::IssueStore` and publishes, for both kinds,
//! `DomainEvent::IssueComment` (created, edited, deleted) and `DomainEvent::Issue` with
//! `Edited`, `Closed`, `Reopened`, `Locked` and `Unlocked`. Opening a thread
//! (`IssueAction::Opened`) is published by its creator (I or P) once their own data is in
//! place.
//!
//! Creating, editing and commenting spend [`ThrottleScope::Content`] for the actor.

mod compat;
mod human_comments;
mod lifecycle;
mod reader;
mod state_change;
mod task_comments;
mod text_edit;
mod threads;
mod writer;

use rbit_core::{
    Account, AccountId, Comment, CommentId, EventId, Issue, IssueId, IssueKind, IssueNumber, IssuePermissions, RepoId, StateReason,
    TimelineEvent, Timestamp, User,
    issue::{CommentAction, IssueAction, comment_permissions},
};
pub use rbit_storage::sqlite::issues::{TimelineCursor, TimelineItemId};

use crate::{DomainEvent, RepoContext, ServiceError, ThrottleScope, ThrottleService, actions::tokens::TaskGrant};

/// Most entries one [`TimelineService::timeline`] call returns.
pub use rbit_collaboration::timeline::MAX_TIMELINE_PAGE;

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

pub use rbit_collaboration::timeline::{CrossReferenceSource, EventSubjects};
pub type TimelineEntry = rbit_collaboration::timeline::TimelineEntry<TimelineEvent>;
pub type Timeline = rbit_collaboration::timeline::Timeline<TimelineEvent>;

#[derive(Debug, Clone)]
pub struct ThreadServices {
    database: rbit_storage::Database,
    repositories: rbit_git::RepositoryStorage,
    events: crate::EventBus,
    throttle: ThrottleService,
}

/// Transitional name; the implementation is the explicit thread component composition.
pub type TimelineService = ThreadServices;

pub use reader::ThreadReader;
pub use writer::ThreadWriter;

impl ThreadServices {
    pub(crate) fn new(
        database: rbit_storage::Database,
        repositories: rbit_git::RepositoryStorage,
        events: crate::EventBus,
        throttle: ThrottleService,
    ) -> Self {
        Self { database, repositories, events, throttle }
    }

    pub fn reader(&self) -> ThreadReader<'_> {
        ThreadReader { database: &self.database }
    }
    pub fn writer(&self) -> ThreadWriter<'_> {
        ThreadWriter { database: &self.database, repositories: &self.repositories, events: &self.events, throttle: &self.throttle }
    }
}

/// What `actor` may do with the thread, decided afresh from the context's access and
/// row, so a context opened for another viewer never lends its permissions. Changes
/// then pass the archive guard, so writers of an archived repository learn why they may
/// not ([`RepoContext::ensure_writable`]).
fn permissions(actor: &User, ctx: &IssueContext) -> IssuePermissions {
    rbit_collaboration::threads::issue_permissions(Some(actor), ctx.repo.access, &ctx.issue)
}

/// What `viewer` is offered on a thread of `repo`, decided by
/// [`rbit_collaboration::threads::thread_permissions`].
pub(crate) fn thread_permissions(viewer: Option<&User>, repo: &RepoContext, issue: &Issue) -> IssuePermissions {
    rbit_collaboration::threads::thread_permissions(viewer, repo.access, repo.repo.repo.is_archived(), issue)
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
        assert!(matches!(timeline.writer().create(&alice, &repo, IssueKind::Issue, "  ", "").await, Err(ServiceError::Invalid(_))));
        let too_long = "x".repeat(rbit_core::issue::MAX_TEXT_BYTES + 1);
        assert!(matches!(
            timeline.writer().create(&alice, &repo, IssueKind::Issue, "Title", &too_long).await,
            Err(ServiceError::Invalid(_))
        ));
        let issue = timeline.writer().create(&alice, &repo, IssueKind::Issue, " Crash ", "a\r\nb").await.unwrap();
        let pull = timeline.writer().create(&alice, &repo, IssueKind::Pull, "Fix", "").await.unwrap();
        assert_eq!((issue.number.get(), issue.title.as_str(), issue.body.as_str()), (1, "Crash", "a\nb"));
        assert_eq!((pull.number.get(), pull.kind), (2, IssueKind::Pull));
        assert!(events.try_recv().is_err(), "the creator publishes `Opened` once its own data is in place");
        assert_eq!(timeline.open_counts(repo.repo.repo.id).await.unwrap(), OpenCounts { issues: 1, pulls: 1 });
    }
}
