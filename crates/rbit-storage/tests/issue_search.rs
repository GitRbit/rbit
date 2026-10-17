//! Issue lists (workstream I): every filter, sorting, counts per tab and FTS5 search,
//! against a real, migrated SQLite database.

use rbit_core::{
    AccountId, IssueId, IssueKind, IssueState, IssueText, IssueTitle, RepoId, StateReason, TimelineEvent, Timestamp, User, Visibility,
    issue_query::IssueSort,
};
use rbit_storage::{
    Database, NewRepository, NewUser,
    sqlite::{
        issue_meta::{LabelFields, MilestoneFields},
        issue_search::SearchFilter,
        issues::NewIssue,
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

    async fn issue(&self, kind: IssueKind, title: &str, body: &str, author: AccountId, at: i64) -> IssueId {
        let new = NewIssue {
            repo: self.repo,
            kind,
            title: IssueTitle::parse(title).unwrap(),
            body: IssueText::body(body).unwrap(),
            author,
            at: Timestamp(at),
        };
        self.database.issues().create(&new).await.unwrap().id
    }

    async fn numbers(&self, filter: &SearchFilter) -> Vec<u32> {
        let found = self.database.issue_search().search(filter, 25, 0).await.unwrap();
        found.into_iter().map(|issue| issue.number.get()).collect()
    }
}

#[tokio::test]
async fn the_bundled_sqlite_has_fts5() {
    let fixture = Fixture::new().await;
    assert!(fixture.database.issue_search().has_fts5().await.unwrap());
}

#[tokio::test]
async fn every_filter_and_order() {
    let fixture = Fixture::new().await;
    let (alice, bob) = (fixture.alice.id(), fixture.bob.id());
    let one = fixture.issue(IssueKind::Issue, "Crash on startup", "Segfault in the boot loader", alice, 10).await;
    let two = fixture.issue(IssueKind::Issue, "Café menu is slow", "Rendering takes seconds", bob, 20).await;
    let three = fixture.issue(IssueKind::Issue, "Typo in docs", "", alice, 30).await;
    let pull = fixture.issue(IssueKind::Pull, "Fix the crash", "Fixes #1", bob, 40).await;

    let meta = fixture.database.issue_meta();
    let label = |name: &str| LabelFields { name: name.to_owned(), color: "#aabbcc".to_owned(), description: String::new() };
    let bug = meta.create_label(fixture.repo, &label("bug"), Timestamp(1)).await.unwrap();
    let ui = meta.create_label(fixture.repo, &label("good first issue"), Timestamp(1)).await.unwrap();
    meta.add_label(one, bug.id, Timestamp(1)).await.unwrap();
    meta.add_label(two, bug.id, Timestamp(1)).await.unwrap();
    meta.add_label(two, ui.id, Timestamp(1)).await.unwrap();
    meta.add_label(pull, bug.id, Timestamp(1)).await.unwrap();
    meta.add_assignee(two, bob, alice, Timestamp(1)).await.unwrap();
    let fields = MilestoneFields { title: "v1.0".to_owned(), description: String::new(), due_on: None };
    let milestone = meta.create_milestone(fixture.repo, &fields, Timestamp(1)).await.unwrap();
    meta.set_milestone(three, Some(milestone.id)).await.unwrap();

    let issues = fixture.database.issues();
    let closed = TimelineEvent::Closed { reason: StateReason::Completed, commit: None, pull: None };
    issues.set_state(three, IssueState::Closed, Some(StateReason::Completed), Some(alice), &closed, Timestamp(50)).await.unwrap();
    let merged = TimelineEvent::Merged { commit: "abc".to_owned(), strategy: None };
    issues.set_state(pull, IssueState::Closed, Some(StateReason::Merged), Some(alice), &merged, Timestamp(60)).await.unwrap();
    issues.set_locked(one, true, alice, Timestamp(70)).await.unwrap();
    issues.add_comment(two, alice, &IssueText::comment("+1").unwrap(), Timestamp(80)).await.unwrap();

    let base = || SearchFilter { kind: Some(IssueKind::Issue), ..SearchFilter::new(fixture.repo) };
    assert_eq!(fixture.numbers(&base()).await, [3, 2, 1], "newest first by default");
    assert_eq!(fixture.numbers(&SearchFilter::new(fixture.repo)).await, [4, 3, 2, 1], "both kinds");
    assert_eq!(fixture.numbers(&SearchFilter { state: Some(IssueState::Open), ..base() }).await, [2, 1]);
    assert_eq!(fixture.numbers(&SearchFilter { state: Some(IssueState::Closed), ..base() }).await, [3]);
    assert_eq!(fixture.numbers(&SearchFilter { merged: true, ..SearchFilter::new(fixture.repo) }).await, [4]);
    assert_eq!(fixture.numbers(&SearchFilter { draft: true, ..SearchFilter::new(fixture.repo) }).await, Vec::<u32>::new());
    assert_eq!(fixture.numbers(&SearchFilter { locked: true, ..base() }).await, [1]);
    assert_eq!(fixture.numbers(&SearchFilter { author: Some(bob), ..base() }).await, [2]);
    assert_eq!(fixture.numbers(&SearchFilter { author: Some(AccountId(0)), ..base() }).await, Vec::<u32>::new(), "unknown author");
    assert_eq!(fixture.numbers(&SearchFilter { assignee: Some(bob), ..base() }).await, [2]);
    assert_eq!(fixture.numbers(&SearchFilter { no_assignee: true, ..base() }).await, [3, 1]);

    let names = |names: &[&str]| names.iter().map(|n| (*n).to_owned()).collect::<Vec<_>>();
    assert_eq!(fixture.numbers(&SearchFilter { labels: names(&["BUG"]), ..base() }).await, [2, 1]);
    assert_eq!(fixture.numbers(&SearchFilter { labels: names(&["bug", "good first issue"]), ..base() }).await, [2], "labels AND");
    assert_eq!(fixture.numbers(&SearchFilter { labels: names(&["bug", "missing"]), ..base() }).await, Vec::<u32>::new());
    assert_eq!(fixture.numbers(&SearchFilter { not_labels: names(&["Good First Issue"]), ..base() }).await, [3, 1]);
    assert_eq!(fixture.numbers(&SearchFilter { no_label: true, ..base() }).await, [3]);
    assert_eq!(fixture.numbers(&SearchFilter { milestone: Some("V1.0".to_owned()), ..base() }).await, [3]);
    assert_eq!(fixture.numbers(&SearchFilter { no_milestone: true, ..base() }).await, [2, 1]);

    let text = |words: &[&str]| SearchFilter { text: names(words), ..base() };
    assert_eq!(fixture.numbers(&text(&["crash"])).await, [1], "title");
    assert_eq!(fixture.numbers(&text(&["segf"])).await, [1], "prefix of the last word, in the body");
    assert_eq!(fixture.numbers(&text(&["cafe"])).await, [2], "diacritics are folded");
    assert_eq!(fixture.numbers(&text(&["boot", "seg"])).await, [1], "all words");
    assert_eq!(fixture.numbers(&text(&["boot", "typo"])).await, Vec::<u32>::new());
    assert_eq!(fixture.numbers(&text(&["\"", "*"])).await, [3, 2, 1], "nothing searchable is no text filter");
    assert_eq!(fixture.numbers(&text(&["NEAR(crash", "OR"])).await, Vec::<u32>::new(), "operators are quoted");

    let sorted = |sort| SearchFilter { sort, ..base() };
    assert_eq!(fixture.numbers(&sorted(IssueSort::CreatedAsc)).await, [1, 2, 3]);
    assert_eq!(fixture.numbers(&sorted(IssueSort::UpdatedDesc)).await, [2, 1, 3]);
    assert_eq!(fixture.numbers(&sorted(IssueSort::UpdatedAsc)).await, [3, 1, 2]);
    assert_eq!(fixture.numbers(&sorted(IssueSort::CommentsDesc)).await, [2, 3, 1]);

    assert_eq!(fixture.numbers(&SearchFilter { any_assignee: true, ..base() }).await, [2]);
    assert_eq!(fixture.numbers(&SearchFilter { any_milestone: true, ..base() }).await, [3]);
    assert_eq!(fixture.numbers(&SearchFilter { updated_since: Some(Timestamp(70)), ..base() }).await, [2, 1]);

    let store = fixture.database.issue_search();
    let open_bugs = SearchFilter { state: Some(IssueState::Open), labels: names(&["bug"]), ..SearchFilter::new(fixture.repo) };
    assert_eq!(store.state_counts(&open_bugs).await.unwrap(), (2, 1), "tab counts ignore the state but honour the rest");
    assert_eq!(store.count(&open_bugs).await.unwrap(), 2);
    assert_eq!(store.search(&base(), 2, 2).await.unwrap().len(), 1, "pages");

    // The index follows edits and deletions.
    issues.update_text(three, Some(&IssueTitle::parse("Spelling mistake").unwrap()), None, alice, Timestamp(90)).await.unwrap();
    assert_eq!(fixture.numbers(&text(&["typo"])).await, Vec::<u32>::new());
    assert_eq!(fixture.numbers(&text(&["spelling"])).await, [3]);
    issues.delete(three).await.unwrap();
    assert_eq!(fixture.numbers(&text(&["spelling"])).await, Vec::<u32>::new());
}
