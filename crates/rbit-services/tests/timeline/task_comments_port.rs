//! Production task scopes, source checks, SQL work and post-commit event parity.
use super::{Fixture, drain};
use rbit_core::{
    CommentId, CommentSource, IssueKind, RepoId, Timestamp,
    actions::{JobId, PermissionLevel, RunId, TaskId, TokenPermissions},
    issue::CommentAction,
};
use rbit_services::{DomainEvent, ServiceError, actions::tokens::TaskGrant};
use rbit_storage::count_work;
fn grant(repo: RepoId) -> TaskGrant {
    TaskGrant { task: TaskId(1), job: JobId(1), run: RunId(1), repo, permissions: TokenPermissions::uniform(PermissionLevel::Write) }
}
#[tokio::test]
async fn tasks_keep_repository_kind_source_and_archive_error_precedence_without_extra_queries() {
    let fixture = Fixture::new(|config| config.security.rate_limit = false).await;
    let number = fixture.seed(&fixture.alice, IssueKind::Issue, "Tasks").await;
    let ctx = fixture.open(&fixture.alice, number).await;
    let timeline = &fixture.services.timeline;
    let mut task = grant(ctx.issue.repo_id);
    task.repo = RepoId(i64::MAX);
    let (result, work) = count_work(timeline.comment_for_task(&task, &ctx, "")).await;
    assert!(matches!(result, Err(ServiceError::Forbidden)));
    assert_eq!(work.statements, 0);
    task.repo = ctx.issue.repo_id;
    task.permissions = TokenPermissions { pull_requests: PermissionLevel::Write, ..TokenPermissions::uniform(PermissionLevel::Read) };
    let (result, work) = count_work(timeline.comment_for_task(&task, &ctx, "valid")).await;
    assert!(matches!(result, Err(ServiceError::Forbidden)));
    assert_eq!(work.statements, 0);
    let pull = fixture.seed(&fixture.alice, IssueKind::Pull, "Pull").await;
    let pull_ctx = fixture.open(&fixture.alice, pull).await;
    assert_eq!(timeline.comment_for_task(&task, &pull_ctx, "pull-only permission").await.unwrap().source, CommentSource::Actions);
    task = grant(ctx.issue.repo_id);
    let human = timeline.comment(&fixture.alice, &ctx, "human").await.unwrap();
    let mut archived = ctx.clone();
    archived.repo.repo.repo.archived_at = Some(Timestamp(1));
    let (result, work) = count_work(timeline.edit_comment_for_task(&task, &archived, human.id, "")).await;
    assert!(matches!(result, Err(ServiceError::Forbidden)));
    assert_eq!(work.statements, 1);
    let (result, work) = count_work(timeline.edit_comment_for_task(&task, &archived, CommentId(i64::MAX), "")).await;
    assert!(matches!(result, Err(ServiceError::NotFound)));
    assert_eq!(work.statements, 1);
    let actions = timeline.comment_for_task(&task, &ctx, "actions").await.unwrap();
    let (result, work) = count_work(timeline.edit_comment_for_task(&task, &archived, actions.id, "")).await;
    assert!(matches!(result, Err(ServiceError::Archived)));
    assert_eq!(work.statements, 1);
    let other = fixture.seed(&fixture.alice, IssueKind::Issue, "Other").await;
    let other = fixture.open(&fixture.alice, other).await;
    let (result, work) = count_work(timeline.delete_comment_for_task(&task, &other, actions.id)).await;
    assert!(matches!(result, Err(ServiceError::NotFound)));
    assert_eq!(work.statements, 1);
}
#[tokio::test]
async fn task_operations_keep_counts_noop_and_cross_run_actions_authority_and_ghost_events() {
    let fixture = Fixture::new(|config| config.security.rate_limit = false).await;
    let number = fixture.seed(&fixture.alice, IssueKind::Issue, "Tasks").await;
    let mut ctx = fixture.open(&fixture.alice, number).await;
    ctx.issue.locked = true;
    let timeline = &fixture.services.timeline;
    let mut task = grant(ctx.issue.repo_id);
    let mut events = fixture.services.events.subscribe();
    let (result, work) = count_work(timeline.comment_for_task(&task, &ctx, "old\nbody")).await;
    let comment = result.unwrap();
    assert_eq!((work.acquisitions, work.statements), (1, 2));
    assert_eq!((comment.author, comment.source), (None, CommentSource::Actions));
    task.task = TaskId(99);
    task.job = JobId(99);
    task.run = RunId(99);
    // Source provenance is shared Actions identity, not a per-run ownership field.
    let (result, work) = count_work(timeline.edit_comment_for_task(&task, &ctx, comment.id, "old\r\nbody")).await;
    assert_eq!(result.unwrap(), comment);
    assert_eq!((work.acquisitions, work.statements), (1, 1));
    let (result, work) = count_work(timeline.edit_comment_for_task(&task, &ctx, comment.id, "changed")).await;
    assert_eq!(result.unwrap().body.as_str(), "changed");
    assert_eq!((work.acquisitions, work.statements), (2, 3));
    let (result, work) = count_work(timeline.delete_comment_for_task(&task, &ctx, comment.id)).await;
    result.unwrap();
    assert_eq!((work.acquisitions, work.statements), (2, 3));
    let actions: Vec<_> = drain(&mut events)
        .into_iter()
        .filter_map(|event| match event {
            DomainEvent::IssueComment { actor, action, .. } => {
                assert_eq!(actor, None);
                Some(action)
            }
            _ => None,
        })
        .collect();
    assert!(
        matches!(actions.as_slice(),[CommentAction::Created,CommentAction::Edited {previous_body},CommentAction::Deleted {snapshot}] if previous_body=="old\nbody" && snapshot.body.as_str()=="changed")
    );
    assert_eq!(fixture.open(&fixture.alice, number).await.issue.comment_count, 0);
}
