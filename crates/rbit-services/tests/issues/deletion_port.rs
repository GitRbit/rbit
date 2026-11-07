//! Actual conditional-delete work, race winner, archive precedence and best-effort audit.
use super::{Fixture, issue_actions};
use rbit_core::{IssueKind, Timestamp, issue::IssueAction};
use rbit_services::ServiceError;
use rbit_storage::count_work;
#[tokio::test]
async fn permission_archive_and_missing_paths_keep_exact_work_without_throttle_or_clock() {
    let fixture = Fixture::new().await;
    let number = fixture.create(&fixture.bob, "spam", "snapshot").await;
    let mut ctx = fixture.open(&fixture.bob, number).await;
    ctx.repo.repo.repo.archived_at = Some(Timestamp(1));
    let (result, work) = count_work(fixture.services.issues.delete(&fixture.bob, &ctx)).await;
    assert!(matches!(result, Err(ServiceError::Forbidden)));
    assert_eq!(work.statements, 0);
    let mut ctx = fixture.open(&fixture.alice, number).await;
    ctx.repo.repo.repo.archived_at = Some(Timestamp(1));
    let (result, work) = count_work(fixture.services.issues.delete(&fixture.alice, &ctx)).await;
    assert!(matches!(result, Err(ServiceError::Archived)));
    assert_eq!(work.statements, 0);
    ctx.issue.kind = IssueKind::Pull;
    let (result, work) = count_work(fixture.services.issues.delete(&fixture.alice, &ctx)).await;
    assert!(matches!(result, Err(ServiceError::Forbidden)));
    assert_eq!(work.statements, 0);
    let ctx = fixture.open(&fixture.alice, number).await;
    let mut events = fixture.services.events.subscribe();
    let (result, work) = count_work(fixture.services.issues.delete(&fixture.alice, &ctx)).await;
    result.unwrap();
    assert_eq!((work.acquisitions, work.statements), (2, 2));
    assert!(matches!(issue_actions(&mut events).as_slice(),[IssueAction::Deleted{snapshot}] if snapshot.body.as_str()=="snapshot"));
    let (result, work) = count_work(fixture.services.issues.delete(&fixture.alice, &ctx)).await;
    assert!(matches!(result, Err(ServiceError::NotFound)));
    assert_eq!((work.acquisitions, work.statements), (1, 1));
    assert_eq!(issue_actions(&mut events).len(), 0);
}
#[tokio::test]
async fn simultaneous_deletes_publish_only_the_sql_winner() {
    let fixture = Fixture::new().await;
    let number = fixture.create(&fixture.alice, "once", "").await;
    let ctx = fixture.open(&fixture.alice, number).await;
    let mut events = fixture.services.events.subscribe();
    let (one, two) =
        tokio::join!(fixture.services.issues.delete(&fixture.alice, &ctx), fixture.services.issues.delete(&fixture.alice, &ctx));
    assert!(matches!((one, two), (Ok(()), Err(ServiceError::NotFound)) | (Err(ServiceError::NotFound), Ok(()))));
    assert_eq!(issue_actions(&mut events).len(), 1);
}
#[tokio::test]
async fn failed_best_effort_audit_does_not_restore_deleted_issue_or_suppress_event() {
    const SENTINEL: &str = "fixture audit failure";
    let fixture = Fixture::with_fault_schema(
        "CREATE TRIGGER fixture_reject_audit BEFORE INSERT ON audit_events \
         BEGIN SELECT RAISE(ABORT,'fixture audit failure'); END",
    )
    .await;
    let number = fixture.create(&fixture.alice, "audit failure", "").await;
    let ctx = fixture.open(&fixture.alice, number).await;
    use sqlx::Connection as _;
    let options = rbit_storage::sqlite::writer_compatibility::register(
        sqlx::sqlite::SqliteConnectOptions::new().filename(fixture.directory.path().join("rbit.db")),
    );
    let mut connection = sqlx::SqliteConnection::connect_with(&options).await.unwrap();
    let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit_events").fetch_one(&mut connection).await.unwrap();
    let failure = sqlx::query("INSERT INTO audit_events(created_at,category,action) VALUES(1,'fixture','probe')")
        .execute(&mut connection)
        .await
        .unwrap_err();
    assert_eq!(failure.as_database_error().expect("injected database failure").message(), SENTINEL);
    let after_probe: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit_events").fetch_one(&mut connection).await.unwrap();
    assert_eq!(after_probe, before, "the proven audit fault inserts no row");
    let mut events = fixture.services.events.subscribe();
    let (result, work) = count_work(fixture.services.issues.delete(&fixture.alice, &ctx)).await;
    result.unwrap();
    assert_eq!((work.acquisitions, work.statements), (2, 2));
    assert!(matches!(issue_actions(&mut events).as_slice(), [IssueAction::Deleted { .. }]));
    assert!(fixture.database.issues().find_by_number(ctx.issue.repo_id, number).await.unwrap().is_none());
    let after_delete: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit_events").fetch_one(&mut connection).await.unwrap();
    assert_eq!(after_delete, before, "best-effort audit remains rejected after successful issue deletion");
    connection.close().await.unwrap();
}
