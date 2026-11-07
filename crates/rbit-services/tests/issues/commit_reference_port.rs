//! Actual Git push and merged-pull reference policy parity.
use super::*;
use rbit_storage::count_work;

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
    let (result, work_count) =
        count_work(issues.handle_event(&pushed("refs/heads/feature", None, &fix, PushSource::User(fixture.alice.id())))).await;
    result.unwrap();
    assert_eq!((work_count.acquisitions, work_count.statements), (7, 9));
    let ctx = fixture.open(&fixture.alice, first).await;
    assert_eq!(ctx.issue.state, IssueState::Open, "a feature branch only references");
    let referenced = TimelineEvent::CommitReferenced { commit: fix.clone(), summary: "Fix the crash".into() };
    assert_eq!(fixture.events_of(&fixture.alice, first).await, std::slice::from_ref(&referenced));
    assert_eq!(fixture.events_of(&fixture.alice, second).await, std::slice::from_ref(&referenced));

    // The same commit reaching main (a deploy key: no pusher) closes #1, without a second reference.
    git(work.path(), &["push", "-q", &path, "feature:main"]);
    let (result, work_count) = count_work(issues.handle_event(&pushed("refs/heads/main", Some(&base), &fix, PushSource::DeployKey))).await;
    result.unwrap();
    assert_eq!((work_count.acquisitions, work_count.statements), (6, 8));
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
        let (result, work_count) = count_work(fixture.services.issues.handle_event(&event)).await;
        result.unwrap();
        assert_eq!((work_count.acquisitions, work_count.statements), if base == "main" { (5, 7) } else { (2, 2) });
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
