//! Cross-reference visibility, ordered dedup and real SQL effects.
use super::*;
use rbit_storage::count_work;

// Actions: task tokens.
#[tokio::test]
async fn workflow_comments_record_no_cross_reference_by_a_ghost() {
    use rbit_core::actions::{JobId, PermissionLevel, RunId, TaskId, TokenPermissions};

    let fixture = Fixture::new().await;
    let target = fixture.create(&fixture.alice, "Target", "").await;
    let source = fixture.create(&fixture.alice, "Source", "").await;
    let source_ctx = fixture.open(&fixture.alice, source).await;
    let grant = rbit_services::actions::tokens::TaskGrant {
        task: TaskId(1),
        job: JobId(1),
        run: RunId(1),
        repo: source_ctx.issue.repo_id,
        permissions: TokenPermissions::uniform(PermissionLevel::Write),
    };
    let comment = fixture.services.timeline.comment_for_task(&grant, &source_ctx, "Regressed since #1").await.unwrap();
    let commented = DomainEvent::IssueComment {
        repo: source_ctx.issue.repo_id,
        issue: source_ctx.issue.id,
        number: source,
        kind: IssueKind::Issue,
        comment: comment.id,
        actor: None,
        action: rbit_core::issue::CommentAction::Created,
    };
    fixture.services.issues.handle_event(&commented).await.unwrap();
    assert_eq!(fixture.events_of(&fixture.alice, target).await, [], "rbit Actions has no account to show for it");
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
        actor: Some(fixture.bob.id()),
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
async fn cross_reference_work_and_unique_race_remain_bounded_and_publish_one_history_record() {
    let fixture = Fixture::new().await;
    let target = fixture.create(&fixture.alice, "Target", "").await;
    let source = fixture.create(&fixture.alice, "Source", "#1 #1").await;
    let source = fixture.open(&fixture.alice, source).await.issue;
    let event = DomainEvent::Issue {
        repo: source.repo_id,
        issue: source.id,
        number: source.number,
        kind: source.kind,
        actor: Some(fixture.alice.id()),
        action: IssueAction::Opened,
    };
    let (result, work) = count_work(fixture.services.issues.handle_event(&event)).await;
    result.unwrap();
    assert_eq!((work.acquisitions, work.statements), (6, 7));
    let (result, work) = count_work(fixture.services.issues.handle_event(&event)).await;
    result.unwrap();
    assert_eq!((work.acquisitions, work.statements), (5, 5));
    assert_eq!(fixture.events_of(&fixture.alice, target).await, [TimelineEvent::CrossReferenced { source: source.id }]);
    let source2 = fixture.create(&fixture.alice, "Concurrent", "#1").await;
    let source2 = fixture.open(&fixture.alice, source2).await.issue;
    let event = DomainEvent::Issue {
        repo: source2.repo_id,
        issue: source2.id,
        number: source2.number,
        kind: source2.kind,
        actor: Some(fixture.alice.id()),
        action: IssueAction::Opened,
    };
    let (first, second) = tokio::join!(fixture.services.issues.handle_event(&event), fixture.services.issues.handle_event(&event));
    first.unwrap();
    second.unwrap();
    assert_eq!(
        fixture
            .events_of(&fixture.alice, target)
            .await
            .iter()
            .filter(|event| matches!(event,TimelineEvent::CrossReferenced {source} if *source==source2.id))
            .count(),
        1
    );
}
#[tokio::test]
async fn history_failure_leaves_unique_reference_link_and_retry_does_not_repair_it() {
    use sqlx::Connection as _;
    let fixture = Fixture::new().await;
    let target = fixture.create(&fixture.alice, "Target", "").await;
    let source = fixture.create(&fixture.alice, "Source", "#1").await;
    let source = fixture.open(&fixture.alice, source).await.issue;
    let options = sqlx::sqlite::SqliteConnectOptions::new().filename(fixture.directory.path().join("rbit.db"));
    let mut connection = sqlx::SqliteConnection::connect_with(&options).await.unwrap();
    sqlx::query("CREATE TRIGGER fixture_reject_reference_event BEFORE INSERT ON issue_events BEGIN SELECT RAISE(ABORT, 'fixture reference failure'); END").execute(&mut connection).await.unwrap();
    let event = DomainEvent::Issue {
        repo: source.repo_id,
        issue: source.id,
        number: source.number,
        kind: source.kind,
        actor: Some(fixture.alice.id()),
        action: IssueAction::Opened,
    };
    assert!(matches!(fixture.services.issues.handle_event(&event).await, Err(ServiceError::Internal(_))));
    assert_eq!(fixture.events_of(&fixture.alice, target).await, Vec::<TimelineEvent>::new());
    sqlx::query("DROP TRIGGER fixture_reject_reference_event").execute(&mut connection).await.unwrap();
    let (result, work) = count_work(fixture.services.issues.handle_event(&event)).await;
    result.unwrap();
    assert_eq!((work.acquisitions, work.statements), (5, 5));
    assert_eq!(fixture.events_of(&fixture.alice, target).await, Vec::<TimelineEvent>::new());
    connection.close().await.unwrap();
}
