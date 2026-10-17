//! The shared issue core (T0) against a real, migrated SQLite database: numbering,
//! comments, state, locking and the merged timeline.

use rbit_core::{
    AccountId, CommentId, EventId, IssueId, IssueKind, IssueNumber, IssueState, IssueText, IssueTitle, LabelId, RepoId, StateReason,
    TimelineEvent, Timestamp, User, Visibility, issue::LabelSnapshot,
};
use rbit_storage::{
    Database, NewRepository, NewUser,
    sqlite::issues::{IssueStore, NewIssue, StoredTimelineItem, TimelineCursor, TimelineItemId},
};
use sqlx::sqlite::SqlitePoolOptions;

struct Fixture {
    dir: tempfile::TempDir,
    database: Database,
    alice: User,
    repo: RepoId,
}

impl Fixture {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let database = Database::open(&dir.path().join("rbit.db")).await.unwrap();
        let alice = user(&database, "alice").await;
        let repo = repo(&database, &alice, "demo", Visibility::Public).await;
        Self { dir, database, alice, repo }
    }

    fn store(&self) -> IssueStore {
        self.database.issues()
    }

    fn new_issue(&self, kind: IssueKind, title: &str, at: i64) -> NewIssue {
        NewIssue {
            repo: self.repo,
            kind,
            title: IssueTitle::parse(title).unwrap(),
            body: IssueText::body("Body").unwrap(),
            author: self.alice.id(),
            at: Timestamp(at),
        }
    }

    /// Runs raw SQL against the same file, for rows the store would never write.
    async fn execute(&self, sql: &str) {
        let pool = SqlitePoolOptions::new().connect(&format!("sqlite:{}", self.dir.path().join("rbit.db").display())).await.unwrap();
        sqlx::query(sqlx::AssertSqlSafe(sql.to_owned())).execute(&pool).await.unwrap();
        pool.close().await;
    }
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

async fn repo(database: &Database, owner: &User, name: &str, visibility: Visibility) -> RepoId {
    database
        .repositories()
        .create(NewRepository {
            owner_id: owner.id(),
            name: name.parse().unwrap(),
            description: None,
            visibility,
            default_branch: "main".to_owned(),
        })
        .await
        .unwrap()
        .id
}

fn number(n: u32) -> IssueNumber {
    IssueNumber::new(n).unwrap()
}

fn text(body: &str) -> IssueText {
    IssueText::comment(body).unwrap()
}

#[tokio::test]
async fn issues_and_pulls_share_one_sequence_per_repository() {
    let fixture = Fixture::new().await;
    let store = fixture.store();
    let first = store.create(&fixture.new_issue(IssueKind::Issue, "Crash", 10)).await.unwrap();
    let second = store.create(&fixture.new_issue(IssueKind::Pull, "Fix the crash", 11)).await.unwrap();
    assert_eq!((first.number.get(), second.number.get()), (1, 2));
    assert_eq!(first.kind, IssueKind::Issue);
    assert_eq!(second.kind, IssueKind::Pull);
    assert_eq!(first.state, IssueState::Open);
    assert_eq!((first.created_at, first.updated_at), (Timestamp(10), Timestamp(10)));
    assert_eq!(first.author, Some(fixture.alice.id()));
    assert_eq!(first.body.as_str(), "Body");

    let other = repo(&fixture.database, &fixture.alice, "other", Visibility::Public).await;
    let elsewhere = store.create(&NewIssue { repo: other, ..fixture.new_issue(IssueKind::Issue, "Elsewhere", 12) }).await.unwrap();
    assert_eq!(elsewhere.number.get(), 1, "numbers are per repository");

    assert_eq!(store.find(first.id).await.unwrap().unwrap(), first);
    assert_eq!(store.find_by_number(fixture.repo, number(2)).await.unwrap().unwrap(), second);
    assert!(store.find_by_number(fixture.repo, number(3)).await.unwrap().is_none());
    assert!(store.find_by_number(other, number(2)).await.unwrap().is_none());
    assert!(store.find(IssueId(999)).await.unwrap().is_none());
}

#[tokio::test]
async fn concurrent_creation_yields_distinct_numbers() {
    let fixture = Fixture::new().await;
    let tasks: Vec<_> = (0..24)
        .map(|i| {
            let store = fixture.store();
            let new = fixture.new_issue(if i % 3 == 0 { IssueKind::Pull } else { IssueKind::Issue }, &format!("Issue {i}"), 100);
            tokio::spawn(async move { store.create(&new).await.unwrap().number.get() })
        })
        .collect();
    let mut numbers = Vec::new();
    for task in tasks {
        numbers.push(task.await.unwrap());
    }
    numbers.sort_unstable();
    assert_eq!(numbers, (1..=24).collect::<Vec<_>>());
}

#[tokio::test]
async fn numbers_are_never_reused_and_pulls_are_never_deleted() {
    let fixture = Fixture::new().await;
    let store = fixture.store();
    let one = store.create(&fixture.new_issue(IssueKind::Issue, "One", 1)).await.unwrap();
    let two = store.create(&fixture.new_issue(IssueKind::Issue, "Two", 1)).await.unwrap();
    let pull = store.create(&fixture.new_issue(IssueKind::Pull, "Three", 1)).await.unwrap();
    store.add_comment(two.id, fixture.alice.id(), &text("bye"), Timestamp(2)).await.unwrap();

    let deleted = store.delete(two.id).await.unwrap().unwrap();
    assert_eq!(deleted.number.get(), 2);
    assert!(store.find(two.id).await.unwrap().is_none());
    assert!(store.delete(two.id).await.unwrap().is_none(), "already gone");
    assert!(store.delete(pull.id).await.unwrap().is_none(), "pull requests cannot be deleted");
    assert!(store.find(pull.id).await.unwrap().is_some());

    let four = store.create(&fixture.new_issue(IssueKind::Issue, "Four", 3)).await.unwrap();
    assert_eq!(four.number.get(), 4, "number 2 stays taken");
    assert_eq!(one.number.get(), 1);
}

#[tokio::test]
async fn comments_are_counted_and_move_updated_at_forward() {
    let fixture = Fixture::new().await;
    let store = fixture.store();
    let issue = store.create(&fixture.new_issue(IssueKind::Issue, "Count me", 10)).await.unwrap();
    let alice = fixture.alice.id();

    let first = store.add_comment(issue.id, alice, &text("first"), Timestamp(20)).await.unwrap().unwrap();
    let second = store.add_comment(issue.id, alice, &text("second"), Timestamp(15)).await.unwrap().unwrap();
    let stored = store.find(issue.id).await.unwrap().unwrap();
    assert_eq!(stored.comment_count, 2);
    assert_eq!(stored.updated_at, Timestamp(20), "never moves backwards");
    assert_eq!(first.edited_at, None);
    assert!(store.add_comment(IssueId(999), alice, &text("x"), Timestamp(1)).await.unwrap().is_none());

    assert_eq!(store.find_comment(issue.id, first.id).await.unwrap().unwrap(), first);
    let other = store.create(&fixture.new_issue(IssueKind::Issue, "Other", 10)).await.unwrap();
    assert!(store.find_comment(other.id, first.id).await.unwrap().is_none(), "scoped to the thread");

    let edited = store.edit_comment(first.id, &text("first, edited"), Timestamp(30)).await.unwrap().unwrap();
    assert_eq!(edited.body.as_str(), "first, edited");
    assert_eq!((edited.edited_at, edited.updated_at, edited.created_at), (Some(Timestamp(30)), Timestamp(30), Timestamp(20)));
    assert_eq!(store.find(issue.id).await.unwrap().unwrap().updated_at, Timestamp(30));
    assert!(store.edit_comment(CommentId(999), &text("x"), Timestamp(31)).await.unwrap().is_none());

    assert_eq!(store.delete_comment(second.id).await.unwrap().unwrap().body.as_str(), "second");
    assert!(store.delete_comment(second.id).await.unwrap().is_none());
    assert_eq!(store.find(issue.id).await.unwrap().unwrap().comment_count, 1);
}

#[tokio::test]
async fn renaming_records_an_event_in_the_same_transaction() {
    let fixture = Fixture::new().await;
    let store = fixture.store();
    let issue = store.create(&fixture.new_issue(IssueKind::Issue, "Old", 10)).await.unwrap();
    let alice = fixture.alice.id();
    let new_title = IssueTitle::parse("New").unwrap();
    let new_body = IssueText::body("New body").unwrap();

    let updated = store.update_text(issue.id, Some(&new_title), Some(&new_body), alice, Timestamp(20)).await.unwrap().unwrap();
    assert_eq!((updated.title.as_str(), updated.body.as_str(), updated.updated_at), ("New", "New body", Timestamp(20)));
    // The same text again changes nothing and records nothing.
    let same = store.update_text(issue.id, Some(&new_title), Some(&new_body), alice, Timestamp(30)).await.unwrap().unwrap();
    assert_eq!(same.updated_at, Timestamp(20));
    // A body-only edit records no event.
    store.update_text(issue.id, None, Some(&IssueText::body("").unwrap()), alice, Timestamp(40)).await.unwrap();
    assert!(store.update_text(IssueId(999), Some(&new_title), None, alice, Timestamp(1)).await.unwrap().is_none());

    let page = store.timeline(issue.id, None, 50).await.unwrap();
    let events: Vec<_> = page
        .items
        .iter()
        .map(|item| match item {
            StoredTimelineItem::Event { event, actor, .. } => (event.clone(), actor.as_ref().map(|a| a.name.to_string())),
            StoredTimelineItem::Comment { .. } => panic!("no comments"),
        })
        .collect();
    assert_eq!(events, [(TimelineEvent::Renamed { from: "Old".to_owned(), to: "New".to_owned() }, Some("alice".to_owned()))]);
}

#[tokio::test]
async fn state_changes_are_conditional_and_explained() {
    let fixture = Fixture::new().await;
    let store = fixture.store();
    let issue = store.create(&fixture.new_issue(IssueKind::Issue, "Close me", 10)).await.unwrap();
    let alice = Some(fixture.alice.id());
    let closed_event = TimelineEvent::Closed { reason: StateReason::Completed, commit: None, pull: None };

    let closed = store
        .set_state(issue.id, IssueState::Closed, Some(StateReason::Completed), alice, &closed_event, Timestamp(20))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        (closed.state, closed.state_reason, closed.closed_at, closed.closed_by),
        (IssueState::Closed, Some(StateReason::Completed), Some(Timestamp(20)), alice)
    );
    assert!(
        store
            .set_state(issue.id, IssueState::Closed, Some(StateReason::NotPlanned), alice, &closed_event, Timestamp(21))
            .await
            .unwrap()
            .is_none(),
        "already closed: nothing changes"
    );
    assert_eq!(store.find(issue.id).await.unwrap().unwrap().state_reason, Some(StateReason::Completed));

    let reopened =
        store.set_state(issue.id, IssueState::Open, None, alice, &TimelineEvent::Reopened, Timestamp(30)).await.unwrap().unwrap();
    assert_eq!((reopened.state, reopened.state_reason, reopened.closed_at, reopened.closed_by), (IssueState::Open, None, None, None));
    assert!(store.set_state(issue.id, IssueState::Open, None, alice, &TimelineEvent::Reopened, Timestamp(31)).await.unwrap().is_none());

    // The schema refuses what the domain forbids: merged issues, closing without a reason.
    let merged = TimelineEvent::Closed { reason: StateReason::Merged, commit: None, pull: None };
    assert!(store.set_state(issue.id, IssueState::Closed, Some(StateReason::Merged), alice, &merged, Timestamp(40)).await.is_err());
    assert!(store.set_state(issue.id, IssueState::Closed, None, alice, &closed_event, Timestamp(40)).await.is_err());
    assert_eq!(store.find(issue.id).await.unwrap().unwrap().state, IssueState::Open, "the failed attempts left no trace");

    let pull = store.create(&fixture.new_issue(IssueKind::Pull, "Merge me", 10)).await.unwrap();
    let merged_pull =
        store.set_state(pull.id, IssueState::Closed, Some(StateReason::Merged), None, &merged, Timestamp(50)).await.unwrap().unwrap();
    assert_eq!((merged_pull.state_reason, merged_pull.closed_by), (Some(StateReason::Merged), None), "system closes have no actor");

    let kinds: Vec<_> = store
        .timeline(issue.id, None, 50)
        .await
        .unwrap()
        .items
        .iter()
        .map(|item| match item {
            StoredTimelineItem::Event { event, .. } => event.kind(),
            StoredTimelineItem::Comment { .. } => "comment",
        })
        .collect();
    assert_eq!(kinds, ["closed", "reopened"]);
}

#[tokio::test]
async fn locking_toggles_once_with_events() {
    let fixture = Fixture::new().await;
    let store = fixture.store();
    let issue = store.create(&fixture.new_issue(IssueKind::Issue, "Heated", 10)).await.unwrap();
    let alice = fixture.alice.id();
    assert!(store.set_locked(issue.id, true, alice, Timestamp(20)).await.unwrap());
    assert!(!store.set_locked(issue.id, true, alice, Timestamp(21)).await.unwrap());
    assert!(store.find(issue.id).await.unwrap().unwrap().locked);
    assert!(store.set_locked(issue.id, false, alice, Timestamp(22)).await.unwrap());
    let stored = store.find(issue.id).await.unwrap().unwrap();
    assert!(!stored.locked);
    assert_eq!(stored.updated_at, Timestamp(22));
    store.touch(issue.id, Timestamp(5)).await.unwrap();
    assert_eq!(store.find(issue.id).await.unwrap().unwrap().updated_at, Timestamp(22), "touch never moves backwards");
    store.touch(issue.id, Timestamp(60)).await.unwrap();
    assert_eq!(store.find(issue.id).await.unwrap().unwrap().updated_at, Timestamp(60));
    let page = store.timeline(issue.id, None, 50).await.unwrap();
    assert_eq!(page.items.len(), 2);
}

#[tokio::test]
async fn the_timeline_merges_comments_and_events_in_order_with_a_cursor() {
    let fixture = Fixture::new().await;
    let store = fixture.store();
    let issue = store.create(&fixture.new_issue(IssueKind::Issue, "Talk", 10)).await.unwrap();
    let alice = fixture.alice.id();
    let label = LabelSnapshot { id: LabelId(1), name: "bug".to_owned(), color: "#d73a4a".to_owned() };

    // Same second: the comment comes before the event even though the event row is older.
    let labeled = store.record_event(issue.id, Some(alice), &TimelineEvent::Labeled { label: label.clone() }, Timestamp(20)).await.unwrap();
    let c1 = store.add_comment(issue.id, alice, &text("one"), Timestamp(20)).await.unwrap().unwrap();
    let c2 = store.add_comment(issue.id, alice, &text("two"), Timestamp(30)).await.unwrap().unwrap();
    let system = store.record_event(issue.id, None, &TimelineEvent::Locked, Timestamp(25)).await.unwrap();
    let c3 = store.add_comment(issue.id, alice, &text("three"), Timestamp(40)).await.unwrap().unwrap();

    let order = |items: &[StoredTimelineItem]| -> Vec<TimelineItemId> { items.iter().map(|item| item.cursor().item).collect() };
    let all = store.timeline(issue.id, None, 200).await.unwrap();
    assert_eq!(
        order(&all.items),
        [
            TimelineItemId::Comment(c1.id),
            TimelineItemId::Event(labeled),
            TimelineItemId::Event(system),
            TimelineItemId::Comment(c2.id),
            TimelineItemId::Comment(c3.id),
        ]
    );
    assert!(all.next.is_none());
    let StoredTimelineItem::Comment { author, comment } = &all.items[0] else { panic!("a comment") };
    assert_eq!(author.as_ref().unwrap().name.as_str(), "alice");
    assert_eq!(comment.body.as_str(), "one");
    let StoredTimelineItem::Event { actor, actor_id, event, at, .. } = &all.items[2] else { panic!("an event") };
    assert!(actor.is_none() && actor_id.is_none(), "a system event has no actor");
    assert_eq!((event, *at), (&TimelineEvent::Locked, Timestamp(25)));

    // Pages of two, chained by their cursors, cover everything exactly once.
    let mut seen = Vec::new();
    let mut cursor: Option<TimelineCursor> = None;
    loop {
        let page = store.timeline(issue.id, cursor, 2).await.unwrap();
        assert!(page.items.len() <= 2);
        seen.extend(order(&page.items));
        match page.next {
            Some(next) => {
                assert_eq!(Some(next), page.items.last().map(StoredTimelineItem::cursor));
                cursor = Some(TimelineCursor::parse(&next.to_param()).unwrap());
            }
            None => break,
        }
    }
    assert_eq!(seen, order(&all.items));
    let exact = store.timeline(issue.id, None, 5).await.unwrap();
    assert!(exact.next.is_none(), "no next page when the page ends exactly at the last item");
}

#[tokio::test]
async fn unknown_event_kinds_are_skipped_and_deleted_accounts_become_ghosts() {
    let fixture = Fixture::new().await;
    let store = fixture.store();
    let bob = user(&fixture.database, "bob").await;
    let issue = store.create(&fixture.new_issue(IssueKind::Issue, "Future", 10)).await.unwrap();
    store.add_comment(issue.id, bob.id(), &text("from bob"), Timestamp(11)).await.unwrap();
    fixture
        .execute(&format!(
            "INSERT INTO issue_events (issue_id, actor_id, kind, data, created_at) VALUES ({}, NULL, 'teleported', '{{\"to\":\"mars\"}}', 12)",
            issue.id.0
        ))
        .await;
    store.record_event(issue.id, Some(bob.id()), &TimelineEvent::Reopened, Timestamp(13)).await.unwrap();
    fixture.execute(&format!("DELETE FROM accounts WHERE id = {}", bob.id().0)).await;

    let page = store.timeline(issue.id, None, 2).await.unwrap();
    assert_eq!(page.items.len(), 1, "the unknown event is left out but counts towards the page");
    assert!(page.next.is_some());
    let StoredTimelineItem::Comment { author, comment } = &page.items[0] else { panic!("a comment") };
    assert!(author.is_none() && comment.author.is_none(), "a ghost");
    let rest = store.timeline(issue.id, page.next, 2).await.unwrap();
    assert!(matches!(&rest.items[..], [StoredTimelineItem::Event { event: TimelineEvent::Reopened, actor: None, .. }]));
    assert!(matches!(rest.items[0].cursor().item, TimelineItemId::Event(EventId(_))));
}

#[tokio::test]
async fn open_counts_and_number_lookups() {
    let fixture = Fixture::new().await;
    let store = fixture.store();
    let alice = Some(fixture.alice.id());
    for (kind, title) in [(IssueKind::Issue, "a"), (IssueKind::Issue, "b"), (IssueKind::Pull, "c"), (IssueKind::Issue, "d")] {
        store.create(&fixture.new_issue(kind, title, 1)).await.unwrap();
    }
    let b = store.find_by_number(fixture.repo, number(2)).await.unwrap().unwrap();
    let closed = TimelineEvent::Closed { reason: StateReason::NotPlanned, commit: None, pull: None };
    store.set_state(b.id, IssueState::Closed, Some(StateReason::NotPlanned), alice, &closed, Timestamp(2)).await.unwrap();
    assert_eq!(store.open_counts(fixture.repo).await.unwrap(), (2, 1));
    assert_eq!(store.open_counts(RepoId(999)).await.unwrap(), (0, 0));

    let found = store.find_numbers(fixture.repo, &[number(3), number(2), number(9)]).await.unwrap();
    let summary: Vec<_> = found.iter().map(|r| (r.number.get(), r.kind, r.state, r.title.as_str())).collect();
    assert_eq!(summary, [(2, IssueKind::Issue, IssueState::Closed, "b"), (3, IssueKind::Pull, IssueState::Open, "c")]);
    assert_eq!(store.find_numbers(fixture.repo, &[]).await.unwrap(), Vec::new());
}

#[tokio::test]
async fn cross_reference_sources_respect_repository_visibility() {
    let fixture = Fixture::new().await;
    let store = fixture.store();
    let carol = user(&fixture.database, "carol").await;
    let secret = repo(&fixture.database, &carol, "secret", Visibility::Private).await;
    let public = store.create(&fixture.new_issue(IssueKind::Issue, "Public", 1)).await.unwrap();
    let hidden =
        store.create(&NewIssue { repo: secret, author: carol.id(), ..fixture.new_issue(IssueKind::Pull, "Hidden", 1) }).await.unwrap();
    let ids = [public.id, hidden.id, IssueId(999)];

    let names = |sources: Vec<rbit_storage::sqlite::issues::IssueSource>| -> Vec<String> {
        sources.into_iter().map(|s| format!("{}{}", s.full_name, s.issue.number)).collect()
    };
    assert_eq!(names(store.readable_sources(&ids, None, false).await.unwrap()), ["alice/demo#1"]);
    assert_eq!(names(store.readable_sources(&ids, Some(fixture.alice.id()), false).await.unwrap()), ["alice/demo#1"]);
    assert_eq!(names(store.readable_sources(&ids, Some(carol.id()), false).await.unwrap()), ["alice/demo#1", "carol/secret#1"]);
    assert_eq!(names(store.readable_sources(&ids, Some(AccountId(12_345)), true).await.unwrap()), ["alice/demo#1", "carol/secret#1"]);
    assert_eq!(store.readable_sources(&[], None, false).await.unwrap(), Vec::new());
}

#[tokio::test]
async fn deleting_a_repository_removes_its_threads_and_counter() {
    let fixture = Fixture::new().await;
    let store = fixture.store();
    let issue = store.create(&fixture.new_issue(IssueKind::Issue, "Doomed", 1)).await.unwrap();
    store.add_comment(issue.id, fixture.alice.id(), &text("bye"), Timestamp(2)).await.unwrap();
    fixture.execute(&format!("DELETE FROM repositories WHERE id = {}", fixture.repo.0)).await;
    assert!(store.find(issue.id).await.unwrap().is_none());
}
