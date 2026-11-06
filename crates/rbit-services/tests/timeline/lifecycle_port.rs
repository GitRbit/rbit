//! Real lock conditional winners, no-op costs and error precedence.
use super::{Fixture, drain, issue_actions};
use rbit_core::{IssueKind, Timestamp, issue::IssueAction};
use rbit_services::ServiceError;
use rbit_storage::count_work;
#[tokio::test]
async fn lock_changes_noops_and_concurrent_winners_keep_original_sql_and_events() {
    let fixture = Fixture::new(|_| {}).await;
    let number = fixture.seed(&fixture.alice, IssueKind::Issue, "Lock").await;
    let ctx = fixture.open(&fixture.alice, number).await;
    let timeline = &fixture.services.timeline;
    let mut events = fixture.services.events.subscribe();
    let (result, work) = count_work(timeline.set_locked(&fixture.alice, &ctx, true)).await;
    result.unwrap();
    assert_eq!((work.acquisitions, work.statements), (1, 3));
    let (result, work) = count_work(timeline.set_locked(&fixture.alice, &ctx, true)).await;
    result.unwrap();
    assert_eq!((work.acquisitions, work.statements), (1, 1));
    assert_eq!(issue_actions(&drain(&mut events)), [IssueAction::Locked]);
    let (first, second) = tokio::join!(timeline.set_locked(&fixture.alice, &ctx, false), timeline.set_locked(&fixture.alice, &ctx, false));
    first.unwrap();
    second.unwrap();
    assert_eq!(issue_actions(&drain(&mut events)), [IssueAction::Unlocked]);
    fixture.database.issues().delete(ctx.issue.id).await.unwrap();
    let (result, work) = count_work(timeline.set_locked(&fixture.alice, &ctx, true)).await;
    result.unwrap();
    assert_eq!((work.acquisitions, work.statements), (1, 1));
    assert_eq!(drain(&mut events).len(), 0);
}
#[tokio::test]
async fn denied_archive_checks_keep_zero_work_and_recompute_actor_permissions() {
    let fixture = Fixture::new(|_| {}).await;
    let number = fixture.seed(&fixture.bob, IssueKind::Issue, "Lock").await;
    let mut reader = fixture.open(&fixture.bob, number).await;
    reader.permissions.can_lock = true;
    reader.repo.repo.repo.archived_at = Some(Timestamp(1));
    let timeline = &fixture.services.timeline;
    let (result, work) = count_work(timeline.set_locked(&fixture.bob, &reader, true)).await;
    assert!(matches!(result, Err(ServiceError::Forbidden)));
    assert_eq!(work.statements, 0);
    let mut writer = fixture.open(&fixture.alice, number).await;
    writer.permissions.can_lock = false;
    writer.repo.repo.repo.archived_at = Some(Timestamp(1));
    let (result, work) = count_work(timeline.set_locked(&fixture.alice, &writer, true)).await;
    assert!(matches!(result, Err(ServiceError::Archived)));
    assert_eq!(work.statements, 0);
}
