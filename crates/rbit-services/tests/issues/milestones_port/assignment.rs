//! Production milestone assignment query counts and existing partial-effect behavior.

use super::super::issue_actions;
use super::Fixture;
use rbit_core::{MilestoneId, Timestamp, issue::IssueAction};
use rbit_services::{ServiceError, issues::MilestoneInput};
use rbit_storage::count_work;
use tokio::sync::broadcast::error::TryRecvError;

#[test]
fn storage_attachment_record_path_is_the_same_feature_type() {
    assert_eq!(
        std::any::TypeId::of::<rbit_storage::sqlite::issue_meta::IssueMilestone>(),
        std::any::TypeId::of::<rbit_collaboration::milestones::IssueMilestone>()
    );
}
#[tokio::test]
async fn assign_same_replace_clear_and_empty_keep_original_work_and_event_order() {
    let fixture = Fixture::new().await;
    let repo = fixture.repo(&fixture.alice).await;
    let issues = &fixture.services.issues;
    let first = issues
        .create_milestone(&fixture.alice, &repo, &MilestoneInput { title: "first".into(), ..MilestoneInput::default() })
        .await
        .unwrap();
    let second = issues
        .create_milestone(&fixture.alice, &repo, &MilestoneInput { title: "second".into(), ..MilestoneInput::default() })
        .await
        .unwrap();
    let number = fixture.create(&fixture.alice, "Issue", "").await;
    let ctx = fixture.open(&fixture.alice, number).await;
    let mut events = fixture.services.events.subscribe();
    let (result, work) = count_work(issues.set_milestone(&fixture.alice, &ctx, Some(first.id))).await;
    result.unwrap();
    assert_eq!((work.acquisitions, work.statements), (4, 6));
    assert_eq!(issue_actions(&mut events), [IssueAction::Milestoned { milestone: first.id }]);
    let (result, work) = count_work(issues.set_milestone(&fixture.alice, &ctx, Some(first.id))).await;
    result.unwrap();
    assert_eq!((work.acquisitions, work.statements), (2, 2));
    assert_eq!(issue_actions(&mut events), Vec::<IssueAction>::new());
    let (result, work) = count_work(issues.set_milestone(&fixture.alice, &ctx, Some(second.id))).await;
    result.unwrap();
    assert_eq!((work.acquisitions, work.statements), (5, 8));
    assert_eq!(
        issue_actions(&mut events),
        [IssueAction::Demilestoned { milestone: first.id, title: "first".into() }, IssueAction::Milestoned { milestone: second.id }]
    );
    let (result, work) = count_work(issues.set_milestone(&fixture.alice, &ctx, None)).await;
    result.unwrap();
    assert_eq!((work.acquisitions, work.statements), (3, 5));
    assert_eq!(issue_actions(&mut events), [IssueAction::Demilestoned { milestone: second.id, title: "second".into() }]);
    let (result, work) = count_work(issues.set_milestone(&fixture.alice, &ctx, None)).await;
    result.unwrap();
    assert_eq!((work.acquisitions, work.statements), (1, 1));
    assert_eq!(issue_actions(&mut events), Vec::<IssueAction>::new());
}

#[tokio::test]
async fn permission_archive_and_unknown_new_milestone_win_before_current_lookup() {
    let fixture = Fixture::new().await;
    let number = fixture.create(&fixture.bob, "Issue", "").await;
    let issues = &fixture.services.issues;
    let mut reader = fixture.open(&fixture.bob, number).await;
    reader.permissions.can_triage = true;
    reader.repo.repo.repo.archived_at = Some(Timestamp(1));
    let (result, work) = count_work(issues.set_milestone(&fixture.bob, &reader, Some(MilestoneId(i64::MAX)))).await;
    assert!(matches!(result, Err(ServiceError::Forbidden)));
    assert_eq!(work.statements, 0);
    let mut writer = fixture.open(&fixture.alice, number).await;
    writer.permissions.can_triage = false;
    writer.repo.repo.repo.archived_at = Some(Timestamp(1));
    let (result, work) = count_work(issues.set_milestone(&fixture.alice, &writer, Some(MilestoneId(i64::MAX)))).await;
    assert!(matches!(result, Err(ServiceError::Archived)));
    assert_eq!(work.statements, 0);
    writer.repo.repo.repo.archived_at = None;
    let (result, work) = count_work(issues.set_milestone(&fixture.alice, &writer, Some(MilestoneId(i64::MAX)))).await;
    assert!(matches!(result,Err(ServiceError::Invalid(message)) if message=="unknown milestone"));
    assert_eq!((work.acquisitions, work.statements), (1, 1));
}

#[tokio::test]
async fn timeline_failure_keeps_assignment_committed_and_same_id_retry_does_not_repair() {
    use sqlx::Connection as _;
    let fixture = Fixture::new().await;
    let repo = fixture.repo(&fixture.alice).await;
    let issues = &fixture.services.issues;
    let milestone = issues
        .create_milestone(&fixture.alice, &repo, &MilestoneInput { title: "target".into(), ..MilestoneInput::default() })
        .await
        .unwrap();
    let number = fixture.create(&fixture.alice, "Issue", "").await;
    let ctx = fixture.open(&fixture.alice, number).await;
    let options = sqlx::sqlite::SqliteConnectOptions::new().filename(fixture.directory.path().join("rbit.db"));
    let mut connection = sqlx::SqliteConnection::connect_with(&options).await.unwrap();
    sqlx::query("CREATE TRIGGER fixture_reject_milestone_history BEFORE INSERT ON issue_events BEGIN SELECT RAISE(ABORT, 'fixture milestone history failure'); END").execute(&mut connection).await.unwrap();
    let mut events = fixture.services.events.subscribe();
    let (result, work) = count_work(issues.set_milestone(&fixture.alice, &ctx, Some(milestone.id))).await;
    assert!(matches!(result, Err(ServiceError::Internal(_))));
    assert_eq!((work.acquisitions, work.statements), (4, 5));
    assert_eq!(fixture.database.issue_meta().milestones_of(&[ctx.issue.id]).await.unwrap()[0].id, milestone.id);
    assert_eq!(fixture.events_of(&fixture.alice, number).await, Vec::<rbit_core::TimelineEvent>::new());
    assert!(matches!(events.try_recv(), Err(TryRecvError::Empty)));
    sqlx::query("DROP TRIGGER fixture_reject_milestone_history").execute(&mut connection).await.unwrap();
    let (result, work) = count_work(issues.set_milestone(&fixture.alice, &ctx, Some(milestone.id))).await;
    result.unwrap();
    assert_eq!((work.acquisitions, work.statements), (2, 2));
    assert_eq!(fixture.events_of(&fixture.alice, number).await, Vec::<rbit_core::TimelineEvent>::new());
    assert!(matches!(events.try_recv(), Err(TryRecvError::Empty)));
    connection.close().await.unwrap();
}

#[tokio::test]
async fn foreign_catalog_milestone_is_rejected_before_reading_current_assignment() {
    let fixture = Fixture::new().await;
    fixture
        .services
        .repositories
        .create(
            &fixture.alice,
            rbit_services::CreateRepository {
                name: "other-milestones".into(),
                description: String::new(),
                visibility: rbit_core::Visibility::Public,
            },
        )
        .await
        .unwrap();
    let other = fixture.services.repositories.open(Some(&fixture.alice), "alice", "other-milestones").await.unwrap();
    let milestone = fixture
        .services
        .issues
        .create_milestone(&fixture.alice, &other, &MilestoneInput { title: "foreign".into(), ..MilestoneInput::default() })
        .await
        .unwrap();
    let number = fixture.create(&fixture.alice, "Issue", "").await;
    let ctx = fixture.open(&fixture.alice, number).await;
    let (result, work) = count_work(fixture.services.issues.set_milestone(&fixture.alice, &ctx, Some(milestone.id))).await;
    assert!(matches!(result, Err(ServiceError::Invalid(message)) if message == "unknown milestone"));
    assert_eq!((work.acquisitions, work.statements), (1, 1));
    assert_eq!(
        fixture.database.issue_meta().milestones_of(&[ctx.issue.id]).await.unwrap(),
        Vec::<rbit_collaboration::milestones::IssueMilestone>::new()
    );
}

#[tokio::test]
async fn concurrent_same_assignment_keeps_unique_row_without_inventing_event_idempotency() {
    let fixture = Fixture::new().await;
    let repo = fixture.repo(&fixture.alice).await;
    let issues = &fixture.services.issues;
    let milestone = issues
        .create_milestone(&fixture.alice, &repo, &MilestoneInput { title: "same".into(), ..MilestoneInput::default() })
        .await
        .unwrap();
    let number = fixture.create(&fixture.alice, "Issue", "").await;
    let ctx = fixture.open(&fixture.alice, number).await;
    let mut events = fixture.services.events.subscribe();
    let (first, second) = tokio::join!(
        issues.set_milestone(&fixture.alice, &ctx, Some(milestone.id)),
        issues.set_milestone(&fixture.alice, &ctx, Some(milestone.id))
    );
    first.unwrap();
    second.unwrap();
    let rows = fixture.database.issue_meta().milestones_of(&[ctx.issue.id]).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, milestone.id);
    let actions = issue_actions(&mut events);
    assert!((1..=2).contains(&actions.len()), "pre-read equality check is not atomic event idempotency");
    assert!(actions.iter().all(|action| *action == IssueAction::Milestoned { milestone: milestone.id }));
}
