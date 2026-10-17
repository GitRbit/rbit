//! Email preferences and the notification email outbox (workstream N) against a real,
//! migrated SQLite database.

use rbit_core::{IssueId, IssueKind, IssueText, IssueTitle, Timestamp, User, Visibility, config::EmailPreference};
use rbit_storage::{Database, NewRepository, NewUser, sqlite::issues::NewIssue};

struct Fixture {
    _dir: tempfile::TempDir,
    database: Database,
    alice: User,
    bob: User,
    issue: IssueId,
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
        let new = NewIssue {
            repo,
            kind: IssueKind::Issue,
            title: IssueTitle::parse("Crash").unwrap(),
            body: IssueText::default(),
            author: alice.id(),
            at: Timestamp(1),
        };
        let issue = database.issues().create(&new).await.unwrap().id;
        Self { _dir: dir, database, alice, bob, issue }
    }
}

#[tokio::test]
async fn preferences_are_stored_per_account() {
    let fixture = Fixture::new().await;
    let store = fixture.database.notification_email();
    assert_eq!(store.preference(fixture.alice.id()).await.unwrap(), None, "no choice yet");

    store.set_preference(fixture.alice.id(), EmailPreference::All, Timestamp(5)).await.unwrap();
    store.set_preference(fixture.alice.id(), EmailPreference::Off, Timestamp(6)).await.unwrap();
    assert_eq!(store.preference(fixture.alice.id()).await.unwrap(), Some(EmailPreference::Off), "the latest choice wins");
    assert_eq!(store.preference(fixture.bob.id()).await.unwrap(), None);
}

#[tokio::test]
async fn each_email_is_claimed_once() {
    let fixture = Fixture::new().await;
    let store = fixture.database.notification_email();
    let (alice, bob, issue) = (fixture.alice.id(), fixture.bob.id(), fixture.issue);

    assert!(store.claim(bob, issue, "comment-1", Timestamp(10)).await.unwrap());
    assert!(!store.claim(bob, issue, "comment-1", Timestamp(11)).await.unwrap(), "a second claim fails");
    assert!(store.claim(alice, issue, "comment-1", Timestamp(10)).await.unwrap(), "claims are per recipient");
    assert!(store.claim(bob, issue, "comment-2", Timestamp(20)).await.unwrap(), "and per activity");

    assert_eq!(store.prune(Timestamp(15)).await.unwrap(), 2);
    assert!(!store.claim(bob, issue, "comment-2", Timestamp(30)).await.unwrap(), "newer claims survive pruning");
    assert!(store.claim(bob, issue, "comment-1", Timestamp(30)).await.unwrap(), "pruned claims are forgotten");
}
