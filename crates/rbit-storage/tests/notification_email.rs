//! Email preferences and the notification email outbox (workstream N) against a real,
//! migrated `SQLite` database.

#[path = "common/query_count.rs"]
mod query_count;

use rbit_core::{IssueId, IssueKind, IssueText, IssueTitle, Timestamp, User, Visibility, config::EmailPreference};
use rbit_storage::{
    Database, NewRepository, NewUser,
    sqlite::{issues::NewIssue, notifications::NOTIFICATION_BATCH_SIZE},
};

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

#[tokio::test]
async fn batch_recipients_read_current_verified_accounts_and_preferences() {
    let fixture = Fixture::new().await;
    let store = fixture.database.notification_email();
    let mut ids = Vec::new();
    for index in 0..=NOTIFICATION_BATCH_SIZE {
        let user = user(&fixture.database, &format!("batch{index}")).await;
        fixture.database.recovery().mark_verified(user.id(), &user.email, Timestamp(1)).await.unwrap();
        ids.push(user.id());
    }
    fixture.database.admin().set_suspended(ids[0], true).await.unwrap();
    store.set_preference(ids[1], EmailPreference::Off, Timestamp(1)).await.unwrap();
    store.set_preference(ids[2], EmailPreference::All, Timestamp(1)).await.unwrap();
    let changed_email = "changed@example.com".parse().unwrap();
    fixture.database.profiles().set_email(ids[3], &changed_email).await.unwrap();
    ids.extend([ids[1], fixture.alice.id(), rbit_core::AccountId(i64::MAX)]);

    let counter = query_count::QueryCounter::new();
    let recipients = counter.measure(store.recipients(&ids)).await.unwrap();
    assert_eq!(recipients.len(), NOTIFICATION_BATCH_SIZE - 1);
    assert!(!recipients.iter().any(|recipient| [ids[0], ids[3], fixture.alice.id()].contains(&recipient.user.id())));
    assert_eq!(recipients[0].preference, Some(EmailPreference::Off));
    assert_eq!(recipients[1].preference, Some(EmailPreference::All));
    assert!(recipients[2..].iter().all(|recipient| recipient.preference.is_none()));
    assert_eq!(counter.queries().len(), 2, "reads scale with batches, not users: {:?}", counter.queries());
    fixture.database.recovery().mark_verified(ids[3], &changed_email, Timestamp(2)).await.unwrap();
    let current = store.recipients(&[ids[3]]).await.unwrap();
    assert_eq!(current[0].user.email, changed_email);
}

#[tokio::test]
async fn concurrent_batch_claims_have_one_winner_per_recipient_and_bounded_writes() {
    let fixture = Fixture::new().await;
    let store = fixture.database.notification_email();
    let mut ids = Vec::new();
    for index in 0..=(NOTIFICATION_BATCH_SIZE * 2) {
        ids.push(user(&fixture.database, &format!("batch{index}")).await.id());
    }
    let expected = ids.len();
    ids.extend([ids[0], ids[1]]);
    let first = query_count::QueryCounter::new();
    let second = query_count::QueryCounter::new();
    let (a, b) = tokio::join!(
        first.measure(store.claim_many(&ids, fixture.issue, "same-event", Timestamp(1))),
        second.measure(store.claim_many(&ids, fixture.issue, "same-event", Timestamp(1))),
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    assert!(a.is_disjoint(&b));
    assert_eq!(a.len() + b.len(), expected);
    for counter in [&first, &second] {
        let queries = counter.queries();
        assert_eq!(queries.len(), expected.div_ceil(NOTIFICATION_BATCH_SIZE), "concurrent scopes stay isolated: {queries:?}");
        assert!(
            queries.iter().all(|(sql, rows)| sql.starts_with("INSERT INTO notification_emails ")
                && *rows <= u64::try_from(NOTIFICATION_BATCH_SIZE).unwrap())
        );
    }
    assert!(store.claim_many(&ids, fixture.issue, "same-event", Timestamp(2)).await.unwrap().is_empty());
    let empty = query_count::QueryCounter::new();
    assert!(empty.measure(store.claim_many(&[], fixture.issue, "same-event", Timestamp(2))).await.unwrap().is_empty());
    assert_eq!(empty.queries(), []);
}
