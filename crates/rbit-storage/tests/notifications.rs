//! Thread subscriptions and the notification inbox (workstream I) against a real,
//! migrated SQLite database.

use rbit_core::{IssueId, IssueKind, IssueText, IssueTitle, RepoId, Timestamp, User, Visibility};
use rbit_storage::{
    Database, NewRepository, NewUser,
    sqlite::{
        issues::NewIssue,
        notifications::{NotificationReason as Reason, Subscription},
    },
};

struct Fixture {
    _dir: tempfile::TempDir,
    database: Database,
    alice: User,
    bob: User,
    repo: RepoId,
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
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let database = Database::open(&dir.path().join("rbit.db")).await.unwrap();
        let alice = user(&database, "alice").await;
        let bob = user(&database, "bob").await;
        let new = NewRepository {
            owner_id: alice.id(),
            name: "demo".parse().unwrap(),
            description: None,
            visibility: Visibility::Public,
            default_branch: "main".to_owned(),
        };
        let repo = database.repositories().create(new).await.unwrap().id;
        Self { _dir: dir, database, alice, bob, repo }
    }

    async fn issue(&self, title: &str) -> IssueId {
        let new = NewIssue {
            repo: self.repo,
            kind: IssueKind::Issue,
            title: IssueTitle::parse(title).unwrap(),
            body: IssueText::default(),
            author: self.alice.id(),
            at: Timestamp(1),
        };
        self.database.issues().create(&new).await.unwrap().id
    }
}

#[tokio::test]
async fn participation_strengthens_and_unsubscribing_sticks() {
    let fixture = Fixture::new().await;
    let store = fixture.database.notifications();
    let issue = fixture.issue("Crash").await;
    let bob = fixture.bob.id();

    store.participate(issue, bob, Reason::Comment, Timestamp(1)).await.unwrap();
    store.participate(issue, bob, Reason::Mention, Timestamp(2)).await.unwrap();
    store.participate(issue, bob, Reason::Comment, Timestamp(3)).await.unwrap();
    assert_eq!(store.subscription(issue, bob).await.unwrap(), Some(Subscription { reason: Reason::Mention, subscribed: true }));

    store.set_subscribed(issue, bob, false, Timestamp(4)).await.unwrap();
    store.participate(issue, bob, Reason::Assigned, Timestamp(5)).await.unwrap();
    assert_eq!(
        store.subscription(issue, bob).await.unwrap(),
        Some(Subscription { reason: Reason::Mention, subscribed: false }),
        "an explicit unsubscribe is not undone by taking part"
    );
    assert_eq!(store.subscribers(issue).await.unwrap(), []);
    store.set_subscribed(issue, bob, true, Timestamp(6)).await.unwrap();
    assert_eq!(store.subscribers(issue).await.unwrap(), [(bob, Reason::Mention)]);

    let carol = user(&fixture.database, "carol").await;
    store.set_subscribed(issue, carol.id(), true, Timestamp(7)).await.unwrap();
    assert_eq!(store.subscription(issue, carol.id()).await.unwrap().map(|s| s.reason), Some(Reason::Manual));
    let known = store.known_participants(issue, &[bob, carol.id(), fixture.alice.id()]).await.unwrap();
    assert_eq!(known, [bob, carol.id()]);
}

#[tokio::test]
async fn the_inbox_keeps_one_row_per_thread_with_the_strongest_reason() {
    let fixture = Fixture::new().await;
    let store = fixture.database.notifications();
    let (alice, bob) = (fixture.alice.id(), fixture.bob.id());
    let first = fixture.issue("First").await;
    let second = fixture.issue("Second").await;
    assert!(!store.has_unread(bob).await.unwrap());

    store.notify(bob, first, fixture.repo, Reason::Assigned, Some(alice), Timestamp(10)).await.unwrap();
    store.notify(bob, first, fixture.repo, Reason::Comment, Some(alice), Timestamp(20)).await.unwrap();
    store.notify(bob, second, fixture.repo, Reason::Mention, None, Timestamp(15)).await.unwrap();
    assert!(store.has_unread(bob).await.unwrap());
    assert!(!store.has_unread(alice).await.unwrap());

    let inbox = store.list(bob, true, 10, 0).await.unwrap();
    let rows: Vec<(&str, Reason, i64)> = inbox.iter().map(|n| (n.title.as_str(), n.reason, n.updated_at.0)).collect();
    assert_eq!(rows, [("First", Reason::Assigned, 20), ("Second", Reason::Mention, 15)], "newest first; unread keeps the stronger");
    assert_eq!(inbox[0].full_name(), "alice/demo");
    assert_eq!(store.count(bob, true).await.unwrap(), 2);

    assert!(store.mark_issue_read(bob, first).await.unwrap());
    assert!(!store.mark_issue_read(bob, first).await.unwrap());
    store.notify(bob, first, fixture.repo, Reason::Comment, Some(alice), Timestamp(30)).await.unwrap();
    let row = store.list(bob, false, 10, 0).await.unwrap().into_iter().find(|n| n.issue == first).unwrap();
    assert_eq!((row.reason, row.unread), (Reason::Comment, true), "a read row takes the new reason");

    let id = row.id;
    assert!(store.find(alice, id).await.unwrap().is_none(), "only the owner sees a row");
    assert!(!store.mark_read(alice, id).await.unwrap());
    assert!(store.mark_read(bob, id).await.unwrap());
    assert_eq!(store.mark_all_read(bob, Timestamp(14)).await.unwrap(), 0, "only rows updated before the cut-off");
    assert_eq!(store.mark_all_read(bob, Timestamp(100)).await.unwrap(), 1);
    assert!(!store.has_unread(bob).await.unwrap());
    assert_eq!(store.count(bob, false).await.unwrap(), 2);

    assert_eq!(store.prune_read(Timestamp(20)).await.unwrap(), 1, "read rows older than the cut-off");
    assert_eq!(store.delete(alice, &[id]).await.unwrap(), 0);
    assert_eq!(store.delete(bob, &[id]).await.unwrap(), 1);
    assert_eq!(store.count(bob, false).await.unwrap(), 0);
}
