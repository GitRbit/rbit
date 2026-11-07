//! Real SQL state fast paths, permission ordering and atomic event behavior.

use super::{Fixture, drain, issue_actions};
use rbit_core::{IssueKind, IssueState, StateReason, Timestamp, issue::IssueAction};
use rbit_services::{
    DomainEvent, ServiceError,
    timeline::{StateChange, TimelineEntry},
};
use rbit_storage::count_work;

#[test]
fn existing_state_change_path_is_the_same_feature_type() {
    assert_eq!(std::any::TypeId::of::<StateChange>(), std::any::TypeId::of::<rbit_collaboration::threads::StateChange>());
}

#[tokio::test]
async fn changed_noop_and_missing_states_retain_exact_work_and_original_reason() {
    let fixture = Fixture::new(|_| {}).await;
    let number = fixture.seed(&fixture.bob, IssueKind::Issue, "Original").await;
    let ctx = fixture.open(&fixture.bob, number).await;
    let timeline = &fixture.services.timeline;
    let mut events = fixture.services.events.subscribe();
    let (closed, work) = count_work(timeline.set_state(&fixture.bob, &ctx, StateChange::Close(StateReason::NotPlanned))).await;
    let closed = closed.unwrap();
    assert_eq!((work.acquisitions, work.statements), (1, 3));
    assert_eq!(closed.state_reason, Some(StateReason::NotPlanned));
    assert_eq!(issue_actions(&drain(&mut events)), [IssueAction::Closed { reason: StateReason::NotPlanned }]);
    let (same, work) = count_work(timeline.set_state(&fixture.bob, &ctx, StateChange::Close(StateReason::Completed))).await;
    assert_eq!(same.unwrap(), closed);
    assert_eq!((work.acquisitions, work.statements), (2, 2));
    assert_eq!(drain(&mut events).len(), 0);
    let (reopened, work) = count_work(timeline.set_state(&fixture.bob, &ctx, StateChange::Reopen)).await;
    let reopened = reopened.unwrap();
    assert_eq!((work.acquisitions, work.statements), (1, 3));
    assert_eq!((reopened.closed_at, reopened.closed_by, reopened.state_reason), (None, None, None));
    assert_eq!(issue_actions(&drain(&mut events)), [IssueAction::Reopened]);
    fixture.database.issues().delete(ctx.issue.id).await.unwrap();
    let (missing, work) = count_work(timeline.set_state(&fixture.bob, &ctx, StateChange::Reopen)).await;
    assert!(matches!(missing, Err(ServiceError::NotFound)));
    assert_eq!((work.acquisitions, work.statements), (2, 2));
}

#[tokio::test]
async fn permission_archive_and_merge_reason_checks_preserve_no_sql_paths() {
    let fixture = Fixture::new(|_| {}).await;
    let number = fixture.seed(&fixture.alice, IssueKind::Issue, "Original").await;
    let timeline = &fixture.services.timeline;
    let mut denied = fixture.open(&fixture.bob, number).await;
    denied.permissions.can_set_state = true;
    denied.repo.repo.repo.archived_at = Some(Timestamp(1));
    let (result, work) = count_work(timeline.set_state(&fixture.bob, &denied, StateChange::Close(StateReason::Merged))).await;
    assert!(matches!(result, Err(ServiceError::Forbidden)));
    assert_eq!(work.statements, 0);
    let mut allowed = fixture.open(&fixture.alice, number).await;
    allowed.permissions.can_set_state = false;
    allowed.repo.repo.repo.archived_at = Some(Timestamp(1));
    let (result, work) = count_work(timeline.set_state(&fixture.alice, &allowed, StateChange::Close(StateReason::Merged))).await;
    assert!(matches!(result, Err(ServiceError::Archived)));
    assert_eq!(work.statements, 0);
    allowed.repo.repo.repo.archived_at = None;
    let (result, work) = count_work(timeline.set_state(&fixture.alice, &allowed, StateChange::Close(StateReason::Merged))).await;
    assert!(matches!(result,Err(ServiceError::Invalid(message)) if message=="only merging a pull request closes it as merged"));
    assert_eq!(work.statements, 0);
}

#[tokio::test]
async fn already_open_pull_still_runs_existing_reopen_lookup_before_noop_fallback() {
    let fixture = Fixture::new(|_| {}).await;
    let number = fixture.seed(&fixture.alice, IssueKind::Pull, "Bare pull thread").await;
    let ctx = fixture.open(&fixture.alice, number).await;
    let mut events = fixture.services.events.subscribe();
    let (result, work) = count_work(fixture.services.timeline.set_state(&fixture.alice, &ctx, StateChange::Reopen)).await;
    assert_eq!(result.unwrap(), ctx.issue);
    assert_eq!((work.acquisitions, work.statements), (3, 3));
    assert_eq!(drain(&mut events).len(), 0);
}

#[tokio::test]
async fn event_failure_rolls_back_state_and_close_fields_without_publishing() {
    use sqlx::Connection as _;
    let fixture = Fixture::new(|_| {}).await;
    let number = fixture.seed(&fixture.alice, IssueKind::Issue, "Original").await;
    let ctx = fixture.open(&fixture.alice, number).await;
    let options = sqlx::sqlite::SqliteConnectOptions::new().filename(fixture.directory.path().join("rbit.db"));
    let mut connection = sqlx::SqliteConnection::connect_with(&options).await.unwrap();
    sqlx::query(
        "CREATE TRIGGER fixture_reject_state BEFORE INSERT ON issue_events BEGIN SELECT RAISE(ABORT, 'fixture state failure'); END",
    )
    .execute(&mut connection)
    .await
    .unwrap();
    let mut events = fixture.services.events.subscribe();
    let (result, work) =
        count_work(fixture.services.timeline.set_state(&fixture.alice, &ctx, StateChange::Close(StateReason::Completed))).await;
    assert!(matches!(result, Err(ServiceError::Internal(_))));
    assert_eq!((work.acquisitions, work.statements), (1, 2));
    assert_eq!(fixture.database.issues().find(ctx.issue.id).await.unwrap().unwrap(), ctx.issue);
    assert_eq!(drain(&mut events).len(), 0);
    connection.close().await.unwrap();
}

#[tokio::test]
async fn concurrent_closes_keep_one_sql_winner_and_one_published_closed_event() {
    let fixture = Fixture::new(|_| {}).await;
    let number = fixture.seed(&fixture.bob, IssueKind::Issue, "Original").await;
    let first = fixture.open(&fixture.alice, number).await;
    let second = fixture.open(&fixture.bob, number).await;
    let timeline = &fixture.services.timeline;
    let mut events = fixture.services.events.subscribe();
    let (left, right) = tokio::join!(
        timeline.set_state(&fixture.alice, &first, StateChange::Close(StateReason::Completed)),
        timeline.set_state(&fixture.bob, &second, StateChange::Close(StateReason::NotPlanned))
    );
    let winner = left.unwrap();
    assert_eq!(right.unwrap(), winner);
    assert_eq!(issue_actions(&drain(&mut events)), [IssueAction::Closed { reason: winner.state_reason.unwrap() }]);
}

#[tokio::test]
async fn closing_and_reopening() {
    let fixture = Fixture::new(|_| {}).await;
    let timeline = &fixture.services.timeline;
    let alices = fixture.seed(&fixture.alice, IssueKind::Issue, "Crash").await;
    let bobs = fixture.seed(&fixture.bob, IssueKind::Issue, "Question").await;
    let pull = fixture.seed(&fixture.alice, IssueKind::Pull, "Fix").await;
    let mut events = fixture.services.events.subscribe();

    let not_bobs = fixture.open(&fixture.bob, alices).await;
    assert!(matches!(
        timeline.set_state(&fixture.bob, &not_bobs, StateChange::Close(StateReason::Completed)).await,
        Err(ServiceError::Forbidden)
    ));

    let own = fixture.open(&fixture.bob, bobs).await;
    let closed = timeline.set_state(&fixture.bob, &own, StateChange::Close(StateReason::NotPlanned)).await.unwrap();
    assert_eq!(
        (closed.state, closed.state_reason, closed.closed_by),
        (IssueState::Closed, Some(StateReason::NotPlanned), Some(fixture.bob.id()))
    );
    let again = timeline.set_state(&fixture.bob, &own, StateChange::Close(StateReason::Completed)).await.unwrap();
    assert_eq!(again.state_reason, Some(StateReason::NotPlanned), "already closed: unchanged");
    let reopened = timeline.set_state(&fixture.bob, &own, StateChange::Reopen).await.unwrap();
    assert_eq!(reopened.state, IssueState::Open);

    let as_alice = fixture.open(&fixture.alice, pull).await;
    let merged = timeline.set_state(&fixture.alice, &as_alice, StateChange::Close(StateReason::Merged)).await;
    assert!(matches!(merged, Err(ServiceError::Invalid(_))), "only a merge closes as merged");
    timeline.set_state(&fixture.alice, &as_alice, StateChange::Close(StateReason::Completed)).await.unwrap();
    let closed_pull = fixture.open(&fixture.alice, pull).await;
    timeline.set_state(&fixture.alice, &closed_pull, StateChange::Reopen).await.unwrap();

    let published = drain(&mut events);
    assert_eq!(
        issue_actions(&published),
        [
            IssueAction::Closed { reason: StateReason::NotPlanned },
            IssueAction::Reopened,
            IssueAction::Closed { reason: StateReason::Completed },
            IssueAction::Reopened,
        ]
    );
    assert!(matches!(published[2], DomainEvent::Issue { kind: IssueKind::Pull, .. }));

    let kinds: Vec<_> = timeline
        .timeline(None, &own, None, 50)
        .await
        .unwrap()
        .entries
        .iter()
        .map(|entry| match entry {
            TimelineEntry::Event { event, .. } => event.kind(),
            TimelineEntry::Comment { .. } => "comment",
        })
        .collect();
    assert_eq!(kinds, ["closed", "reopened"]);
}
