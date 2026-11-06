//! Real-store milestone catalog and retained attachment/timeline parity.

use super::Fixture;
use rbit_core::{IssueState, MilestoneId, TimelineEvent, Timestamp, Visibility};
use rbit_services::{CreateRepository, ServiceError, issues::MilestoneInput};
use rbit_storage::{count_work, sqlite::admin::ListFilter};
use tokio::sync::broadcast::error::TryRecvError;

#[test]
fn legacy_milestone_and_shared_state_paths_remain_exact_type_aliases() {
    use rbit_collaboration::milestones;
    use std::any::TypeId;
    assert_eq!(TypeId::of::<IssueState>(), TypeId::of::<rbit_collaboration::IssueState>());
    assert_eq!(TypeId::of::<MilestoneId>(), TypeId::of::<milestones::MilestoneId>());
    assert_eq!(TypeId::of::<MilestoneInput>(), TypeId::of::<milestones::MilestoneInput>());
    assert_eq!(TypeId::of::<rbit_services::issues::MilestoneRef>(), TypeId::of::<milestones::MilestoneRef>());
    assert_eq!(TypeId::of::<rbit_services::issues::Milestone>(), TypeId::of::<milestones::Milestone>());
    assert_eq!(TypeId::of::<rbit_storage::sqlite::issue_meta::MilestoneFields>(), TypeId::of::<milestones::MilestoneFields>());
    assert_eq!(TypeId::of::<rbit_storage::sqlite::issue_meta::MilestoneProgress>(), TypeId::of::<milestones::MilestoneProgress>());
}

#[tokio::test]
async fn all_catalog_operations_keep_single_statement_counts_without_new_events_or_audits() {
    let fixture = Fixture::new().await;
    let repo = fixture.repo(&fixture.alice).await;
    let issues = &fixture.services.issues;
    let mut events = fixture.services.events.subscribe();
    let filter = ListFilter { search: "", limit: 100, offset: 0 };
    let before = fixture.database.audit().list(None, filter).await.unwrap();
    let input = MilestoneInput { title: " v1 ".into(), description: "a\r\nb".into(), due_on: " 2028-02-29 ".into() };
    let (created, work) = count_work(issues.create_milestone(&fixture.alice, &repo, &input)).await;
    let created = created.unwrap();
    assert_eq!((work.acquisitions, work.statements), (1, 1));
    assert_eq!((created.title.as_str(), created.description.as_str(), created.due_on.as_deref()), ("v1", "a\nb", Some("2028-02-29")));
    assert_eq!(created.state, IssueState::Open);
    let (duplicate, work) = count_work(issues.create_milestone(&fixture.alice, &repo, &input)).await;
    assert!(matches!(duplicate, Err(ServiceError::Conflict("milestone title"))));
    assert_eq!(work.statements, 1);
    let (updated, work) = count_work(issues.update_milestone(&fixture.alice, &repo, created.id, &input)).await;
    assert_eq!(updated.unwrap().id, created.id);
    assert_eq!(work.statements, 1);
    let (listed, work) = count_work(issues.milestones(&repo, Some(IssueState::Open))).await;
    assert_eq!(listed.unwrap().len(), 1);
    assert_eq!(work.statements, 1);
    let (counts, work) = count_work(issues.milestone_counts(&repo)).await;
    assert_eq!(counts.unwrap(), (1, 0));
    assert_eq!(work.statements, 1);
    let (found, work) = count_work(issues.milestone(&repo, created.id)).await;
    let progress = found.unwrap();
    assert_eq!(progress.milestone.id, created.id);
    assert_eq!((progress.open_issues, progress.closed_issues), (0, 0));
    assert_eq!(work.statements, 1);
    let (titled, work) = count_work(issues.milestone_titled(&repo, "V1")).await;
    assert_eq!(titled.unwrap().id, created.id);
    assert_eq!(work.statements, 1);
    let (closed, work) = count_work(issues.set_milestone_state(&repo, created.id, IssueState::Closed)).await;
    assert_eq!(closed.unwrap().state, IssueState::Closed);
    assert_eq!(work.statements, 1);
    let (deleted, work) = count_work(issues.delete_milestone(&repo, created.id)).await;
    assert_eq!(deleted.unwrap().id, created.id);
    assert_eq!(work.statements, 1);
    assert!(matches!(events.try_recv(), Err(TryRecvError::Empty)));
    let after = fixture.database.audit().list(None, filter).await.unwrap();
    assert_eq!(before.items, after.items);
    assert_eq!(before.total, after.total);
}

#[tokio::test]
async fn denied_archived_invalid_missing_and_cross_repository_writes_keep_error_precedence() {
    let fixture = Fixture::new().await;
    let mut repo = fixture.repo(&fixture.bob).await;
    repo.repo.repo.archived_at = Some(Timestamp(1));
    let issues = &fixture.services.issues;
    let (result, work) = count_work(issues.create_milestone(&fixture.bob, &repo, &MilestoneInput::default())).await;
    assert!(matches!(result, Err(ServiceError::Forbidden)));
    assert_eq!(work.statements, 0);
    repo = fixture.repo(&fixture.alice).await;
    repo.repo.repo.archived_at = Some(Timestamp(1));
    let (result, work) = count_work(issues.create_milestone(&fixture.alice, &repo, &MilestoneInput::default())).await;
    assert!(matches!(result, Err(ServiceError::Archived)));
    assert_eq!(work.statements, 0);
    repo.repo.repo.archived_at = None;
    let (result, work) = count_work(issues.create_milestone(&fixture.alice, &repo, &MilestoneInput::default())).await;
    assert!(matches!(result, Err(ServiceError::Invalid(_))));
    assert_eq!(work.statements, 0);
    let input = MilestoneInput { title: "v1".into(), ..MilestoneInput::default() };
    let id = issues.create_milestone(&fixture.alice, &repo, &input).await.unwrap().id;
    fixture
        .services
        .repositories
        .create(
            &fixture.alice,
            CreateRepository { name: "other-milestones".into(), description: String::new(), visibility: Visibility::Public },
        )
        .await
        .unwrap();
    let other = fixture.services.repositories.open(Some(&fixture.alice), "alice", "other-milestones").await.unwrap();
    let (result, work) = count_work(issues.update_milestone(&fixture.alice, &other, id, &input)).await;
    assert!(matches!(result, Err(ServiceError::NotFound)));
    assert_eq!(work.statements, 1);
    let (result, work) = count_work(issues.set_milestone_state(&other, id, IssueState::Closed)).await;
    assert!(matches!(result, Err(ServiceError::NotFound)));
    assert_eq!(work.statements, 1);
    let (result, work) = count_work(issues.delete_milestone(&other, id)).await;
    assert!(matches!(result, Err(ServiceError::NotFound)));
    assert_eq!(work.statements, 1);
    assert_eq!(issues.milestone(&repo, id).await.unwrap().milestone.id, id);
}

#[tokio::test]
async fn repeated_close_preserves_first_timestamp_reopen_clears_it_and_sql_order_stays_stable() {
    let fixture = Fixture::new().await;
    let repo = fixture.repo(&fixture.alice).await;
    let issues = &fixture.services.issues;
    let later = issues
        .create_milestone(
            &fixture.alice,
            &repo,
            &MilestoneInput { title: "later".into(), due_on: "2028-01-01".into(), ..MilestoneInput::default() },
        )
        .await
        .unwrap();
    let early = issues
        .create_milestone(
            &fixture.alice,
            &repo,
            &MilestoneInput { title: "early".into(), due_on: "2027-01-01".into(), ..MilestoneInput::default() },
        )
        .await
        .unwrap();
    let undated = issues
        .create_milestone(&fixture.alice, &repo, &MilestoneInput { title: "undated".into(), ..MilestoneInput::default() })
        .await
        .unwrap();
    let ids: Vec<_> = issues.milestones(&repo, None).await.unwrap().into_iter().map(|p| p.milestone.id).collect();
    assert_eq!(ids, [early.id, later.id, undated.id]);
    fixture.database.issue_meta().set_milestone_state(repo.repo.repo.id, early.id, IssueState::Closed, Timestamp(7)).await.unwrap();
    let closed = issues.set_milestone_state(&repo, early.id, IssueState::Closed).await.unwrap();
    assert_eq!(closed.closed_at, Some(Timestamp(7)));
    assert!(closed.updated_at >= early.updated_at);
    let reopened = issues.set_milestone_state(&repo, early.id, IssueState::Open).await.unwrap();
    assert_eq!(reopened.closed_at, None);
    assert!(issues.milestone_titled(&repo, " EARLY ").await.is_err(), "lookup has no new trimming");
}

#[tokio::test]
async fn milestones_one_per_issue_with_events() {
    let fixture = Fixture::new().await;
    let repo = fixture.repo(&fixture.alice).await;
    let issues = &fixture.services.issues;
    let make = |title: &str| MilestoneInput { title: title.into(), description: "Plan".into(), due_on: "2026-12-31".into() };
    let v1 = issues.create_milestone(&fixture.alice, &repo, &make("v1")).await.unwrap();
    let v2 = issues.create_milestone(&fixture.alice, &repo, &make("v2")).await.unwrap();
    assert!(matches!(issues.create_milestone(&fixture.alice, &repo, &make("V1")).await, Err(ServiceError::Conflict(_))));
    assert!(matches!(
        issues
            .create_milestone(
                &fixture.alice,
                &repo,
                &MilestoneInput { title: "v3".into(), due_on: "2026-02-30".into(), ..MilestoneInput::default() }
            )
            .await,
        Err(ServiceError::Invalid(_))
    ));
    let number = fixture.create(&fixture.alice, "Crash", "").await;
    let ctx = fixture.open(&fixture.alice, number).await;
    issues.set_milestone(&fixture.alice, &ctx, Some(v1.id)).await.unwrap();
    issues.set_milestone(&fixture.alice, &ctx, Some(v1.id)).await.unwrap();
    issues.set_milestone(&fixture.alice, &ctx, Some(v2.id)).await.unwrap();
    issues.set_milestone(&fixture.alice, &ctx, None).await.unwrap();
    let events = fixture.events_of(&fixture.alice, number).await;
    let kinds: Vec<&str> = events.iter().map(TimelineEvent::kind).collect();
    assert_eq!(kinds, ["milestoned", "demilestoned", "milestoned", "demilestoned"]);

    issues.set_milestone(&fixture.alice, &ctx, Some(v2.id)).await.unwrap();
    let progress = issues.milestone(&repo, v2.id).await.unwrap();
    assert_eq!((progress.open_issues, progress.closed_issues), (1, 0));
    issues.set_milestone_state(&repo, v2.id, IssueState::Closed).await.unwrap();
    assert_eq!(issues.milestone_counts(&repo).await.unwrap(), (1, 1));
    issues.delete_milestone(&repo, v2.id).await.unwrap();
    assert!(issues.metadata(&[ctx.issue.id]).await.unwrap()[&ctx.issue.id].milestone.is_none());
    assert!(matches!(issues.milestone(&repo, v2.id).await, Err(ServiceError::NotFound)));
}
