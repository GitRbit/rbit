//! The issue service (workstream I) against real stores: creation with metadata, label
//! and assignee rules, milestones, deletion, search, cross-references and references
//! from pushed commits.

use std::{path::Path, process::Command, sync::Arc};

use rbit_core::{
    Config, IssueKind, IssueNumber, IssueState, PushSource, StateReason, TimelineEvent, User, Visibility, issue::IssueAction,
    issue_query::IssueQuery, membership::Permission,
};
use rbit_services::{
    CreateRepository, DomainEvent, IssueContext, RepoContext, ServiceError, Services,
    events::RefUpdate,
    issues::{LabelInput, NewIssueInput},
    timeline::{StateChange, TimelineEntry},
};
use rbit_storage::{Database, KvStore, NewUser};
use tokio::sync::broadcast::{Receiver, error::TryRecvError};

struct Fixture {
    directory: tempfile::TempDir,
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
        let database = Database::open(&dir.path().join("rbit.db")).await.unwrap();
        let kv = KvStore::open(&dir.path().join("rbit.redb")).unwrap();
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
        Self { directory: dir, database, services, alice, bob, carol }
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

mod read_create_port;

mod labels_port;

mod assignees_port;

mod milestones_port;

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

mod reference_port;

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

    let pushed = |name: &str, before: Option<&str>, after: &str, source| DomainEvent::RepositoryPushed {
        repo: repo.repo.repo.id,
        source,
        updates: vec![RefUpdate { name: name.to_owned(), before: before.map(str::to_owned), after: Some(after.to_owned()) }],
    };
    issues.handle_event(&pushed("refs/heads/feature", None, &fix, PushSource::User(fixture.alice.id()))).await.unwrap();
    let ctx = fixture.open(&fixture.alice, first).await;
    assert_eq!(ctx.issue.state, IssueState::Open, "a feature branch only references");
    let referenced = TimelineEvent::CommitReferenced { commit: fix.clone(), summary: "Fix the crash".into() };
    assert_eq!(fixture.events_of(&fixture.alice, first).await, std::slice::from_ref(&referenced));
    assert_eq!(fixture.events_of(&fixture.alice, second).await, std::slice::from_ref(&referenced));

    // The same commit reaching main (a deploy key: no pusher) closes #1, without a second reference.
    git(work.path(), &["push", "-q", &path, "feature:main"]);
    issues.handle_event(&pushed("refs/heads/main", Some(&base), &fix, PushSource::DeployKey)).await.unwrap();
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

#[tokio::test]
async fn personal_dashboards_filter_and_revalidate_access() {
    use rbit_services::issues::DashboardRelation;
    let fixture = Fixture::new().await;
    let first = fixture.create(&fixture.bob, "First report", "Please ask @carol; `@alice` is code.").await;
    fixture.create(&fixture.alice, "Other report", "").await;
    let ctx = fixture.open(&fixture.bob, first).await;
    let issues = &fixture.services.issues;
    let query = IssueQuery::parse("is:open sort:updated-desc");
    let result = issues.dashboard(&fixture.bob, IssueKind::Issue, DashboardRelation::Created, &query, u32::MAX).await.unwrap();
    assert_eq!(result.total, 1);
    assert_eq!(result.page, 1);
    assert_eq!(result.issues[0].id, ctx.issue.id);
    let conflicting = IssueQuery::parse("author:alice label:absent");
    assert_eq!(issues.dashboard(&fixture.bob, IssueKind::Issue, DashboardRelation::Created, &conflicting, 1).await.unwrap().total, 0);
    issues
        .handle_event(&DomainEvent::Issue {
            repo: ctx.issue.repo_id,
            issue: ctx.issue.id,
            number: first,
            kind: IssueKind::Issue,
            actor: Some(fixture.bob.id()),
            action: IssueAction::Opened,
        })
        .await
        .unwrap();
    assert_eq!(issues.dashboard(&fixture.carol, IssueKind::Issue, DashboardRelation::Mentioned, &query, 1).await.unwrap().total, 1);
    assert_eq!(issues.dashboard(&fixture.alice, IssueKind::Issue, DashboardRelation::Mentioned, &query, 1).await.unwrap().total, 0);
    fixture.database.admin().set_suspended(fixture.bob.id(), true).await.unwrap();
    assert!(matches!(
        issues.dashboard(&fixture.bob, IssueKind::Issue, DashboardRelation::Created, &query, 1).await,
        Err(ServiceError::NotFound)
    ));
}

#[tokio::test]
async fn existing_mentions_are_backfilled_on_upgrade() {
    use rbit_services::issues::DashboardRelation;
    let dir = tempfile::tempdir().unwrap();
    let database = Database::open(&dir.path().join("rbit.db")).await.unwrap();
    let alice = user(&database, "alice").await;
    let bob = user(&database, "bob").await;
    let repo = database
        .repositories()
        .create(rbit_storage::NewRepository {
            owner_id: alice.id(),
            name: "demo".parse().unwrap(),
            description: None,
            visibility: Visibility::Public,
            default_branch: "main".into(),
        })
        .await
        .unwrap();
    database
        .issues()
        .create(&rbit_storage::sqlite::issues::NewIssue {
            repo: repo.id,
            kind: IssueKind::Issue,
            title: rbit_core::IssueTitle::parse("Historical").unwrap(),
            body: rbit_core::IssueText::body("Hello @bob; `@alice` is code").unwrap(),
            author: alice.id(),
            at: rbit_core::Timestamp(1),
        })
        .await
        .unwrap();
    let kv = KvStore::open(&dir.path().join("rbit.redb")).unwrap();
    let mut config = Config::default();
    config.storage.data_dir = dir.path().to_path_buf();
    let services = Services::new(Arc::new(config), &database, &kv).await.unwrap();
    let query = IssueQuery::parse("is:open");
    assert_eq!(services.issues.dashboard(&bob, IssueKind::Issue, DashboardRelation::Mentioned, &query, 1).await.unwrap().total, 1);
    assert_eq!(services.issues.dashboard(&alice, IssueKind::Issue, DashboardRelation::Mentioned, &query, 1).await.unwrap().total, 0);
    assert!(database.issue_search().mentions_indexed().await.unwrap());
}

#[tokio::test]
async fn personal_dashboard_pages_are_global_and_stable_across_repositories() {
    use rbit_services::issues::DashboardRelation;
    let fixture = Fixture::new().await;
    let second = fixture
        .services
        .repositories
        .create(&fixture.bob, CreateRepository { name: "other".into(), visibility: Visibility::Public, description: String::new() })
        .await
        .unwrap();
    let first = fixture.repo(&fixture.bob).await;
    let second = fixture.services.repositories.open(Some(&fixture.bob), "bob", second.repo.name.as_str()).await.unwrap();
    for n in 0..30 {
        let repo = if n % 2 == 0 { &first } else { &second };
        fixture
            .services
            .issues
            .create(&fixture.bob, repo, NewIssueInput { title: format!("Report {n}"), ..Default::default() })
            .await
            .unwrap();
    }
    let query = IssueQuery::parse("is:open sort:created-asc");
    let issues = &fixture.services.issues;
    let one = issues.dashboard(&fixture.bob, IssueKind::Issue, DashboardRelation::Created, &query, 0).await.unwrap();
    let two = issues.dashboard(&fixture.bob, IssueKind::Issue, DashboardRelation::Created, &query, u32::MAX).await.unwrap();
    assert_eq!((one.total, one.open, one.closed, one.page_count()), (30, 30, 0, 2));
    assert_eq!(one.issues.len(), 25);
    assert_eq!(two.page, 2);
    assert_eq!(two.issues.len(), 5);
    assert_eq!(one.issues.first().unwrap().title.as_str(), "Report 0");
    assert_eq!(two.issues.last().unwrap().title.as_str(), "Report 29");
    assert!(!two.issues.iter().any(|issue| one.issues.iter().any(|other| other.id == issue.id)));
}

#[tokio::test]
async fn delayed_events_preserve_mentions_after_edits_and_comment_deletion() {
    use rbit_services::issues::DashboardRelation;
    let fixture = Fixture::new().await;
    let mut events = fixture.services.events.subscribe();
    let number = fixture.create(&fixture.alice, "Fast changes", "Please check @bob").await;
    let ctx = fixture.open(&fixture.alice, number).await;
    fixture.services.timeline.edit(&fixture.alice, &ctx, None, Some("Mention removed")).await.unwrap();
    let comment = fixture.services.timeline.comment(&fixture.alice, &ctx, "Please check @carol").await.unwrap();
    fixture.services.timeline.delete_comment(&fixture.alice, &ctx, comment.id).await.unwrap();
    // All events are handled only after the old bodies have disappeared from storage.
    while let Ok(event) = events.try_recv() {
        fixture.services.issues.handle_event(&event).await.unwrap();
    }
    let query = IssueQuery::parse("is:open");
    for viewer in [&fixture.bob, &fixture.carol] {
        let results = fixture.services.issues.dashboard(viewer, IssueKind::Issue, DashboardRelation::Mentioned, &query, 1).await.unwrap();
        assert_eq!(results.total, 1);
        assert_eq!(results.issues[0].id, ctx.issue.id);
    }
}

#[tokio::test]
async fn review_mention_history_is_indexed_only_after_submission() {
    use rbit_core::{
        IssueText, IssueTitle, Timestamp,
        issue::{ReviewAction, ReviewState},
        pull::ReviewSide,
    };
    use rbit_services::issues::DashboardRelation;
    use rbit_storage::sqlite::{
        issues::NewIssue,
        pulls::{CreatedPull, NewPull},
        reviews::NewReviewComment,
    };
    let fixture = Fixture::new().await;
    let repo = fixture.repo(&fixture.alice).await.repo.repo.id;
    let CreatedPull::Created(created) = fixture
        .database
        .pulls()
        .create(&NewPull {
            issue: NewIssue {
                repo,
                kind: IssueKind::Pull,
                title: IssueTitle::parse("Review privacy").unwrap(),
                body: IssueText::body("").unwrap(),
                author: fixture.alice.id(),
                at: Timestamp(1),
            },
            head_repo: repo,
            head_branch: "feature/review".into(),
            base_branch: "main".into(),
            head_sha: "abc".into(),
            is_draft: false,
        })
        .await
        .unwrap()
    else {
        panic!("new pull");
    };
    let (issue, _) = *created;
    let store = fixture.database.reviews();
    let (pending, _) = store
        .add_pending_comment(
            issue.id,
            fixture.alice.id(),
            "abc",
            &NewReviewComment {
                path: "file.txt".into(),
                side: ReviewSide::Right,
                line: 1,
                commit_sha: "abc".into(),
                diff_hunk: String::new(),
                in_reply_to: None,
                body: "Pending @bob".into(),
            },
            Timestamp(2),
        )
        .await
        .unwrap();
    let edited = DomainEvent::PullRequestReview {
        repo,
        pull: issue.id,
        number: issue.number,
        review: pending.id,
        actor: fixture.alice.id(),
        action: ReviewAction::Edited { previous_body: "Previously @carol".into() },
    };
    let issues = &fixture.services.issues;
    issues.handle_event(&edited).await.unwrap();
    let query = IssueQuery::parse("is:open");
    for viewer in [&fixture.bob, &fixture.carol] {
        assert_eq!(issues.dashboard(viewer, IssueKind::Pull, DashboardRelation::Mentioned, &query, 1).await.unwrap().total, 0);
    }
    store.submit_pending(issue.id, fixture.alice.id(), ReviewState::Commented, "Previously @carol", "abc", Timestamp(3)).await.unwrap();
    store.edit_body(issue.id, pending.id, "Removed mention").await.unwrap();
    issues.handle_event(&edited).await.unwrap();
    for viewer in [&fixture.bob, &fixture.carol] {
        assert_eq!(issues.dashboard(viewer, IssueKind::Pull, DashboardRelation::Mentioned, &query, 1).await.unwrap().total, 1);
    }
}
