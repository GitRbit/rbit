//! The issue service (workstream I) against real stores: creation with metadata, label
//! and assignee rules, milestones, deletion, search, cross-references and references
//! from pushed commits.

use std::{path::Path, process::Command, sync::Arc};

use rbit_core::{
    Config, IssueKind, IssueNumber, IssueState, StateReason, TimelineEvent, User, Visibility, issue::IssueAction, issue_query::IssueQuery,
    membership::Permission,
};
use rbit_services::{
    CreateRepository, DomainEvent, IssueContext, RepoContext, ServiceError, Services,
    events::RefUpdate,
    issues::{LabelInput, MilestoneInput, NewIssueInput},
    timeline::{StateChange, TimelineEntry},
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

impl Fixture {
    async fn new() -> Self {
        Self::with(|_| {}).await
    }

    async fn with(configure: impl FnOnce(&mut Config)) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let database = Database::open(&dir.path().join("rustit.db")).await.unwrap();
        let kv = KvStore::open(&dir.path().join("rustit.redb")).unwrap();
        let mut config = Config::default();
        config.storage.data_dir = dir.path().to_path_buf();
        // These tests make many changes in a row; throttling has its own test below.
        config.security.rate_limit = false;
        configure(&mut config);
        let services = Services::new(Arc::new(config), &database, &kv).await.unwrap();
        let alice = user(&database, "alice").await;
        let bob = user(&database, "bob").await;
        let carol = user(&database, "carol").await;
        let input = CreateRepository { name: "demo".to_owned(), description: String::new(), visibility: Visibility::Public };
        services.repositories.create(&alice, input).await.unwrap();
        Self { _dir: dir, database, services, alice, bob, carol }
    }

    async fn repo(&self, viewer: &User) -> RepoContext {
        self.services.repositories.open(Some(viewer), "alice", "demo").await.unwrap()
    }

    async fn create(&self, author: &User, title: &str, body: &str) -> IssueNumber {
        let input = NewIssueInput { title: title.to_owned(), body: body.to_owned(), ..NewIssueInput::default() };
        self.services.issues.create(author, &self.repo(author).await, input).await.unwrap().number
    }

    async fn open(&self, viewer: &User, number: IssueNumber) -> IssueContext {
        self.services.timeline.open(Some(viewer), self.repo(viewer).await, number).await.unwrap()
    }

    async fn label(&self, name: &str) -> rbit_core::LabelId {
        let input = LabelInput { name: name.to_owned(), color: "#aabbcc".to_owned(), description: String::new() };
        self.services.issues.create_label(&self.alice, &self.repo(&self.alice).await, &input).await.unwrap().id
    }

    async fn events_of(&self, viewer: &User, number: IssueNumber) -> Vec<TimelineEvent> {
        let ctx = self.open(viewer, number).await;
        let timeline = self.services.timeline.timeline(Some(viewer), &ctx, None, 200).await.unwrap();
        timeline
            .entries
            .into_iter()
            .filter_map(|entry| if let TimelineEntry::Event { event, .. } = entry { Some(event) } else { None })
            .collect()
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

fn issue_actions(events: &mut Receiver<DomainEvent>) -> Vec<IssueAction> {
    let mut seen = Vec::new();
    loop {
        match events.try_recv() {
            Ok(DomainEvent::Issue { action, .. }) => seen.push(action),
            Ok(_) => {}
            Err(TryRecvError::Empty | TryRecvError::Closed) => return seen,
            Err(TryRecvError::Lagged(_)) => panic!("lagged"),
        }
    }
}

fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(dir)
        .args(["-c", "user.name=Ada", "-c", "user.email=ada@example.com", "-c", "init.defaultBranch=main"])
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

#[tokio::test]
async fn writers_open_issues_with_metadata_and_opened_is_published_once() {
    let fixture = Fixture::new().await;
    let repo = fixture.repo(&fixture.alice).await;
    fixture.services.collaborators.add(&fixture.alice, &repo, "bob", Permission::Write).await.unwrap();
    let bug = fixture.label("bug").await;
    let milestone = fixture
        .services
        .issues
        .create_milestone(&fixture.alice, &repo, &MilestoneInput { title: "v1".into(), ..MilestoneInput::default() })
        .await
        .unwrap();
    let mut events = fixture.services.events.subscribe();

    let input = NewIssueInput {
        title: "Crash".into(),
        body: "Boom".into(),
        labels: vec![bug, bug],
        assignees: vec![fixture.bob.id()],
        milestone: Some(milestone.id),
    };
    let issue = fixture.services.issues.create(&fixture.alice, &repo, input).await.unwrap();
    assert_eq!(issue_actions(&mut events), [IssueAction::Opened], "metadata changes are not published separately");
    let metadata = fixture.services.issues.metadata(&[issue.id]).await.unwrap().remove(&issue.id).unwrap();
    assert_eq!(metadata.labels.len(), 1);
    assert_eq!(metadata.assignees[0].id, fixture.bob.id());
    assert_eq!(metadata.milestone.unwrap().title, "v1");
    let recorded = fixture.events_of(&fixture.alice, issue.number).await;
    assert_eq!(recorded.len(), 3, "labeled, assigned and milestoned: {recorded:?}");

    // A reader's metadata is ignored; a carol (not a writer) cannot be assigned.
    let reader_repo = fixture.repo(&fixture.carol).await;
    let input = NewIssueInput { title: "Question".into(), labels: vec![bug], ..NewIssueInput::default() };
    let theirs = fixture.services.issues.create(&fixture.carol, &reader_repo, input).await.unwrap();
    assert_eq!(fixture.services.issues.metadata(&[theirs.id]).await.unwrap()[&theirs.id].labels, []);
    let input = NewIssueInput { title: "x".into(), assignees: vec![fixture.carol.id()], ..NewIssueInput::default() };
    assert!(matches!(fixture.services.issues.create(&fixture.alice, &repo, input).await, Err(ServiceError::Invalid(_))));
    let input = NewIssueInput { title: " ".into(), ..NewIssueInput::default() };
    assert!(matches!(fixture.services.issues.create(&fixture.alice, &repo, input).await, Err(ServiceError::Invalid(_))));
}

#[tokio::test]
async fn labels_have_limits_uniqueness_and_events() {
    let fixture = Fixture::new().await;
    let repo = fixture.repo(&fixture.alice).await;
    let issues = &fixture.services.issues;
    let bug = fixture.label("bug").await;
    let input = LabelInput { name: "BUG".into(), color: "#000000".into(), description: String::new() };
    assert!(matches!(issues.create_label(&fixture.alice, &repo, &input).await, Err(ServiceError::Conflict(_))));
    let input = LabelInput { name: "x".into(), color: "nope".into(), description: String::new() };
    assert!(matches!(issues.create_label(&fixture.alice, &repo, &input).await, Err(ServiceError::Invalid(_))));
    let reader = fixture.repo(&fixture.bob).await;
    let input = LabelInput { name: "mine".into(), color: "#000000".into(), description: String::new() };
    assert!(matches!(issues.create_label(&fixture.bob, &reader, &input).await, Err(ServiceError::Forbidden)));

    let number = fixture.create(&fixture.bob, "Crash", "").await;
    let as_bob = fixture.open(&fixture.bob, number).await;
    assert!(
        matches!(issues.change_labels(&fixture.bob, &as_bob, &[bug], &[]).await, Err(ServiceError::Forbidden)),
        "authors cannot triage"
    );

    let ctx = fixture.open(&fixture.alice, number).await;
    let mut events = fixture.services.events.subscribe();
    issues.change_labels(&fixture.alice, &ctx, &[bug], &[]).await.unwrap();
    issues.change_labels(&fixture.alice, &ctx, &[bug], &[]).await.unwrap();
    issues.change_labels(&fixture.alice, &ctx, &[], &[bug]).await.unwrap();
    issues.change_labels(&fixture.alice, &ctx, &[], &[bug]).await.unwrap();
    let actions = issue_actions(&mut events);
    assert!(matches!(actions.as_slice(), [IssueAction::Labeled { .. }, IssueAction::Unlabeled { .. }]), "once per change: {actions:?}");
    assert_eq!(fixture.events_of(&fixture.alice, number).await.len(), 2);

    let other = CreateRepository { name: "other".into(), description: String::new(), visibility: Visibility::Public };
    fixture.services.repositories.create(&fixture.alice, other).await.unwrap();
    let foreign = {
        let other_repo = fixture.services.repositories.open(Some(&fixture.alice), "alice", "other").await.unwrap();
        let input = LabelInput { name: "theirs".into(), color: "#000000".into(), description: String::new() };
        issues.create_label(&fixture.alice, &other_repo, &input).await.unwrap().id
    };
    assert!(
        matches!(issues.change_labels(&fixture.alice, &ctx, &[foreign], &[]).await, Err(ServiceError::Invalid(_))),
        "labels of other repositories"
    );

    let mut many = Vec::new();
    for i in 0..26 {
        many.push(fixture.label(&format!("l{i}")).await);
    }
    assert!(matches!(issues.set_labels(&fixture.alice, &ctx, &many).await, Err(ServiceError::Invalid(_))), "25 per issue");
    issues.set_labels(&fixture.alice, &ctx, &many[..25]).await.unwrap();
    issues.set_labels(&fixture.alice, &ctx, &many[1..3]).await.unwrap();
    assert_eq!(issues.metadata(&[ctx.issue.id]).await.unwrap()[&ctx.issue.id].labels.len(), 2);

    issues.delete_label(&repo, many[1]).await.unwrap();
    assert_eq!(issues.metadata(&[ctx.issue.id]).await.unwrap()[&ctx.issue.id].labels.len(), 1, "deleting a label removes it");

    let empty = fixture.services.repositories.open(Some(&fixture.alice), "alice", "other").await.unwrap();
    assert_eq!(issues.create_default_labels(&fixture.alice, &repo).await.unwrap(), 0, "only when there are none");
    issues.delete_label(&empty, foreign).await.unwrap();
    assert_eq!(issues.create_default_labels(&fixture.alice, &empty).await.unwrap(), 9);
}

#[tokio::test]
async fn assignees_must_be_writers_and_at_most_ten() {
    let fixture = Fixture::new().await;
    let repo = fixture.repo(&fixture.alice).await;
    fixture.services.collaborators.add(&fixture.alice, &repo, "bob", Permission::Write).await.unwrap();
    fixture.services.collaborators.add(&fixture.alice, &repo, "carol", Permission::Read).await.unwrap();
    let issues = &fixture.services.issues;
    let number = fixture.create(&fixture.alice, "Crash", "").await;
    let ctx = fixture.open(&fixture.alice, number).await;

    let names: Vec<String> = issues.assignable(&repo, "").await.unwrap().into_iter().map(|a| a.name.to_string()).collect();
    assert_eq!(names, ["alice", "bob"]);
    assert!(matches!(issues.change_assignees(&fixture.alice, &ctx, &[fixture.carol.id()], &[]).await, Err(ServiceError::Invalid(_))));
    issues.change_assignees(&fixture.alice, &ctx, &[fixture.bob.id(), fixture.alice.id()], &[]).await.unwrap();
    issues.set_assignees(&fixture.alice, &ctx, &[fixture.bob.id()]).await.unwrap();
    let assigned = &issues.metadata(&[ctx.issue.id]).await.unwrap()[&ctx.issue.id].assignees;
    assert_eq!(assigned.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(), ["bob"]);

    let mut writers = Vec::new();
    for i in 0..10 {
        let writer = user(&fixture.database, &format!("w{i}")).await;
        fixture.services.collaborators.add(&fixture.alice, &repo, writer.name().as_str(), Permission::Write).await.unwrap();
        writers.push(writer.id());
    }
    assert!(matches!(issues.change_assignees(&fixture.alice, &ctx, &writers, &[]).await, Err(ServiceError::Invalid(_))), "ten at most");
    let events = fixture.events_of(&fixture.alice, number).await;
    let assigned = events.iter().filter(|e| matches!(e, TimelineEvent::Assigned { .. })).count();
    let unassigned = events.iter().filter(|e| matches!(e, TimelineEvent::Unassigned { .. })).count();
    assert_eq!((assigned, unassigned), (2, 1));
}

#[tokio::test]
async fn milestones_one_per_issue_with_events() {
    let fixture = Fixture::new().await;
    let repo = fixture.repo(&fixture.alice).await;
    let issues = &fixture.services.issues;
    let make = |title: &str| MilestoneInput { title: title.into(), description: "Plan".into(), due_on: "2026-12-31".into() };
    let v1 = issues.create_milestone(&fixture.alice, &repo, &make("v1")).await.unwrap();
    let v2 = issues.create_milestone(&fixture.alice, &repo, &make("v2")).await.unwrap();
    assert!(matches!(issues.create_milestone(&fixture.alice, &repo, &make("V1")).await, Err(ServiceError::Conflict(_))));
    assert!(matches!(
        issues
            .create_milestone(
                &fixture.alice,
                &repo,
                &MilestoneInput { title: "v3".into(), due_on: "2026-02-30".into(), ..MilestoneInput::default() }
            )
            .await,
        Err(ServiceError::Invalid(_))
    ));
    let number = fixture.create(&fixture.alice, "Crash", "").await;
    let ctx = fixture.open(&fixture.alice, number).await;
    issues.set_milestone(&fixture.alice, &ctx, Some(v1.id)).await.unwrap();
    issues.set_milestone(&fixture.alice, &ctx, Some(v1.id)).await.unwrap();
    issues.set_milestone(&fixture.alice, &ctx, Some(v2.id)).await.unwrap();
    issues.set_milestone(&fixture.alice, &ctx, None).await.unwrap();
    let events = fixture.events_of(&fixture.alice, number).await;
    let kinds: Vec<&str> = events.iter().map(TimelineEvent::kind).collect();
    assert_eq!(kinds, ["milestoned", "demilestoned", "milestoned", "demilestoned"]);

    issues.set_milestone(&fixture.alice, &ctx, Some(v2.id)).await.unwrap();
    let progress = issues.milestone(&repo, v2.id).await.unwrap();
    assert_eq!((progress.open_issues, progress.closed_issues), (1, 0));
    issues.set_milestone_state(&repo, v2.id, IssueState::Closed).await.unwrap();
    assert_eq!(issues.milestone_counts(&repo).await.unwrap(), (1, 1));
    issues.delete_milestone(&repo, v2.id).await.unwrap();
    assert!(issues.metadata(&[ctx.issue.id]).await.unwrap()[&ctx.issue.id].milestone.is_none());
    assert!(matches!(issues.milestone(&repo, v2.id).await, Err(ServiceError::NotFound)));
}

#[tokio::test]
async fn only_admins_delete_issues_and_never_pull_requests() {
    let fixture = Fixture::new().await;
    let repo = fixture.repo(&fixture.alice).await;
    fixture.services.collaborators.add(&fixture.alice, &repo, "bob", Permission::Write).await.unwrap();
    let number = fixture.create(&fixture.bob, "Spam", "").await;
    let as_bob = fixture.open(&fixture.bob, number).await;
    assert!(matches!(fixture.services.issues.delete(&fixture.bob, &as_bob).await, Err(ServiceError::Forbidden)), "writers cannot");

    let ctx = fixture.open(&fixture.alice, number).await;
    let mut events = fixture.services.events.subscribe();
    fixture.services.issues.delete(&fixture.alice, &ctx).await.unwrap();
    assert!(matches!(issue_actions(&mut events).as_slice(), [IssueAction::Deleted { snapshot }] if snapshot.title.as_str() == "Spam"));
    assert!(matches!(fixture.services.timeline.open(Some(&fixture.alice), repo.clone(), number).await, Err(ServiceError::NotFound)));
}

#[tokio::test]
async fn searches_with_queries_hints_and_counts() {
    let fixture = Fixture::new().await;
    let repo = fixture.repo(&fixture.alice).await;
    let issues = &fixture.services.issues;
    let first = fixture.create(&fixture.alice, "Crash on boot", "").await;
    fixture.create(&fixture.bob, "Slow menu", "").await;
    let ctx = fixture.open(&fixture.alice, first).await;
    fixture.services.timeline.set_state(&fixture.alice, &ctx, StateChange::Close(StateReason::Completed)).await.unwrap();

    let results = issues.search(Some(&fixture.alice), &repo, &IssueQuery::parse(""), 1).await.unwrap();
    assert_eq!((results.total, results.open, results.closed), (1, 1, 1));
    assert_eq!(results.issues[0].title.as_str(), "Slow menu");
    assert_eq!(results.authors[&fixture.bob.id()].name.as_str(), "bob");
    let mine = issues.search(Some(&fixture.alice), &repo, &IssueQuery::parse("is:closed author:@me"), 1).await.unwrap();
    assert_eq!(mine.issues.len(), 1);
    let anonymous = issues.search(None, &repo, &IssueQuery::parse("author:@me wat:x"), 1).await.unwrap();
    assert_eq!(anonymous.hints.len(), 2, "{:?}", anonymous.hints);
    let nobody = issues.search(None, &repo, &IssueQuery::parse("author:nobody"), 1).await.unwrap();
    assert_eq!(nobody.total, 0);
    let text = issues.search(None, &repo, &IssueQuery::parse("is:closed boo"), 9).await.unwrap();
    assert_eq!((text.total, text.page), (1, 1), "pages past the end are clamped");
}

#[tokio::test]
async fn mentions_of_other_threads_become_cross_references_once() {
    let fixture = Fixture::new().await;
    let issues = &fixture.services.issues;
    let target = fixture.create(&fixture.alice, "Target", "").await;
    let source = fixture.create(&fixture.bob, "Source", "Related to #1 and `#1` and alice/demo#1").await;
    let source_ctx = fixture.open(&fixture.bob, source).await;
    let opened = DomainEvent::Issue {
        repo: source_ctx.issue.repo_id,
        issue: source_ctx.issue.id,
        number: source,
        kind: IssueKind::Issue,
        actor: Some(fixture.bob.id()),
        action: IssueAction::Opened,
    };
    issues.handle_event(&opened).await.unwrap();
    issues.handle_event(&opened).await.unwrap();
    let comment = fixture.services.timeline.comment(&fixture.bob, &source_ctx, "see #1 again; also #2 itself and #99").await.unwrap();
    let commented = DomainEvent::IssueComment {
        repo: source_ctx.issue.repo_id,
        issue: source_ctx.issue.id,
        number: source,
        kind: IssueKind::Issue,
        comment: comment.id,
        actor: fixture.bob.id(),
        action: rbit_core::issue::CommentAction::Created,
    };
    issues.handle_event(&commented).await.unwrap();
    let events = fixture.events_of(&fixture.alice, target).await;
    assert_eq!(events, [TimelineEvent::CrossReferenced { source: source_ctx.issue.id }], "once per source");
    assert_eq!(fixture.events_of(&fixture.alice, source).await, [], "a thread never references itself");

    // References into a private repository its author cannot read are not recorded.
    let private = CreateRepository { name: "vault".into(), description: String::new(), visibility: Visibility::Private };
    fixture.services.repositories.create(&fixture.alice, private).await.unwrap();
    let vault = fixture.services.repositories.open(Some(&fixture.alice), "alice", "vault").await.unwrap();
    let secret = issues.create(&fixture.alice, &vault, NewIssueInput { title: "Secret".into(), ..NewIssueInput::default() }).await.unwrap();
    let probe = fixture.create(&fixture.bob, "Probe", "alice/vault#1").await;
    let probe_ctx = fixture.open(&fixture.bob, probe).await;
    let opened = DomainEvent::Issue {
        repo: probe_ctx.issue.repo_id,
        issue: probe_ctx.issue.id,
        number: probe,
        kind: IssueKind::Issue,
        actor: Some(fixture.bob.id()),
        action: IssueAction::Opened,
    };
    issues.handle_event(&opened).await.unwrap();
    let vault_ctx = fixture.services.timeline.open(Some(&fixture.alice), vault, secret.number).await.unwrap();
    let timeline = fixture.services.timeline.timeline(Some(&fixture.alice), &vault_ctx, None, 50).await.unwrap();
    assert_eq!(timeline.entries.len(), 0);
}

#[tokio::test]
async fn pushed_commits_reference_and_close_issues_on_the_default_branch_only() {
    let fixture = Fixture::new().await;
    let issues = &fixture.services.issues;
    let first = fixture.create(&fixture.alice, "Crash", "").await;
    let second = fixture.create(&fixture.alice, "Typo", "").await;
    let repo = fixture.repo(&fixture.alice).await;
    let path = repo.disk_path.to_str().unwrap().to_owned();

    let work = tempfile::tempdir().unwrap();
    git(work.path(), &["init", "-q"]);
    std::fs::write(work.path().join("a.txt"), "a\n").unwrap();
    git(work.path(), &["add", "."]);
    git(work.path(), &["commit", "-q", "-m", "Start"]);
    git(work.path(), &["push", "-q", &path, "HEAD:refs/heads/main"]);
    let base = git(work.path(), &["rev-parse", "HEAD"]);
    git(work.path(), &["checkout", "-q", "-b", "feature"]);
    std::fs::write(work.path().join("a.txt"), "b\n").unwrap();
    git(work.path(), &["commit", "-q", "-am", "Fix the crash", "-m", "Fixes #1, see #2 and #99"]);
    let fix = git(work.path(), &["rev-parse", "HEAD"]);
    git(work.path(), &["push", "-q", &path, "feature"]);

    let pushed = |name: &str, before: Option<&str>, after: &str, pusher| DomainEvent::RepositoryPushed {
        repo: repo.repo.repo.id,
        pusher,
        updates: vec![RefUpdate { name: name.to_owned(), before: before.map(str::to_owned), after: Some(after.to_owned()) }],
    };
    issues.handle_event(&pushed("refs/heads/feature", None, &fix, Some(fixture.alice.id()))).await.unwrap();
    let ctx = fixture.open(&fixture.alice, first).await;
    assert_eq!(ctx.issue.state, IssueState::Open, "a feature branch only references");
    let referenced = TimelineEvent::CommitReferenced { commit: fix.clone(), summary: "Fix the crash".into() };
    assert_eq!(fixture.events_of(&fixture.alice, first).await, std::slice::from_ref(&referenced));
    assert_eq!(fixture.events_of(&fixture.alice, second).await, std::slice::from_ref(&referenced));

    // The same commit reaching main (a deploy key: no pusher) closes #1, without a second reference.
    git(work.path(), &["push", "-q", &path, "feature:main"]);
    issues.handle_event(&pushed("refs/heads/main", Some(&base), &fix, None)).await.unwrap();
    let ctx = fixture.open(&fixture.alice, first).await;
    assert_eq!((ctx.issue.state, ctx.issue.closed_by), (IssueState::Closed, None));
    let closed = TimelineEvent::Closed { reason: StateReason::Completed, commit: Some(fix.clone()), pull: None };
    assert_eq!(fixture.events_of(&fixture.alice, first).await, [referenced.clone(), closed]);
    assert_eq!(fixture.open(&fixture.alice, second).await.issue.state, IssueState::Open, "\"see\" does not close");
    assert_eq!(fixture.events_of(&fixture.alice, second).await, [referenced]);
}

#[tokio::test]
async fn merged_pull_requests_close_the_issues_their_body_names() {
    use rbit_storage::sqlite::pulls::{CreatedPull, MergeRecord, NewPull};

    let fixture = Fixture::new().await;
    let first = fixture.create(&fixture.alice, "Crash", "").await;
    let second = fixture.create(&fixture.alice, "Other crash", "").await;
    let repo = fixture.repo(&fixture.alice).await;
    let pulls = fixture.database.pulls();
    // Pull requests into `main` (the default branch) and into `release`, both naming an issue.
    let open = |head: &str, base: &str, body: &str| NewPull {
        issue: rbit_storage::sqlite::issues::NewIssue {
            repo: repo.repo.repo.id,
            kind: IssueKind::Pull,
            title: rbit_core::IssueTitle::parse("Fix").unwrap(),
            body: rbit_core::IssueText::body(body).unwrap(),
            author: fixture.alice.id(),
            at: rbit_core::Timestamp::now(),
        },
        head_repo: repo.repo.repo.id,
        head_branch: head.to_owned(),
        base_branch: base.to_owned(),
        head_sha: "a".repeat(40),
        is_draft: false,
    };
    let mut merged = Vec::new();
    for (head, base, body) in [("fix", "main", "This resolves #1."), ("backport", "release", "Fixes #2.")] {
        let CreatedPull::Created(created) = pulls.create(&open(head, base, body)).await.unwrap() else { panic!("a new pull request") };
        let (pull, _) = *created;
        let record = MergeRecord {
            merged_by: Some(fixture.bob.id()),
            strategy: None,
            commit: "b",
            base: "c",
            head: "a",
            at: rbit_core::Timestamp::now(),
        };
        assert!(pulls.record_merge(pull.id, &record).await.unwrap());
        let event = DomainEvent::Issue {
            repo: repo.repo.repo.id,
            issue: pull.id,
            number: pull.number,
            kind: IssueKind::Pull,
            actor: Some(fixture.bob.id()),
            action: IssueAction::Closed { reason: StateReason::Merged },
        };
        fixture.services.issues.handle_event(&event).await.unwrap();
        merged.push(pull.number);
    }
    let ctx = fixture.open(&fixture.alice, first).await;
    assert_eq!((ctx.issue.state, ctx.issue.closed_by), (IssueState::Closed, Some(fixture.bob.id())));
    assert_eq!(
        fixture.events_of(&fixture.alice, first).await,
        [TimelineEvent::Closed { reason: StateReason::Completed, commit: None, pull: Some(merged[0]) }]
    );
    assert_eq!(fixture.open(&fixture.alice, second).await.issue.state, IssueState::Open, "merging into another branch closes nothing");
}

#[tokio::test]
async fn triage_spends_the_content_budget() {
    let fixture = Fixture::with(|config| config.security.rate_limit = true).await;
    let number = fixture.create(&fixture.alice, "Crash", "").await;
    let ctx = fixture.open(&fixture.alice, number).await;
    let bug = fixture.label("bug").await;
    let mut limited = false;
    for _ in 0..40 {
        let result = fixture.services.issues.set_labels(&fixture.alice, &ctx, &[bug]).await;
        let result = match result {
            Ok(()) => fixture.services.issues.set_labels(&fixture.alice, &ctx, &[]).await,
            other => other,
        };
        if matches!(result, Err(ServiceError::RateLimited { .. })) {
            limited = true;
            break;
        }
    }
    assert!(limited, "label changes are throttled like comments");
}
