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

mod fixture;
use fixture::Fixture;

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

mod deletion_port;
mod listing_port;
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

mod commit_reference_port;

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
