//! Existing label association/event commits and work counts through production adapters.

use super::{Fixture, issue_actions};
use rbit_core::{LabelId, Timestamp, issue::IssueAction};
use rbit_services::ServiceError;
use rbit_storage::count_work;
use tokio::sync::broadcast::error::TryRecvError;

#[tokio::test]
async fn addition_dedup_add_wins_and_noop_removal_keep_original_work_and_events() {
    let fixture = Fixture::new().await;
    let label = fixture.label("bug").await;
    let number = fixture.create(&fixture.alice, "Issue", "").await;
    let ctx = fixture.open(&fixture.alice, number).await;
    let issues = &fixture.services.issues;
    let mut events = fixture.services.events.subscribe();
    let (result, work) = count_work(issues.change_labels(&fixture.alice, &ctx, &[label, label], &[label, label, LabelId(i64::MAX)])).await;
    result.unwrap();
    assert_eq!((work.acquisitions, work.statements), (4, 5));
    assert!(matches!(issue_actions(&mut events).as_slice(),[IssueAction::Labeled{label:entry}] if entry.id==label));
    let (result, work) = count_work(issues.change_labels(&fixture.alice, &ctx, &[label, label], &[])).await;
    result.unwrap();
    assert_eq!((work.acquisitions, work.statements), (3, 3));
    assert!(issue_actions(&mut events).is_empty());
    let (result, work) = count_work(issues.change_labels(&fixture.alice, &ctx, &[], &[label, label])).await;
    result.unwrap();
    assert_eq!((work.acquisitions, work.statements), (3, 4));
    assert!(matches!(issue_actions(&mut events).as_slice(),[IssueAction::Unlabeled{label:entry}] if entry.id==label));
    let (result, work) = count_work(issues.change_labels(&fixture.alice, &ctx, &[], &[label])).await;
    result.unwrap();
    assert_eq!((work.acquisitions, work.statements), (1, 1));
    assert!(issue_actions(&mut events).is_empty());
}

#[tokio::test]
async fn exact_set_retains_initial_and_delegated_reads_and_replacement_order() {
    let fixture = Fixture::new().await;
    let first = fixture.label("a-first").await;
    let second = fixture.label("b-second").await;
    let number = fixture.create(&fixture.alice, "Issue", "").await;
    let ctx = fixture.open(&fixture.alice, number).await;
    let issues = &fixture.services.issues;
    let mut events = fixture.services.events.subscribe();
    let (result, work) = count_work(issues.set_labels(&fixture.alice, &ctx, &[first])).await;
    result.unwrap();
    assert_eq!((work.acquisitions, work.statements), (5, 6));
    issue_actions(&mut events);
    let (result, work) = count_work(issues.set_labels(&fixture.alice, &ctx, &[first])).await;
    result.unwrap();
    assert_eq!((work.acquisitions, work.statements), (4, 4));
    assert!(issue_actions(&mut events).is_empty());
    let (result, work) = count_work(issues.set_labels(&fixture.alice, &ctx, &[second])).await;
    result.unwrap();
    assert_eq!((work.acquisitions, work.statements), (7, 9));
    assert!(
        matches!(issue_actions(&mut events).as_slice(),[IssueAction::Unlabeled{label:old},IssueAction::Labeled{label:new}] if old.id==first && new.id==second)
    );
    let (result, work) = count_work(issues.set_labels(&fixture.alice, &ctx, &[])).await;
    result.unwrap();
    assert_eq!((work.acquisitions, work.statements), (4, 5));
}

#[tokio::test]
async fn triage_archive_unknown_and_limit_errors_keep_existing_query_precedence() {
    let fixture = Fixture::new().await;
    let number = fixture.create(&fixture.bob, "Reader issue", "").await;
    let mut reader = fixture.open(&fixture.bob, number).await;
    reader.permissions.can_triage = true;
    reader.repo.repo.repo.archived_at = Some(Timestamp(1));
    let issues = &fixture.services.issues;
    let (result, work) = count_work(issues.set_labels(&fixture.bob, &reader, &[LabelId(i64::MAX)])).await;
    assert!(matches!(result, Err(ServiceError::Forbidden)));
    assert_eq!(work.statements, 0);
    let mut writer = fixture.open(&fixture.alice, number).await;
    writer.permissions.can_triage = false;
    writer.repo.repo.repo.archived_at = Some(Timestamp(1));
    let (result, work) = count_work(issues.change_labels(&fixture.alice, &writer, &[LabelId(i64::MAX)], &[])).await;
    assert!(matches!(result, Err(ServiceError::Archived)));
    assert_eq!(work.statements, 0);
    writer.repo.repo.repo.archived_at = None;
    let (result, work) = count_work(issues.change_labels(&fixture.alice, &writer, &[LabelId(i64::MAX)], &[])).await;
    assert!(matches!(result,Err(ServiceError::Invalid(message)) if message=="unknown label"));
    assert_eq!(work.statements, 1);
    let (result, work) = count_work(issues.set_labels(&fixture.alice, &writer, &[LabelId(i64::MAX)])).await;
    assert!(matches!(result,Err(ServiceError::Invalid(message)) if message=="unknown label"));
    assert_eq!(work.statements, 2);
    let mut labels = Vec::new();
    for id in 0..26 {
        labels.push(fixture.label(&format!("label-{id}")).await);
    }
    let (result, work) = count_work(issues.set_labels(&fixture.alice, &writer, &labels)).await;
    assert!(matches!(result,Err(ServiceError::Invalid(message)) if message=="an issue can have at most 25 labels"));
    assert_eq!(work.statements, 28);
}

#[tokio::test]
async fn concurrent_identical_adds_publish_once_for_the_sql_unique_winner() {
    let fixture = Fixture::new().await;
    let label = fixture.label("bug").await;
    let number = fixture.create(&fixture.alice, "Issue", "").await;
    let ctx = fixture.open(&fixture.alice, number).await;
    let issues = &fixture.services.issues;
    let ids = [label];
    let mut events = fixture.services.events.subscribe();
    let (first, second) =
        tokio::join!(issues.change_labels(&fixture.alice, &ctx, &ids, &[]), issues.change_labels(&fixture.alice, &ctx, &ids, &[]));
    first.unwrap();
    second.unwrap();
    assert!(matches!(issue_actions(&mut events).as_slice(),[IssueAction::Labeled{label:entry}] if entry.id==label));
    assert_eq!(fixture.database.issue_meta().labels_of(&[ctx.issue.id]).await.unwrap().len(), 1);
}

#[tokio::test]
async fn failed_timeline_after_committed_add_retains_existing_gap_and_retry_is_not_repair() {
    use sqlx::Connection as _;
    let fixture = Fixture::new().await;
    let label = fixture.label("bug").await;
    let number = fixture.create(&fixture.alice, "Issue", "").await;
    let ctx = fixture.open(&fixture.alice, number).await;
    let options = sqlx::sqlite::SqliteConnectOptions::new().filename(fixture._dir.path().join("rbit.db"));
    let mut connection = sqlx::SqliteConnection::connect_with(&options).await.unwrap();
    sqlx::query("CREATE TRIGGER fixture_reject_label_event BEFORE INSERT ON issue_events BEGIN SELECT RAISE(ABORT, 'fixture label event failure'); END").execute(&mut connection).await.unwrap();
    let mut events = fixture.services.events.subscribe();
    let issues = &fixture.services.issues;
    let (result, work) = count_work(issues.change_labels(&fixture.alice, &ctx, &[label], &[])).await;
    assert!(matches!(result, Err(ServiceError::Internal(_))));
    assert_eq!((work.acquisitions, work.statements), (4, 4));
    assert_eq!(fixture.database.issue_meta().labels_of(&[ctx.issue.id]).await.unwrap().len(), 1);
    assert!(fixture.events_of(&fixture.alice, number).await.is_empty());
    assert!(matches!(events.try_recv(), Err(TryRecvError::Empty)));
    sqlx::query("DROP TRIGGER fixture_reject_label_event").execute(&mut connection).await.unwrap();
    let (result, work) = count_work(issues.change_labels(&fixture.alice, &ctx, &[label], &[])).await;
    result.unwrap();
    assert_eq!((work.acquisitions, work.statements), (3, 3));
    assert!(fixture.events_of(&fixture.alice, number).await.is_empty());
    assert!(matches!(events.try_recv(), Err(TryRecvError::Empty)));
    connection.close().await.unwrap();
}
