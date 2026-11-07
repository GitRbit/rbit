//! Real-store editing counts, stale contexts and transactional event rollback.

use super::{Fixture, drain, issue_actions};
use rbit_core::{IssueKind, TimelineEvent, Timestamp, issue::IssueAction};
use rbit_services::{ServiceError, timeline::TimelineEntry};
use rbit_storage::count_work;

#[tokio::test]
async fn edit_changed_and_unchanged_fast_paths_keep_exact_work_and_event_payloads() {
    let fixture = Fixture::new(|_| {}).await;
    let number = fixture.seed(&fixture.alice, IssueKind::Issue, "Original").await;
    let timeline = &fixture.services.timeline;
    let ctx = fixture.open(&fixture.alice, number).await;
    let mut events = fixture.services.events.subscribe();
    let (updated, work) = count_work(timeline.edit(&fixture.alice, &ctx, Some(" Updated "), None)).await;
    let updated = updated.unwrap();
    assert_eq!(updated.title.as_str(), "Updated");
    assert_eq!((work.acquisitions, work.statements), (1, 5));
    assert_eq!(
        issue_actions(&drain(&mut events)),
        [IssueAction::Edited { previous_title: Some("Original".into()), previous_body: None, previous_base: None }]
    );
    let current = fixture.open(&fixture.alice, number).await;
    let (unchanged, work) = count_work(timeline.edit(&fixture.alice, &current, Some("Updated"), None)).await;
    assert_eq!(unchanged.unwrap(), updated);
    assert_eq!((work.acquisitions, work.statements), (1, 2));
    assert_eq!(drain(&mut events).len(), 0);
    let (changed, work) = count_work(timeline.edit(&fixture.alice, &current, None, Some("a\r\nb"))).await;
    assert_eq!(changed.unwrap().body.as_str(), "a\nb");
    assert_eq!((work.acquisitions, work.statements), (1, 3));
    assert_eq!(
        issue_actions(&drain(&mut events)),
        [IssueAction::Edited { previous_title: None, previous_body: Some("Steps to reproduce".into()), previous_base: None }]
    );
}

#[tokio::test]
async fn edit_recomputes_actor_permission_before_archive_and_validation_without_extra_queries() {
    let fixture = Fixture::new(|_| {}).await;
    let number = fixture.seed(&fixture.alice, IssueKind::Issue, "Original").await;
    let timeline = &fixture.services.timeline;
    let mut denied = fixture.open(&fixture.bob, number).await;
    denied.permissions.can_edit = true;
    denied.repo.repo.repo.archived_at = Some(Timestamp(1));
    let (result, work) = count_work(timeline.edit(&fixture.bob, &denied, Some(""), None)).await;
    assert!(matches!(result, Err(ServiceError::Forbidden)));
    assert_eq!(work.statements, 0);
    let mut allowed = fixture.open(&fixture.alice, number).await;
    allowed.permissions.can_edit = false;
    allowed.repo.repo.repo.archived_at = Some(Timestamp(1));
    let (result, work) = count_work(timeline.edit(&fixture.alice, &allowed, Some(""), None)).await;
    assert!(matches!(result, Err(ServiceError::Archived)));
    assert_eq!(work.statements, 0);
    allowed.repo.repo.repo.archived_at = None;
    let (result, work) = count_work(timeline.edit(&fixture.alice, &allowed, Some(""), Some(&"x".repeat(65537)))).await;
    assert!(matches!(result,Err(ServiceError::Invalid(message)) if message=="title must not be empty"));
    assert_eq!(work.statements, 0);
    fixture.database.issues().delete(allowed.issue.id).await.unwrap();
    let (result, work) = count_work(timeline.edit(&fixture.alice, &allowed, None, None)).await;
    assert!(matches!(result, Err(ServiceError::NotFound)));
    assert_eq!((work.acquisitions, work.statements), (1, 1));
}

#[tokio::test]
async fn stale_context_noop_keeps_snapshot_based_previous_values_and_original_sql_noop_path() {
    let fixture = Fixture::new(|_| {}).await;
    let number = fixture.seed(&fixture.alice, IssueKind::Issue, "Original").await;
    let timeline = &fixture.services.timeline;
    let stale = fixture.open(&fixture.alice, number).await;
    timeline.edit(&fixture.alice, &stale, Some("Other edit"), None).await.unwrap();
    let mut events = fixture.services.events.subscribe();
    let (updated, work) = count_work(timeline.edit(&fixture.alice, &stale, None, None)).await;
    assert_eq!(updated.unwrap().title.as_str(), "Other edit");
    assert_eq!((work.acquisitions, work.statements), (1, 2));
    assert_eq!(
        issue_actions(&drain(&mut events)),
        [IssueAction::Edited { previous_title: Some("Original".into()), previous_body: None, previous_base: None }]
    );
}

#[tokio::test]
async fn rename_event_insert_failure_rolls_back_title_atomically_and_publishes_nothing() {
    use sqlx::Connection as _;
    let fixture = Fixture::new(|_| {}).await;
    let number = fixture.seed(&fixture.alice, IssueKind::Issue, "Original").await;
    let ctx = fixture.open(&fixture.alice, number).await;
    let options = sqlx::sqlite::SqliteConnectOptions::new().filename(fixture.directory.path().join("rbit.db"));
    let mut connection = sqlx::SqliteConnection::connect_with(&options).await.unwrap();
    sqlx::query(
        "CREATE TRIGGER fixture_reject_rename BEFORE INSERT ON issue_events BEGIN SELECT RAISE(ABORT, 'fixture rename failure'); END",
    )
    .execute(&mut connection)
    .await
    .unwrap();
    let mut events = fixture.services.events.subscribe();
    let (result, work) = count_work(fixture.services.timeline.edit(&fixture.alice, &ctx, Some("Rejected"), None)).await;
    assert!(matches!(result, Err(ServiceError::Internal(_))));
    assert_eq!((work.acquisitions, work.statements), (1, 3));
    let row = fixture.database.issues().find(ctx.issue.id).await.unwrap().unwrap();
    assert_eq!(row, ctx.issue);
    assert_eq!(drain(&mut events).len(), 0);
    assert_eq!(fixture.services.timeline.timeline(Some(&fixture.alice), &ctx, None, 50).await.unwrap().entries.len(), 0);
    connection.close().await.unwrap();
}

#[tokio::test]
async fn racing_body_edits_preserve_last_writer_wins_and_context_snapshot_events() {
    let fixture = Fixture::new(|_| {}).await;
    let number = fixture.seed(&fixture.bob, IssueKind::Issue, "Original").await;
    let first = fixture.open(&fixture.alice, number).await;
    let second = fixture.open(&fixture.bob, number).await;
    let timeline = &fixture.services.timeline;
    let mut events = fixture.services.events.subscribe();
    let (left, right) =
        tokio::join!(timeline.edit(&fixture.alice, &first, None, Some("left")), timeline.edit(&fixture.bob, &second, None, Some("right")));
    assert_eq!(left.unwrap().body.as_str(), "left");
    assert_eq!(right.unwrap().body.as_str(), "right");
    let row = fixture.database.issues().find(first.issue.id).await.unwrap().unwrap();
    assert!(["left", "right"].contains(&row.body.as_str()));
    assert_eq!(
        issue_actions(&drain(&mut events)),
        vec![IssueAction::Edited { previous_title: None, previous_body: Some("Steps to reproduce".into()), previous_base: None }; 2]
    );
}

#[tokio::test]
async fn editing_title_and_body_records_a_rename() {
    let fixture = Fixture::new(|_| {}).await;
    let timeline = &fixture.services.timeline;
    let number = fixture.seed(&fixture.alice, IssueKind::Issue, "Crash").await;
    let bobs = fixture.seed(&fixture.bob, IssueKind::Issue, "Bob's report").await;
    let mut events = fixture.services.events.subscribe();

    let as_bob = fixture.open(&fixture.bob, number).await;
    assert!(matches!(timeline.edit(&fixture.bob, &as_bob, Some("Mine now"), None).await, Err(ServiceError::Forbidden)));
    let own = fixture.open(&fixture.bob, bobs).await;
    timeline.edit(&fixture.bob, &own, None, Some("More detail")).await.unwrap();

    let as_alice = fixture.open(&fixture.alice, number).await;
    assert!(matches!(timeline.edit(&fixture.alice, &as_alice, Some("  "), None).await, Err(ServiceError::Invalid(_))));
    let updated = timeline.edit(&fixture.alice, &as_alice, Some("Crash on start"), Some("Steps to reproduce")).await.unwrap();
    assert_eq!(updated.title.as_str(), "Crash on start");
    let again = fixture.open(&fixture.alice, number).await;
    timeline.edit(&fixture.alice, &again, Some("Crash on start"), None).await.unwrap();

    assert_eq!(
        issue_actions(&drain(&mut events)),
        [
            IssueAction::Edited { previous_title: None, previous_body: Some("Steps to reproduce".to_owned()), previous_base: None },
            IssueAction::Edited { previous_title: Some("Crash".to_owned()), previous_body: None, previous_base: None },
        ],
        "only what changed is reported, and no change publishes nothing"
    );

    let entries = timeline.timeline(Some(&fixture.bob), &again, None, 50).await.unwrap().entries;
    assert!(matches!(
        &entries[..],
        [TimelineEntry::Event { event: TimelineEvent::Renamed { from, to }, actor: Some(actor), .. }]
            if from == "Crash" && to == "Crash on start" && actor.name.as_str() == "alice"
    ));
}
