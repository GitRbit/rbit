//! Human comment production parity and original SQL work counts.

use super::{Fixture, drain};
use rbit_core::{CommentId, IssueKind, Timestamp, issue::CommentAction};
use rbit_services::{DomainEvent, ServiceError};
use rbit_storage::count_work;

#[tokio::test]
async fn comments_follow_the_permission_table_and_publish_events() {
    let fixture = Fixture::new(|_| {}).await;
    let timeline = &fixture.services.timeline;
    let number = fixture.seed(&fixture.alice, IssueKind::Issue, "Crash").await;
    let mut events = fixture.services.events.subscribe();

    let as_bob = fixture.open(&fixture.bob, number).await;
    let as_alice = fixture.open(&fixture.alice, number).await;
    assert!(matches!(timeline.comment(&fixture.bob, &as_bob, " \n ").await, Err(ServiceError::Invalid(_))));
    let comment = timeline.comment(&fixture.bob, &as_bob, "Same here\r\n").await.unwrap();
    assert_eq!(comment.body.as_str(), "Same here\n");

    assert!(matches!(timeline.edit_comment(&fixture.alice, &as_alice, comment.id, "Hacked").await, Err(ServiceError::Forbidden)));
    let edited = timeline.edit_comment(&fixture.bob, &as_bob, comment.id, "Same here, on Linux").await.unwrap();
    assert!(edited.edited_at.is_some());
    let unchanged = timeline.edit_comment(&fixture.bob, &as_bob, comment.id, "Same here, on Linux").await.unwrap();
    assert_eq!(unchanged, edited, "the same text again is not an edit");

    let mine = timeline.comment(&fixture.alice, &as_alice, "Thanks").await.unwrap();
    assert!(matches!(timeline.delete_comment(&fixture.bob, &as_bob, mine.id).await, Err(ServiceError::Forbidden)));
    timeline.delete_comment(&fixture.alice, &as_alice, comment.id).await.unwrap();

    // A comment of another thread is not found through this one.
    let other = fixture.open(&fixture.alice, fixture.seed(&fixture.alice, IssueKind::Issue, "Other").await).await;
    assert!(matches!(timeline.delete_comment(&fixture.alice, &other, mine.id).await, Err(ServiceError::NotFound)));

    let actions: Vec<_> = drain(&mut events)
        .into_iter()
        .filter_map(|event| match event {
            DomainEvent::IssueComment { action, actor, number: n, kind, .. } => {
                assert_eq!((n, kind), (number, IssueKind::Issue));
                Some((actor, action))
            }
            _ => None,
        })
        .collect();
    assert_eq!(actions.len(), 4);
    assert_eq!(actions[0], (Some(fixture.bob.id()), CommentAction::Created));
    assert_eq!(actions[1], (Some(fixture.bob.id()), CommentAction::Edited { previous_body: "Same here\n".to_owned() }));
    assert_eq!(actions[2], (Some(fixture.alice.id()), CommentAction::Created));
    assert!(
        matches!(&actions[3], (actor, CommentAction::Deleted { snapshot }) if *actor == Some(fixture.alice.id()) && snapshot.id == comment.id)
    );

    assert_eq!(fixture.open(&fixture.alice, number).await.issue.comment_count, 1);
}
#[test]
fn legacy_comment_paths_keep_type_identity_id_wire_and_source_values() {
    use rbit_collaboration::comments;
    use std::any::TypeId;
    assert_eq!(TypeId::of::<rbit_core::Comment>(), TypeId::of::<comments::Comment>());
    assert_eq!(TypeId::of::<rbit_core::issue::CommentId>(), TypeId::of::<comments::CommentId>());
    assert_eq!(TypeId::of::<rbit_core::CommentSource>(), TypeId::of::<comments::CommentSource>());
    assert_eq!(serde_json::to_string(&CommentId(-7)).unwrap(), "-7");
    assert_eq!(serde_json::from_str::<CommentId>("-7").unwrap(), CommentId(-7));
    assert_eq!(serde_json::to_string(&comments::CommentSource::Actions).unwrap(), "\"actions\"");
    assert_eq!(comments::CommentSource::parse("user"), Some(comments::CommentSource::default()));
    assert_eq!(comments::CommentSource::parse("USER"), None);
}

#[tokio::test]
async fn actual_create_edit_noop_delete_and_rejected_paths_retain_sql_work() {
    let fixture = Fixture::new(|config| config.security.rate_limit = false).await;
    let number = fixture.seed(&fixture.alice, IssueKind::Issue, "Comments").await;
    let ctx = fixture.open(&fixture.bob, number).await;
    let owner = fixture.open(&fixture.alice, number).await;
    let timeline = &fixture.services.timeline;
    let (result, work) = count_work(timeline.comment(&fixture.bob, &ctx, " \n ")).await;
    assert!(matches!(result, Err(ServiceError::Invalid(_))));
    assert_eq!(work.statements, 0);
    let (result, work) = count_work(timeline.comment(&fixture.bob, &ctx, "a\nb")).await;
    let comment = result.unwrap();
    assert_eq!((work.acquisitions, work.statements), (1, 2));
    let mut events = fixture.services.events.subscribe();
    let (result, work) = count_work(timeline.edit_comment(&fixture.bob, &ctx, comment.id, "a\r\nb")).await;
    assert_eq!(result.unwrap(), comment);
    assert_eq!((work.acquisitions, work.statements), (1, 1));
    assert_eq!(drain(&mut events).len(), 0);
    let (result, work) = count_work(timeline.edit_comment(&fixture.alice, &owner, comment.id, "")).await;
    assert!(matches!(result, Err(ServiceError::Forbidden)));
    assert_eq!(work.statements, 1);
    let mut archived = ctx.clone();
    archived.repo.repo.repo.archived_at = Some(Timestamp(1));
    let (result, work) = count_work(timeline.edit_comment(&fixture.bob, &archived, CommentId(i64::MAX), "")).await;
    assert!(matches!(result, Err(ServiceError::NotFound)));
    assert_eq!(work.statements, 1);
    let (result, work) = count_work(timeline.edit_comment(&fixture.bob, &archived, comment.id, "")).await;
    assert!(matches!(result, Err(ServiceError::Archived)));
    assert_eq!(work.statements, 1);
    // Mutation permission is recomputed before the independent archive guard.
    let (result, work) = count_work(timeline.edit_comment(&fixture.bob, &ctx, comment.id, "changed")).await;
    assert_eq!(result.unwrap().body.as_str(), "changed");
    assert_eq!((work.acquisitions, work.statements), (2, 3));
    let (result, work) = count_work(timeline.delete_comment(&fixture.bob, &ctx, comment.id)).await;
    result.unwrap();
    assert_eq!((work.acquisitions, work.statements), (2, 3));
    let (result, work) = count_work(timeline.delete_comment(&fixture.bob, &ctx, comment.id)).await;
    assert!(matches!(result, Err(ServiceError::NotFound)));
    assert_eq!((work.acquisitions, work.statements), (1, 1));
    assert_eq!(drain(&mut events).len(), 2);
    assert_eq!(fixture.open(&fixture.alice, number).await.issue.comment_count, 0);
}

#[tokio::test]
async fn failing_comment_insert_rolls_back_thread_count_and_never_publishes() {
    use sqlx::Connection as _;
    let fixture = Fixture::new(|config| config.security.rate_limit = false).await;
    let number = fixture.seed(&fixture.alice, IssueKind::Issue, "Comments").await;
    let ctx = fixture.open(&fixture.bob, number).await;
    let options = sqlx::sqlite::SqliteConnectOptions::new().filename(fixture.directory.path().join("rbit.db"));
    let mut connection = sqlx::SqliteConnection::connect_with(&options).await.unwrap();
    sqlx::query("CREATE TRIGGER fixture_reject_human_comment BEFORE INSERT ON issue_comments BEGIN SELECT RAISE(ABORT, 'fixture comment failure'); END").execute(&mut connection).await.unwrap();
    let mut events = fixture.services.events.subscribe();
    let (result, work) = count_work(fixture.services.timeline.comment(&fixture.bob, &ctx, "valid")).await;
    assert!(matches!(result, Err(ServiceError::Internal(_))));
    assert_eq!((work.acquisitions, work.statements), (1, 2));
    assert_eq!(fixture.open(&fixture.alice, number).await.issue.comment_count, 0);
    assert_eq!(drain(&mut events).len(), 0);
    connection.close().await.unwrap();
}
