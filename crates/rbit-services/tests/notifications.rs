//! Notification fan-out (workstream I) against real stores: who hears about what, and
//! that nobody hears about a repository they cannot read.

use std::sync::Arc;

use rbit_core::{
    Config, IssueKind, IssueNumber, StateReason, User, Visibility,
    issue::{CommentAction, IssueAction},
    membership::Permission,
};
use rbit_services::{
    CreateRepository, DomainEvent, IssueContext, RepoContext, Services, issues::NewIssueInput, notifications::NotificationReason,
    timeline::StateChange,
};
use rbit_storage::{Database, KvStore, NewUser};
use tokio::sync::broadcast::{Receiver, error::TryRecvError};

struct Fixture {
    _dir: tempfile::TempDir,
    database: Database,
    services: Services,
    alice: User,
    bob: User,
    carol: User,
}

async fn user(database: &Database, name: &str) -> User {
    database
        .accounts()
        .create_user(NewUser {
            name: name.parse().unwrap(),
            email: format!("{name}@example.com").parse().unwrap(),
            display_name: None,
            password_hash: "not-a-real-hash".to_owned(),
            is_admin: false,
        })
        .await
        .unwrap()
}

impl Fixture {
    async fn new(visibility: Visibility, configure: impl FnOnce(&mut Config)) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let database = Database::open(&dir.path().join("rbit.db")).await.unwrap();
        let kv = KvStore::open(&dir.path().join("rbit.redb")).unwrap();
        let mut config = Config::default();
        config.storage.data_dir = dir.path().to_path_buf();
        configure(&mut config);
        let services = Services::new(Arc::new(config), &database, &kv).await.unwrap();
        let alice = user(&database, "alice").await;
        let bob = user(&database, "bob").await;
        let carol = user(&database, "carol").await;
        let input = CreateRepository { name: "demo".to_owned(), description: String::new(), visibility };
        services.repositories.create(&alice, input).await.unwrap();
        Self { _dir: dir, database, services, alice, bob, carol }
    }

    async fn repo(&self, viewer: &User) -> RepoContext {
        self.services.repositories.open(Some(viewer), "alice", "demo").await.unwrap()
    }

    async fn open(&self, viewer: &User, number: IssueNumber) -> IssueContext {
        self.services.timeline.open(Some(viewer), self.repo(viewer).await, number).await.unwrap()
    }

    /// Delivers every event published so far to the notification service, as its
    /// background subscriber would.
    async fn deliver(&self, events: &mut Receiver<DomainEvent>) {
        loop {
            match events.try_recv() {
                Ok(event) => self.services.notifications.handle_event(&event).await.unwrap(),
                Err(TryRecvError::Empty | TryRecvError::Closed) => return,
                Err(TryRecvError::Lagged(_)) => panic!("lagged"),
            }
        }
    }

    async fn unread(&self, user: &User) -> Vec<(String, NotificationReason)> {
        let page = self.services.notifications.inbox(user, true, 1).await.unwrap();
        page.notifications.into_iter().map(|n| (n.title, n.reason)).collect()
    }
}

#[tokio::test]
async fn participants_hear_about_activity_but_never_their_own() {
    let fixture = Fixture::new(Visibility::Public, |_| {}).await;
    let notifications = &fixture.services.notifications;
    let mut events = fixture.services.events.subscribe();
    let repo = fixture.repo(&fixture.alice).await;
    fixture.services.collaborators.add(&fixture.alice, &repo, "bob", Permission::Write).await.unwrap();

    let input = NewIssueInput {
        title: "Crash".into(),
        body: "cc @carol, not `@dave` and not @nobody".into(),
        assignees: vec![fixture.bob.id()],
        ..NewIssueInput::default()
    };
    let dave = user(&fixture.database, "dave").await;
    let issue = fixture.services.issues.create(&fixture.alice, &repo, input).await.unwrap();
    fixture.deliver(&mut events).await;
    assert_eq!(fixture.unread(&fixture.alice).await, [], "the actor is never notified");
    assert_eq!(fixture.unread(&fixture.bob).await, [("Crash".to_owned(), NotificationReason::Assigned)]);
    assert_eq!(fixture.unread(&fixture.carol).await, [("Crash".to_owned(), NotificationReason::Mention)]);
    assert_eq!(fixture.unread(&dave).await, [], "mentions in code are ignored");
    assert!(notifications.has_unread(fixture.bob.id()).await.unwrap());

    // Bob reads it; carol comments: alice (author) and bob (assigned) hear about it.
    notifications.mark_thread_read(fixture.bob.id(), issue.id).await;
    assert!(!notifications.has_unread(fixture.bob.id()).await.unwrap());
    let as_carol = fixture.open(&fixture.carol, issue.number).await;
    fixture.services.timeline.comment(&fixture.carol, &as_carol, "Same here, @carol").await.unwrap();
    fixture.deliver(&mut events).await;
    assert_eq!(fixture.unread(&fixture.alice).await, [("Crash".to_owned(), NotificationReason::Author)]);
    assert_eq!(fixture.unread(&fixture.bob).await, [("Crash".to_owned(), NotificationReason::Assigned)], "the strongest reason wins");
    assert_eq!(fixture.unread(&fixture.carol).await, [("Crash".to_owned(), NotificationReason::Mention)], "unchanged: carol acted");

    // Bob unsubscribes: closing the issue reaches alice's inbox only... and carol's.
    notifications.mark_all_read(&fixture.alice).await.unwrap();
    notifications.mark_all_read(&fixture.carol).await.unwrap();
    notifications.mark_thread_read(fixture.bob.id(), issue.id).await;
    notifications.set_subscribed(fixture.bob.id(), issue.id, false).await.unwrap();
    let as_alice = fixture.open(&fixture.alice, issue.number).await;
    fixture.services.timeline.set_state(&fixture.alice, &as_alice, StateChange::Close(StateReason::Completed)).await.unwrap();
    fixture.deliver(&mut events).await;
    assert_eq!(fixture.unread(&fixture.bob).await, [], "an unsubscribe sticks");
    assert_eq!(fixture.unread(&fixture.carol).await, [("Crash".to_owned(), NotificationReason::Mention)]);
    assert_eq!(fixture.unread(&fixture.alice).await, []);

    // A comment editing in a new mention notifies only the newly mentioned.
    let comment = fixture.services.timeline.comment(&fixture.alice, &as_alice, "Thanks").await.unwrap();
    fixture.deliver(&mut events).await;
    notifications.mark_all_read(&fixture.carol).await.unwrap();
    fixture.services.timeline.edit_comment(&fixture.alice, &as_alice, comment.id, "Thanks @dave and @carol").await.unwrap();
    fixture.deliver(&mut events).await;
    assert_eq!(fixture.unread(&dave).await, [("Crash".to_owned(), NotificationReason::Mention)]);
    assert_eq!(fixture.unread(&fixture.carol).await, [], "carol was mentioned before");
}

#[tokio::test]
async fn nobody_without_read_access_is_notified() {
    let fixture = Fixture::new(Visibility::Private, |_| {}).await;
    let mut events = fixture.services.events.subscribe();
    let repo = fixture.repo(&fixture.alice).await;
    fixture.services.collaborators.add(&fixture.alice, &repo, "bob", Permission::Read).await.unwrap();

    let input = NewIssueInput { title: "Secret".into(), body: "@bob @carol".into(), ..NewIssueInput::default() };
    let issue = fixture.services.issues.create(&fixture.alice, &repo, input).await.unwrap();
    fixture.deliver(&mut events).await;
    assert_eq!(fixture.unread(&fixture.bob).await, [("Secret".to_owned(), NotificationReason::Mention)]);
    assert_eq!(fixture.unread(&fixture.carol).await, [], "an outsider mentioned in a private repository hears nothing");
    assert!(!fixture.services.notifications.has_unread(fixture.carol.id()).await.unwrap());

    // Bob loses access: new activity does not reach him, and his old row disappears.
    fixture.services.collaborators.remove(&fixture.alice, &repo, "bob").await.unwrap();
    let ctx = fixture.open(&fixture.alice, issue.number).await;
    fixture.services.timeline.comment(&fixture.alice, &ctx, "Still secret").await.unwrap();
    fixture.deliver(&mut events).await;
    assert_eq!(fixture.unread(&fixture.bob).await, [], "rows of repositories bob cannot read are hidden");
    assert_eq!(fixture.services.notifications.inbox(&fixture.bob, false, 1).await.unwrap().total, 0, "and deleted lazily");
}

#[tokio::test]
async fn suspended_users_and_disabled_notifications() {
    let fixture = Fixture::new(Visibility::Public, |_| {}).await;
    let mut events = fixture.services.events.subscribe();
    let repo = fixture.repo(&fixture.alice).await;
    fixture.database.admin().set_suspended(fixture.bob.id(), true).await.unwrap();
    let input = NewIssueInput { title: "Hello".into(), body: "@bob".into(), ..NewIssueInput::default() };
    fixture.services.issues.create(&fixture.alice, &repo, input).await.unwrap();
    fixture.deliver(&mut events).await;
    assert_eq!(fixture.unread(&fixture.bob).await, []);

    let off = Fixture::new(Visibility::Public, |config| config.issues.notifications = false).await;
    let mut events = off.services.events.subscribe();
    let repo = off.repo(&off.alice).await;
    let input = NewIssueInput { title: "Hello".into(), body: "@bob".into(), ..NewIssueInput::default() };
    off.services.issues.create(&off.alice, &repo, input).await.unwrap();
    off.deliver(&mut events).await;
    assert!(!off.services.notifications.has_unread(off.bob.id()).await.unwrap());
}

#[tokio::test]
async fn reviews_and_review_requests_notify() {
    let fixture = Fixture::new(Visibility::Public, |_| {}).await;
    let repo = fixture.repo(&fixture.alice).await;
    let pull = fixture
        .database
        .issues()
        .create(&rbit_storage::sqlite::issues::NewIssue {
            repo: repo.repo.repo.id,
            kind: IssueKind::Pull,
            title: rbit_core::IssueTitle::parse("Fix").unwrap(),
            body: rbit_core::IssueText::default(),
            author: fixture.alice.id(),
            at: rbit_core::Timestamp::now(),
        })
        .await
        .unwrap();
    let notifications = &fixture.services.notifications;
    let issue_event = |actor: &User, action| DomainEvent::Issue {
        repo: repo.repo.repo.id,
        issue: pull.id,
        number: pull.number,
        kind: IssueKind::Pull,
        actor: Some(actor.id()),
        action,
    };
    notifications.handle_event(&issue_event(&fixture.alice, IssueAction::Opened)).await.unwrap();
    notifications.handle_event(&issue_event(&fixture.alice, IssueAction::ReviewRequested { reviewer: fixture.bob.id() })).await.unwrap();
    assert_eq!(fixture.unread(&fixture.bob).await, [("Fix".to_owned(), NotificationReason::ReviewRequested)]);
    let review = DomainEvent::PullRequestReview {
        repo: repo.repo.repo.id,
        pull: pull.id,
        number: pull.number,
        review: rbit_core::ReviewId(1),
        actor: fixture.bob.id(),
        action: rbit_core::issue::ReviewAction::Submitted,
    };
    notifications.handle_event(&review).await.unwrap();
    assert_eq!(fixture.unread(&fixture.alice).await, [("Fix".to_owned(), NotificationReason::Author)]);
    let deleted = DomainEvent::IssueComment {
        repo: repo.repo.repo.id,
        issue: pull.id,
        number: pull.number,
        kind: IssueKind::Pull,
        comment: rbit_core::CommentId(1),
        actor: fixture.carol.id(),
        action: CommentAction::Created,
    };
    notifications.handle_event(&deleted).await.unwrap();
    assert_eq!(fixture.unread(&fixture.bob).await.len(), 1, "commenters reach subscribers even when the comment is gone");
    assert_eq!(notifications.prune().await.unwrap(), 0);
}
