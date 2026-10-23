//! Labels, milestones, assignees and reference de-duplication (workstream I) against a
//! real, migrated `SQLite` database.

use rbit_core::{
    IssueId, IssueKind, IssueState, IssueText, IssueTitle, LabelId, MilestoneId, OrgRole, RepoId, Timestamp, User, Visibility,
    membership::{BasePermission, Permission},
};
use rbit_storage::{
    Database, NewRepository, NewUser, StorageError,
    sqlite::{
        issue_meta::{LabelFields, MilestoneFields},
        issues::NewIssue,
        orgs::NewOrganization,
    },
};

struct Fixture {
    _dir: tempfile::TempDir,
    database: Database,
    alice: User,
    repo: RepoId,
}

impl Fixture {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let database = Database::open(&dir.path().join("rbit.db")).await.unwrap();
        let alice = user(&database, "alice").await;
        let repo = repo(&database, &alice, "demo").await;
        Self { _dir: dir, database, alice, repo }
    }

    async fn issue(&self, kind: IssueKind, title: &str) -> IssueId {
        let new = NewIssue {
            repo: self.repo,
            kind,
            title: IssueTitle::parse(title).unwrap(),
            body: IssueText::default(),
            author: self.alice.id(),
            at: Timestamp(100),
        };
        self.database.issues().create(&new).await.unwrap().id
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

async fn repo(database: &Database, owner: &User, name: &str) -> RepoId {
    let new = NewRepository {
        owner_id: owner.id(),
        name: name.parse().unwrap(),
        description: None,
        visibility: Visibility::Public,
        default_branch: "main".to_owned(),
    };
    database.repositories().create(new).await.unwrap().id
}

fn label(name: &str, color: &str) -> LabelFields {
    LabelFields { name: name.to_owned(), color: color.to_owned(), description: String::new() }
}

#[tokio::test]
async fn labels_are_unique_per_repository_case_insensitively() {
    let fixture = Fixture::new().await;
    let store = fixture.database.issue_meta();
    let bug = store.create_label(fixture.repo, &label("bug", "#d73a4a"), Timestamp(1)).await.unwrap();
    assert_eq!((bug.name.as_str(), bug.color.as_str()), ("bug", "#d73a4a"));
    let duplicate = store.create_label(fixture.repo, &label("BUG", "#000000"), Timestamp(2)).await;
    assert!(matches!(duplicate, Err(StorageError::Conflict("label name"))));
    let other = repo(&fixture.database, &fixture.alice, "other").await;
    store.create_label(other, &label("bug", "#000000"), Timestamp(2)).await.unwrap();

    assert_eq!(store.find_label_by_name(fixture.repo, "Bug").await.unwrap().map(|l| l.id), Some(bug.id));
    assert!(store.find_label(other, bug.id).await.unwrap().is_none(), "labels are per repository");
    let docs = store.create_label(fixture.repo, &label("docs", "#0075ca"), Timestamp(3)).await.unwrap();
    let renamed = store.update_label(fixture.repo, docs.id, &label("Bug", "#0075ca"), Timestamp(4)).await;
    assert!(matches!(renamed, Err(StorageError::Conflict("label name"))));
    let renamed = store.update_label(fixture.repo, docs.id, &label("documentation", "#0075ca"), Timestamp(4)).await.unwrap().unwrap();
    assert_eq!(renamed.name, "documentation");
    assert!(store.update_label(other, docs.id, &label("x", "#000000"), Timestamp(5)).await.unwrap().is_none());

    let names: Vec<String> = store.labels(fixture.repo).await.unwrap().into_iter().map(|l| l.name).collect();
    assert_eq!(names, ["bug", "documentation"]);
    assert_eq!(store.count_labels(fixture.repo).await.unwrap(), 2);
    let found = store.labels_named(fixture.repo, &["DOCUMENTATION".to_owned(), "nope".to_owned()]).await.unwrap();
    assert_eq!(found.len(), 1);

    // The schema refuses colours that are not lower-case #rrggbb.
    assert!(store.create_label(fixture.repo, &label("x", "#ABCDEF"), Timestamp(5)).await.is_err());
    assert!(store.create_label(fixture.repo, &label("y", "red"), Timestamp(5)).await.is_err());

    let defaults = [label("bug", "#d73a4a"), label("question", "#d876e3")];
    assert_eq!(store.create_missing_labels(fixture.repo, &defaults, Timestamp(6)).await.unwrap(), 1);
}

#[tokio::test]
async fn labels_attach_to_threads_and_vanish_with_their_deletion() {
    let fixture = Fixture::new().await;
    let store = fixture.database.issue_meta();
    let issue = fixture.issue(IssueKind::Issue, "Crash").await;
    let pull = fixture.issue(IssueKind::Pull, "Fix").await;
    let bug = store.create_label(fixture.repo, &label("bug", "#d73a4a"), Timestamp(1)).await.unwrap();
    let ui = store.create_label(fixture.repo, &label("ui", "#123456"), Timestamp(1)).await.unwrap();

    assert!(store.add_label(issue, bug.id, Timestamp(2)).await.unwrap());
    assert!(!store.add_label(issue, bug.id, Timestamp(2)).await.unwrap(), "only once");
    assert!(store.add_label(issue, ui.id, Timestamp(2)).await.unwrap());
    assert!(store.add_label(pull, bug.id, Timestamp(2)).await.unwrap());
    let labels = store.labels_of(&[issue, pull]).await.unwrap();
    let names: Vec<(i64, &str)> = labels.iter().map(|(id, l)| (id.0, l.name.as_str())).collect();
    assert_eq!(names, [(issue.0, "bug"), (issue.0, "ui"), (pull.0, "bug")]);

    let usage = store.label_usage(fixture.repo, "issue").await.unwrap();
    assert_eq!(usage.iter().map(|u| u.open_issues).collect::<Vec<_>>(), [1, 1], "pull requests are counted separately");

    assert!(store.remove_label(issue, ui.id).await.unwrap());
    assert!(!store.remove_label(issue, ui.id).await.unwrap());
    let deleted = store.delete_label(fixture.repo, bug.id).await.unwrap().unwrap();
    assert_eq!(deleted.snapshot().name, "bug");
    assert_eq!(store.labels_of(&[issue, pull]).await.unwrap().len(), 0);
    assert!(store.delete_label(fixture.repo, LabelId(999)).await.unwrap().is_none());
}

#[tokio::test]
async fn milestones_track_progress_and_one_per_thread() {
    let fixture = Fixture::new().await;
    let store = fixture.database.issue_meta();
    let fields = |title: &str, due: Option<&str>| MilestoneFields {
        title: title.to_owned(),
        description: String::new(),
        due_on: due.map(str::to_owned),
    };
    let later = store.create_milestone(fixture.repo, &fields("v2", None), Timestamp(1)).await.unwrap();
    let soon = store.create_milestone(fixture.repo, &fields("v1", Some("2026-11-01")), Timestamp(1)).await.unwrap();
    assert!(matches!(store.create_milestone(fixture.repo, &fields("V1", None), Timestamp(1)).await, Err(StorageError::Conflict(_))));
    assert!(store.create_milestone(fixture.repo, &fields("bad date", Some("soon")), Timestamp(1)).await.is_err());

    let first = fixture.issue(IssueKind::Issue, "One").await;
    let second = fixture.issue(IssueKind::Issue, "Two").await;
    assert_eq!(store.set_milestone(first, Some(soon.id)).await.unwrap(), None);
    store.set_milestone(second, Some(later.id)).await.unwrap();
    let previous = store.set_milestone(second, Some(soon.id)).await.unwrap().unwrap();
    assert_eq!((previous.id, previous.title.as_str()), (later.id, "v2"), "the previous milestone is returned");
    let event = rbit_core::TimelineEvent::Closed { reason: rbit_core::StateReason::Completed, commit: None, pull: None };
    fixture
        .database
        .issues()
        .set_state(second, IssueState::Closed, Some(rbit_core::StateReason::Completed), None, &event, Timestamp(5))
        .await
        .unwrap();

    let progress = store.milestones(fixture.repo, None).await.unwrap();
    let rows: Vec<(&str, u64, u64)> = progress.iter().map(|p| (p.milestone.title.as_str(), p.open_issues, p.closed_issues)).collect();
    assert_eq!(rows, [("v1", 1, 1), ("v2", 0, 0)], "dated open milestones first");
    let of = store.milestones_of(&[first, second]).await.unwrap();
    assert_eq!(of.iter().map(|m| m.id).collect::<Vec<_>>(), [soon.id, soon.id]);

    let closed = store.set_milestone_state(fixture.repo, later.id, IssueState::Closed, Timestamp(9)).await.unwrap().unwrap();
    assert_eq!((closed.state, closed.closed_at), (IssueState::Closed, Some(Timestamp(9))));
    assert_eq!(store.milestone_counts(fixture.repo).await.unwrap(), (1, 1));
    assert_eq!(store.milestones(fixture.repo, Some(IssueState::Closed)).await.unwrap().len(), 1);
    let reopened = store.set_milestone_state(fixture.repo, later.id, IssueState::Open, Timestamp(10)).await.unwrap().unwrap();
    assert_eq!(reopened.closed_at, None);

    assert_eq!(store.set_milestone(first, None).await.unwrap().map(|m| m.id), Some(soon.id));
    store.delete_milestone(fixture.repo, soon.id).await.unwrap().unwrap();
    assert_eq!(store.milestones_of(&[first, second]).await.unwrap().len(), 0, "deleting a milestone clears it from threads");
    assert!(store.find_milestone(fixture.repo, MilestoneId(999)).await.unwrap().is_none());
    assert_eq!(store.find_milestone_by_title(fixture.repo, "V2").await.unwrap().map(|m| m.id), Some(later.id));
}

#[tokio::test]
async fn assignees_and_who_may_be_assigned() {
    let fixture = Fixture::new().await;
    let store = fixture.database.issue_meta();
    let bob = user(&fixture.database, "bob").await;
    let carol = user(&fixture.database, "carol").await;
    let dave = user(&fixture.database, "dave").await;
    let collaborators = fixture.database.collaborators();
    collaborators.add(fixture.repo, bob.id(), Permission::Write, fixture.alice.id()).await.unwrap();
    collaborators.add(fixture.repo, carol.id(), Permission::Read, fixture.alice.id()).await.unwrap();

    let names = |accounts: Vec<rbit_core::Account>| accounts.into_iter().map(|a| a.name.to_string()).collect::<Vec<_>>();
    assert_eq!(names(store.assignable(fixture.repo, "", 100).await.unwrap()), ["alice", "bob"], "readers and strangers are not listed");
    assert_eq!(names(store.assignable(fixture.repo, "B", 100).await.unwrap()), ["bob"]);
    assert_eq!(names(store.assignable(fixture.repo, "_", 100).await.unwrap()), Vec::<String>::new(), "wildcards are literal");
    let _ = dave;

    let issue = fixture.issue(IssueKind::Issue, "Crash").await;
    assert!(store.add_assignee(issue, bob.id(), fixture.alice.id(), Timestamp(1)).await.unwrap());
    assert!(!store.add_assignee(issue, bob.id(), fixture.alice.id(), Timestamp(1)).await.unwrap());
    assert!(store.add_assignee(issue, fixture.alice.id(), fixture.alice.id(), Timestamp(1)).await.unwrap());
    let assigned = store.assignees_of(&[issue]).await.unwrap();
    assert_eq!(assigned.iter().map(|(_, a)| a.name.as_str()).collect::<Vec<_>>(), ["alice", "bob"]);
    assert!(store.remove_assignee(issue, bob.id()).await.unwrap());
    assert!(!store.remove_assignee(issue, bob.id()).await.unwrap());
}

/// Assignee search starts from the repository's possible writers; every way of getting
/// push access to an organisation's repository must still be found, and nothing else.
#[tokio::test]
async fn organisation_repositories_list_owners_writers_and_team_grants() {
    let fixture = Fixture::new().await;
    let database = &fixture.database;
    let store = database.issue_meta();
    let [bob, carol, dave, erin, frank, _stranger] = {
        let mut users = Vec::new();
        for name in ["bob", "carol", "dave", "erin", "frank", "stranger"] {
            users.push(user(database, name).await);
        }
        <[User; 6]>::try_from(users).unwrap()
    };
    let org =
        |name: &str| NewOrganization { name: name.parse().unwrap(), display_name: None, description: None, creator: fixture.alice.id() };
    let acme = database.orgs().create(org("acme")).await.unwrap().account.id;
    let other = database.orgs().create(org("other")).await.unwrap().account.id;
    let rocket = repo_owned_by(database, acme, "rocket").await;
    database.orgs().set_base_permission(acme, BasePermission::Read).await.unwrap();
    for member in [&bob, &carol, &dave, &erin] {
        database.orgs().add_member(acme, member.id(), OrgRole::Member).await.unwrap();
    }
    database.orgs().add_member(other, erin.id(), OrgRole::Member).await.unwrap();
    let core = database.teams().create(acme, &"core".parse().unwrap(), None).await.unwrap();
    for member in [&carol, &dave] {
        database.teams().add_member(core.id, member.id()).await.unwrap();
    }
    database.teams().set_repository(core.id, rocket, Permission::Write).await.unwrap();
    // A team of another organisation never grants anything here.
    let foreign = database.teams().create(other, &"core".parse().unwrap(), None).await.unwrap();
    database.teams().add_member(foreign.id, erin.id()).await.unwrap();
    database.teams().set_repository(foreign.id, rocket, Permission::Write).await.unwrap();
    database.collaborators().add(rocket, frank.id(), Permission::Write, fixture.alice.id()).await.unwrap();
    database.admin().set_suspended(dave.id(), true).await.unwrap();

    let names = |accounts: Vec<rbit_core::Account>| accounts.into_iter().map(|a| a.name.to_string()).collect::<Vec<_>>();
    assert_eq!(names(store.assignable(rocket, "", 100).await.unwrap()), ["alice", "carol", "frank"], "owner, team writer, collaborator");
    assert_eq!(names(store.assignable(rocket, "C", 100).await.unwrap()), ["carol"]);
    assert_eq!(names(store.assignable(rocket, "", 2).await.unwrap()), ["alice", "carol"], "limit after ordering by name");
    database.orgs().set_base_permission(acme, BasePermission::Write).await.unwrap();
    assert_eq!(names(store.assignable(rocket, "", 100).await.unwrap()), ["alice", "bob", "carol", "erin", "frank"], "every member writes");
    assert_eq!(names(store.assignable(RepoId(999), "", 100).await.unwrap()), Vec::<String>::new());
    let _ = (bob, erin);
}

async fn repo_owned_by(database: &Database, owner: rbit_core::AccountId, name: &str) -> RepoId {
    let new = NewRepository {
        owner_id: owner,
        name: name.parse().unwrap(),
        description: None,
        visibility: Visibility::Private,
        default_branch: "main".to_owned(),
    };
    database.repositories().create(new).await.unwrap().id
}

#[tokio::test]
async fn references_are_recorded_once() {
    let fixture = Fixture::new().await;
    let store = fixture.database.issue_meta();
    let target = fixture.issue(IssueKind::Issue, "Target").await;
    let source = fixture.issue(IssueKind::Pull, "Source").await;
    assert!(store.record_commit_ref(target, "abc123", Timestamp(1)).await.unwrap());
    assert!(!store.record_commit_ref(target, "abc123", Timestamp(2)).await.unwrap());
    assert!(store.record_commit_ref(source, "abc123", Timestamp(2)).await.unwrap());
    assert!(store.record_cross_ref(target, source, Timestamp(1)).await.unwrap());
    assert!(!store.record_cross_ref(target, source, Timestamp(1)).await.unwrap());
    assert!(store.record_cross_ref(source, target, Timestamp(1)).await.unwrap());
}

#[tokio::test]
async fn new_threads_get_their_metadata_at_once_and_mentions_resolve() {
    let fixture = Fixture::new().await;
    let store = fixture.database.issue_meta();
    let bob = user(&fixture.database, "bob").await;
    let issue = fixture.issue(IssueKind::Issue, "Crash").await;
    let bug = store.create_label(fixture.repo, &label("bug", "#d73a4a"), Timestamp(1)).await.unwrap();
    let fields = MilestoneFields { title: "v1".to_owned(), description: String::new(), due_on: None };
    let milestone = store.create_milestone(fixture.repo, &fields, Timestamp(1)).await.unwrap();
    store.attach(issue, &[bug.id], &[bob.id()], Some(milestone.id), fixture.alice.id(), Timestamp(2)).await.unwrap();
    assert_eq!(store.labels_of(&[issue]).await.unwrap().len(), 1);
    assert_eq!(store.assignees_of(&[issue]).await.unwrap()[0].1.id, bob.id());
    assert_eq!(store.milestones_of(&[issue]).await.unwrap()[0].id, milestone.id);

    let comment = fixture
        .database
        .issues()
        .add_comment(issue, fixture.alice.id(), &IssueText::comment("Hi").unwrap(), Timestamp(3))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(store.comment_thread(fixture.repo, comment.id).await.unwrap().map(|(id, n)| (id, n.get())), Some((issue, 1)));
    let other = repo(&fixture.database, &fixture.alice, "other").await;
    assert_eq!(store.comment_thread(other, comment.id).await.unwrap(), None, "only within the repository");

    let found = store.accounts_named(&["BOB".to_owned(), "nobody".to_owned(), "alice".to_owned()]).await.unwrap();
    assert_eq!(found.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(), ["alice", "bob"]);
    let names = ["bob", "Bob", "ALICE"].map(str::to_owned);
    let found = store.accounts_named(&names).await.unwrap();
    assert_eq!(found.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(), ["alice", "bob"], "each account once, whatever the case");
}
