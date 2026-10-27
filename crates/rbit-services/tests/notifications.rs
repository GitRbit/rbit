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
use sqlx::{Connection, SqliteConnection, sqlite::SqliteConnectOptions};
use tokio::sync::broadcast::{Receiver, error::TryRecvError};

struct Fixture {
    dir: tempfile::TempDir,
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
        Self { dir, database, services, alice, bob, carol }
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
    let session = fixture.services.accounts.start_session(&fixture.bob).await.unwrap();
    let signed_in = fixture.services.accounts.resume_session(&session).await.unwrap().unwrap();
    assert!(signed_in.has_unread_notifications, "the session carries the header's indicator");
    assert!(!signed_in.password_change_required);

    // Bob reads it; carol comments: alice (author) and bob (assigned) hear about it.
    assert!(notifications.mark_thread_read(fixture.bob.id(), issue.id).await, "the thread was unread");
    assert!(!notifications.mark_thread_read(fixture.bob.id(), issue.id).await, "nothing left to mark: no need to look again");
    assert!(!notifications.has_unread(fixture.bob.id()).await.unwrap());
    assert!(!fixture.services.accounts.resume_session(&session).await.unwrap().unwrap().has_unread_notifications);
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
    let issue = off.services.issues.create(&off.alice, &repo, input).await.unwrap();
    off.deliver(&mut events).await;
    assert!(!off.services.notifications.has_unread(off.bob.id()).await.unwrap());
    // Even an inbox row left from when notifications were on is not indicated or marked.
    let at = rbit_core::Timestamp::now();
    off.database.notifications().notify(off.bob.id(), issue.id, repo.repo.repo.id, NotificationReason::Mention, None, at).await.unwrap();
    let session = off.services.accounts.start_session(&off.bob).await.unwrap();
    assert!(!off.services.accounts.resume_session(&session).await.unwrap().unwrap().has_unread_notifications);
    assert!(!off.services.notifications.mark_thread_read(off.bob.id(), issue.id).await);
}

#[tokio::test]
async fn fanout_keeps_permissions_suspension_and_optouts_across_batch_boundaries() {
    use rbit_core::Timestamp;
    use rbit_storage::sqlite::notifications::NOTIFICATION_BATCH_SIZE;

    let fixture = Fixture::new(Visibility::Private, |_| {}).await;
    let repo = fixture.repo(&fixture.alice).await;
    let issue = fixture
        .services
        .issues
        .create(&fixture.alice, &repo, NewIssueInput { title: "Private batch".into(), ..NewIssueInput::default() })
        .await
        .unwrap();
    let store = fixture.database.notifications();
    let mut users = Vec::new();
    for index in 0..(NOTIFICATION_BATCH_SIZE * 2 + 7) {
        let user = user(&fixture.database, &format!("batch{index}")).await;
        store.participate(issue.id, user.id(), NotificationReason::Mention, Timestamp(1)).await.unwrap();
        if index % 4 != 0 {
            fixture.database.repositories().grant_collaborator(repo.repo.repo.id, user.id(), Permission::Read).await.unwrap();
        }
        if index % 4 == 1 {
            fixture.database.admin().set_suspended(user.id(), true).await.unwrap();
        }
        if index % 4 == 2 {
            store.set_subscribed(issue.id, user.id(), false, Timestamp(2)).await.unwrap();
        }
        users.push(user);
    }
    store.participate(issue.id, fixture.alice.id(), NotificationReason::Author, Timestamp(1)).await.unwrap();
    fixture
        .services
        .notifications
        .handle_event(&DomainEvent::Issue {
            repo: repo.repo.repo.id,
            issue: issue.id,
            number: issue.number,
            kind: IssueKind::Issue,
            actor: Some(fixture.alice.id()),
            action: IssueAction::Closed { reason: StateReason::Completed },
        })
        .await
        .unwrap();
    for (index, user) in users.iter().enumerate() {
        assert_eq!(store.count(user.id(), true).await.unwrap(), u64::from(index % 4 == 3), "recipient {index}");
    }
    assert_eq!(store.count(fixture.alice.id(), true).await.unwrap(), 0, "the actor is still excluded");
}

#[tokio::test]
async fn inbox_batches_refresh_repository_visibility() {
    use rbit_core::Timestamp;
    use rbit_storage::sqlite::notifications::NOTIFICATION_BATCH_SIZE;

    let fixture = Fixture::new(Visibility::Public, |_| {}).await;
    let repo = fixture.repo(&fixture.alice).await;
    let issue = fixture
        .services
        .issues
        .create(&fixture.alice, &repo, NewIssueInput { title: "Visibility changes during fan-out".into(), ..NewIssueInput::default() })
        .await
        .unwrap();
    let store = fixture.database.notifications();
    let mut users = Vec::new();
    for index in 0..=NOTIFICATION_BATCH_SIZE {
        let user = user(&fixture.database, &format!("visibility{index}")).await;
        store.participate(issue.id, user.id(), NotificationReason::Comment, Timestamp(1)).await.unwrap();
        users.push(user);
    }
    let options = SqliteConnectOptions::new().filename(fixture.dir.path().join("rbit.db"));
    let mut connection = SqliteConnection::connect_with(&options).await.unwrap();
    // Switch privacy at the end of the first committed batch, with no timing race.
    // Safe interpolation: only an internal numeric fixture ID and batch-size constant.
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "CREATE TRIGGER privatize_after_inbox_batch AFTER INSERT ON notifications \
         WHEN (SELECT COUNT(*) FROM notifications) = {NOTIFICATION_BATCH_SIZE} \
         BEGIN UPDATE repositories SET visibility = 'private' WHERE id = {}; END",
        repo.repo.repo.id.0,
    )))
    .execute(&mut connection)
    .await
    .unwrap();
    fixture
        .services
        .notifications
        .handle_event(&DomainEvent::Issue {
            repo: repo.repo.repo.id,
            issue: issue.id,
            number: issue.number,
            kind: IssueKind::Issue,
            actor: Some(fixture.alice.id()),
            action: IssueAction::Closed { reason: StateReason::Completed },
        })
        .await
        .unwrap();
    for (index, user) in users.iter().enumerate() {
        assert_eq!(store.count(user.id(), true).await.unwrap(), u64::from(index < NOTIFICATION_BATCH_SIZE), "recipient {index}");
    }
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
        actor: Some(fixture.carol.id()),
        action: CommentAction::Created,
    };
    notifications.handle_event(&deleted).await.unwrap();
    assert_eq!(fixture.unread(&fixture.bob).await.len(), 1, "commenters reach subscribers even when the comment is gone");
    assert_eq!(notifications.prune().await.unwrap(), 0);
}
