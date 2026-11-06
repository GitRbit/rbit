//! Actual assignee storage/grant/event paths and work-count parity.

use super::{Fixture, issue_actions, user};
use rbit_core::{AccountId, TimelineEvent, Timestamp, issue::IssueAction, membership::Permission};
use rbit_services::ServiceError;
use rbit_storage::count_work;
use tokio::sync::broadcast::error::TryRecvError;

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
async fn add_remove_replace_and_noop_paths_preserve_exact_sql_work_and_events() {
    let fixture = Fixture::new().await;
    let number = fixture.create(&fixture.alice, "Assignments", "").await;
    let ctx = fixture.open(&fixture.alice, number).await;
    let issues = &fixture.services.issues;
    let alice = fixture.alice.id();
    let mut events = fixture.services.events.subscribe();
    let (result, work) = count_work(issues.change_assignees(&fixture.alice, &ctx, &[alice, alice], &[])).await;
    result.unwrap();
    assert_eq!((work.acquisitions, work.statements), (4, 5));
    let (result, work) = count_work(issues.change_assignees(&fixture.alice, &ctx, &[alice], &[alice])).await;
    result.unwrap();
    assert_eq!((work.acquisitions, work.statements), (1, 1));
    let (result, work) = count_work(issues.set_assignees(&fixture.alice, &ctx, &[alice])).await;
    result.unwrap();
    assert_eq!((work.acquisitions, work.statements), (2, 2));
    let (result, work) = count_work(issues.set_assignees(&fixture.alice, &ctx, &[])).await;
    result.unwrap();
    assert_eq!((work.acquisitions, work.statements), (4, 5));
    let (result, work) = count_work(issues.change_assignees(&fixture.alice, &ctx, &[], &[alice])).await;
    result.unwrap();
    assert_eq!((work.acquisitions, work.statements), (1, 1));
    let (result, work) = count_work(issues.set_assignees(&fixture.alice, &ctx, &[alice])).await;
    result.unwrap();
    assert_eq!((work.acquisitions, work.statements), (5, 6));
    assert!(
        matches!(issue_actions(&mut events).as_slice(), [IssueAction::Assigned { assignee: first }, IssueAction::Unassigned { assignee: second }, IssueAction::Assigned { assignee: third }] if *first==alice && *second==alice && *third==alice)
    );
    assert_eq!(fixture.events_of(&fixture.alice, number).await.len(), 3);
}

#[tokio::test]
async fn triage_archive_unknown_and_grant_errors_keep_query_precedence() {
    let fixture = Fixture::new().await;
    let number = fixture.create(&fixture.bob, "Reader", "").await;
    let mut reader = fixture.open(&fixture.bob, number).await;
    reader.permissions.can_triage = true;
    reader.repo.repo.repo.archived_at = Some(Timestamp(1));
    let issues = &fixture.services.issues;
    let unknown = AccountId(i64::MAX);
    let (result, work) = count_work(issues.set_assignees(&fixture.bob, &reader, &[unknown])).await;
    assert!(matches!(result, Err(ServiceError::Forbidden)));
    assert_eq!(work.statements, 0);
    let mut writer = fixture.open(&fixture.alice, number).await;
    writer.permissions.can_triage = false;
    writer.repo.repo.repo.archived_at = Some(Timestamp(1));
    let (result, work) = count_work(issues.change_assignees(&fixture.alice, &writer, &[unknown], &[])).await;
    assert!(matches!(result, Err(ServiceError::Archived)));
    assert_eq!(work.statements, 0);
    writer.repo.repo.repo.archived_at = None;
    let (result, work) = count_work(issues.change_assignees(&fixture.alice, &writer, &[unknown], &[])).await;
    assert!(matches!(result, Err(ServiceError::Invalid(message)) if message=="unknown user"));
    assert_eq!((work.acquisitions, work.statements), (2, 2));
    let (result, work) = count_work(issues.set_assignees(&fixture.alice, &writer, &[unknown])).await;
    assert!(matches!(result, Err(ServiceError::Invalid(message)) if message=="unknown user"));
    assert_eq!((work.acquisitions, work.statements), (3, 3));
    let (result, work) = count_work(issues.change_assignees(&fixture.alice, &writer, &[fixture.bob.id()], &[])).await;
    assert!(
        matches!(result, Err(ServiceError::Invalid(message)) if message=="bob cannot be assigned: only people who can push to the repository can")
    );
    assert_eq!((work.acquisitions, work.statements), (3, 3));
}

#[tokio::test]
async fn existing_assignee_is_not_revalidated_but_a_new_suspended_assignment_is_rejected() {
    let fixture = Fixture::new().await;
    let repo = fixture.repo(&fixture.alice).await;
    fixture.services.collaborators.add(&fixture.alice, &repo, "bob", Permission::Write).await.unwrap();
    let number = fixture.create(&fixture.alice, "Assignments", "").await;
    let owner_ctx = fixture.open(&fixture.alice, number).await;
    let issues = &fixture.services.issues;
    let alice = fixture.alice.id();
    issues.change_assignees(&fixture.alice, &owner_ctx, &[alice], &[]).await.unwrap();
    let writer_ctx = fixture.open(&fixture.bob, number).await;
    fixture.database.admin().set_suspended(alice, true).await.unwrap();
    let (result, work) = count_work(issues.change_assignees(&fixture.bob, &writer_ctx, &[alice], &[alice])).await;
    result.unwrap();
    assert_eq!((work.acquisitions, work.statements), (1, 1));
    issues.change_assignees(&fixture.bob, &writer_ctx, &[], &[alice]).await.unwrap();
    let (result, work) = count_work(issues.change_assignees(&fixture.bob, &writer_ctx, &[alice], &[])).await;
    assert!(matches!(result, Err(ServiceError::Invalid(message)) if message=="unknown user"));
    assert_eq!((work.acquisitions, work.statements), (2, 2));
}

#[tokio::test]
async fn concurrent_identical_adds_publish_once_for_the_sql_unique_winner() {
    let fixture = Fixture::new().await;
    let number = fixture.create(&fixture.alice, "Assignments", "").await;
    let ctx = fixture.open(&fixture.alice, number).await;
    let issues = &fixture.services.issues;
    let ids = [fixture.alice.id()];
    let mut events = fixture.services.events.subscribe();
    let (first, second) =
        tokio::join!(issues.change_assignees(&fixture.alice, &ctx, &ids, &[]), issues.change_assignees(&fixture.alice, &ctx, &ids, &[]));
    first.unwrap();
    second.unwrap();
    assert!(matches!(issue_actions(&mut events).as_slice(), [IssueAction::Assigned { assignee }] if *assignee==fixture.alice.id()));
    assert_eq!(fixture.database.issue_meta().assignees_of(&[ctx.issue.id]).await.unwrap().len(), 1);
}

#[tokio::test]
async fn timeline_failure_keeps_committed_assignment_and_retry_does_not_repair_history() {
    use sqlx::Connection as _;
    let fixture = Fixture::new().await;
    let number = fixture.create(&fixture.alice, "Assignments", "").await;
    let ctx = fixture.open(&fixture.alice, number).await;
    let options = sqlx::sqlite::SqliteConnectOptions::new().filename(fixture.directory.path().join("rbit.db"));
    let mut connection = sqlx::SqliteConnection::connect_with(&options).await.unwrap();
    sqlx::query("CREATE TRIGGER fixture_reject_assignee_event BEFORE INSERT ON issue_events BEGIN SELECT RAISE(ABORT, 'fixture assignee event failure'); END").execute(&mut connection).await.unwrap();
    let mut events = fixture.services.events.subscribe();
    let issues = &fixture.services.issues;
    let ids = [fixture.alice.id()];
    let (result, work) = count_work(issues.change_assignees(&fixture.alice, &ctx, &ids, &[])).await;
    assert!(matches!(result, Err(ServiceError::Internal(_))));
    assert_eq!((work.acquisitions, work.statements), (4, 4));
    assert_eq!(fixture.database.issue_meta().assignees_of(&[ctx.issue.id]).await.unwrap().len(), 1);
    assert_eq!(fixture.events_of(&fixture.alice, number).await.len(), 0);
    assert!(matches!(events.try_recv(), Err(TryRecvError::Empty)));
    sqlx::query("DROP TRIGGER fixture_reject_assignee_event").execute(&mut connection).await.unwrap();
    let (result, work) = count_work(issues.change_assignees(&fixture.alice, &ctx, &ids, &[])).await;
    result.unwrap();
    assert_eq!((work.acquisitions, work.statements), (1, 1));
    assert_eq!(fixture.events_of(&fixture.alice, number).await.len(), 0);
    assert!(matches!(events.try_recv(), Err(TryRecvError::Empty)));
    connection.close().await.unwrap();
}
#[tokio::test]
async fn replacement_preserves_new_user_grant_lookup_and_remove_before_add_effects() {
    let fixture = Fixture::new().await;
    let repo = fixture.repo(&fixture.alice).await;
    fixture.services.collaborators.add(&fixture.alice, &repo, "bob", Permission::Write).await.unwrap();
    let number = fixture.create(&fixture.alice, "Assignments", "").await;
    let ctx = fixture.open(&fixture.alice, number).await;
    let issues = &fixture.services.issues;
    issues.change_assignees(&fixture.alice, &ctx, &[fixture.alice.id()], &[]).await.unwrap();
    let mut events = fixture.services.events.subscribe();
    let (result, work) = count_work(issues.set_assignees(&fixture.alice, &ctx, &[fixture.bob.id(), fixture.bob.id()])).await;
    result.unwrap();
    assert_eq!((work.acquisitions, work.statements), (8, 10));
    assert!(
        matches!(issue_actions(&mut events).as_slice(), [IssueAction::Unassigned { assignee: previous }, IssueAction::Assigned { assignee: next }] if *previous==fixture.alice.id() && *next==fixture.bob.id())
    );
    let roster = fixture.database.issue_meta().assignees_of(&[ctx.issue.id]).await.unwrap();
    assert_eq!(roster.iter().map(|(_, account)| account.id).collect::<Vec<_>>(), [fixture.bob.id()]);
}
