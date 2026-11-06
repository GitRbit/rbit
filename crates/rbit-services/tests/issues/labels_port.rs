//! Complete label catalog activation against real `SQLite` and existing service contexts.

use super::{Fixture, issue_actions};
use rbit_core::{Visibility, issue::IssueAction};
use rbit_services::{CreateRepository, ServiceError, issues::LabelInput};
use rbit_storage::count_work;
use tokio::sync::broadcast::error::TryRecvError;

#[test]
fn legacy_label_paths_are_the_exact_feature_owned_types() {
    use rbit_collaboration::labels;
    use std::any::TypeId;

    assert_eq!(TypeId::of::<rbit_core::LabelId>(), TypeId::of::<labels::LabelId>());
    assert_eq!(TypeId::of::<rbit_core::issue::LabelSnapshot>(), TypeId::of::<labels::LabelSnapshot>());
    assert_eq!(TypeId::of::<LabelInput>(), TypeId::of::<labels::LabelInput>());
    assert_eq!(TypeId::of::<rbit_services::issues::Label>(), TypeId::of::<labels::Label>());
    assert_eq!(TypeId::of::<rbit_storage::sqlite::issue_meta::LabelFields>(), TypeId::of::<labels::LabelFields>());
    assert_eq!(TypeId::of::<rbit_storage::sqlite::issue_meta::LabelUsage>(), TypeId::of::<labels::LabelUsage>());
}

#[tokio::test]
async fn catalog_mutations_and_reads_keep_statement_counts_and_publish_no_catalog_events() {
    let fixture = Fixture::new().await;
    let repo = fixture.repo(&fixture.alice).await;
    let issues = &fixture.services.issues;
    let mut events = fixture.services.events.subscribe();
    let filter = rbit_storage::sqlite::admin::ListFilter { search: "", limit: 100, offset: 0 };
    let audit_before = fixture.database.audit().list(None, filter).await.unwrap();
    let input = LabelInput { name: " bug ".into(), color: " AABBCC ".into(), description: " note ".into() };
    let (created, work) = count_work(issues.create_label(&fixture.alice, &repo, &input)).await;
    let created = created.unwrap();
    assert_eq!((work.acquisitions, work.statements), (2, 2));
    assert_eq!((created.name.as_str(), created.color.as_str(), created.description.as_str()), ("bug", "#aabbcc", "note"));
    let (duplicate, work) = count_work(issues.create_label(&fixture.alice, &repo, &input)).await;
    assert!(matches!(duplicate, Err(ServiceError::Conflict("label name"))));
    assert_eq!(work.statements, 2);
    let (updated, work) = count_work(issues.update_label(&fixture.alice, &repo, created.id, &input)).await;
    assert_eq!(updated.unwrap().id, created.id);
    assert_eq!(work.statements, 1);
    let (listed, work) = count_work(issues.labels(&repo)).await;
    assert_eq!(listed.unwrap().len(), 1);
    assert_eq!(work.statements, 1);
    let (usage, work) = count_work(issues.label_usage(&repo)).await;
    assert_eq!(usage.unwrap()[0].open_issues, 0);
    assert_eq!(work.statements, 1);
    let (named, work) = count_work(issues.label_named(&repo, "BUG")).await;
    assert_eq!(named.unwrap().id, created.id);
    assert_eq!(work.statements, 1);
    let (named, work) = count_work(issues.labels_named(&repo, &["BUG".into(), "bug".into()])).await;
    assert_eq!(named.unwrap().len(), 1);
    assert_eq!(work.statements, 1);
    let (empty, work) = count_work(issues.labels_named(&repo, &[])).await;
    assert_eq!(empty.unwrap(), Vec::<rbit_collaboration::labels::Label>::new());
    assert_eq!(work.statements, 0);
    let (deleted, work) = count_work(issues.delete_label(&repo, created.id)).await;
    assert_eq!(deleted.unwrap().id, created.id);
    assert_eq!(work.statements, 1);
    assert!(matches!(events.try_recv(), Err(TryRecvError::Empty)));
    let after = fixture.database.audit().list(None, filter).await.unwrap();
    assert_eq!(after.items, audit_before.items);
    assert_eq!(after.total, audit_before.total);
}

#[tokio::test]
async fn foreign_repository_labels_cannot_be_updated_or_deleted_through_another_context() {
    let fixture = Fixture::new().await;
    let id = fixture.label("private-to-catalog").await;
    fixture
        .services
        .repositories
        .create(
            &fixture.alice,
            CreateRepository { name: "other-catalog".into(), description: String::new(), visibility: Visibility::Public },
        )
        .await
        .unwrap();
    let other = fixture.services.repositories.open(Some(&fixture.alice), "alice", "other-catalog").await.unwrap();
    let input = LabelInput { name: "rename".into(), color: "#000000".into(), description: String::new() };
    let issues = &fixture.services.issues;
    let (updated, work) = count_work(issues.update_label(&fixture.alice, &other, id, &input)).await;
    assert!(matches!(updated, Err(ServiceError::NotFound)));
    assert_eq!(work.statements, 1);
    let (deleted, work) = count_work(issues.delete_label(&other, id)).await;
    assert!(matches!(deleted, Err(ServiceError::NotFound)));
    assert_eq!(work.statements, 1);
    assert_eq!(issues.labels(&fixture.repo(&fixture.alice).await).await.unwrap()[0].id, id);
}

#[tokio::test]
async fn racing_default_seeds_report_actual_insertions_and_keep_one_sql_unique_catalog() {
    let fixture = Fixture::new().await;
    let repo = fixture.repo(&fixture.alice).await;
    let issues = &fixture.services.issues;
    let (first, second) =
        tokio::join!(issues.create_default_labels(&fixture.alice, &repo), issues.create_default_labels(&fixture.alice, &repo),);
    assert_eq!(first.unwrap() + second.unwrap(), 9);
    assert_eq!(issues.labels(&repo).await.unwrap().len(), 9);
}

#[tokio::test]
async fn denied_archived_missing_and_default_paths_preserve_visibility_and_work() {
    let fixture = Fixture::new().await;
    let mut reader = fixture.repo(&fixture.bob).await;
    // The writer check must win even when archive status would also reject the write.
    reader.repo.repo.archived_at = Some(rbit_core::Timestamp(9));
    let (result, work) = count_work(fixture.services.issues.create_label(&fixture.bob, &reader, &LabelInput::default())).await;
    assert!(matches!(result, Err(ServiceError::Forbidden)));
    assert_eq!(work.statements, 0);
    let mut writer = fixture.repo(&fixture.alice).await;
    writer.repo.repo.archived_at = Some(rbit_core::Timestamp(9));
    let (result, work) = count_work(fixture.services.issues.create_label(&fixture.alice, &writer, &LabelInput::default())).await;
    assert!(matches!(result, Err(ServiceError::Archived)));
    assert_eq!(work.statements, 0);
    writer.repo.repo.archived_at = None;
    let issues = &fixture.services.issues;
    let (created, work) = count_work(issues.create_default_labels(&fixture.alice, &writer)).await;
    assert_eq!(created.unwrap(), 9);
    assert_eq!((work.acquisitions, work.statements), (2, 10));
    let (again, work) = count_work(issues.create_default_labels(&fixture.alice, &writer)).await;
    assert_eq!(again.unwrap(), 0);
    assert_eq!(work.statements, 1);
    let input = LabelInput { name: "valid".into(), color: "#000000".into(), description: String::new() };
    let (missing, work) = count_work(issues.update_label(&fixture.alice, &writer, rbit_core::LabelId(i64::MAX), &input)).await;
    assert!(matches!(missing, Err(ServiceError::NotFound)));
    assert_eq!(work.statements, 1);
    let (missing, work) = count_work(issues.delete_label(&writer, rbit_core::LabelId(i64::MAX))).await;
    assert!(matches!(missing, Err(ServiceError::NotFound)));
    assert_eq!(work.statements, 1);
}

#[tokio::test]
async fn labels_have_limits_uniqueness_and_events() {
    let fixture = Fixture::new().await;
    let repo = fixture.repo(&fixture.alice).await;
    let issues = &fixture.services.issues;
    let bug = fixture.label("bug").await;
    let input = LabelInput { name: "BUG".into(), color: "#000000".into(), description: String::new() };
    assert!(matches!(issues.create_label(&fixture.alice, &repo, &input).await, Err(ServiceError::Conflict(_))));
    let input = LabelInput { name: "x".into(), color: "nope".into(), description: String::new() };
    assert!(matches!(issues.create_label(&fixture.alice, &repo, &input).await, Err(ServiceError::Invalid(_))));
    let reader = fixture.repo(&fixture.bob).await;
    let input = LabelInput { name: "mine".into(), color: "#000000".into(), description: String::new() };
    assert!(matches!(issues.create_label(&fixture.bob, &reader, &input).await, Err(ServiceError::Forbidden)));

    let number = fixture.create(&fixture.bob, "Crash", "").await;
    let as_bob = fixture.open(&fixture.bob, number).await;
    assert!(
        matches!(issues.change_labels(&fixture.bob, &as_bob, &[bug], &[]).await, Err(ServiceError::Forbidden)),
        "authors cannot triage"
    );

    let ctx = fixture.open(&fixture.alice, number).await;
    let mut events = fixture.services.events.subscribe();
    issues.change_labels(&fixture.alice, &ctx, &[bug], &[]).await.unwrap();
    issues.change_labels(&fixture.alice, &ctx, &[bug], &[]).await.unwrap();
    issues.change_labels(&fixture.alice, &ctx, &[], &[bug]).await.unwrap();
    issues.change_labels(&fixture.alice, &ctx, &[], &[bug]).await.unwrap();
    let actions = issue_actions(&mut events);
    assert!(matches!(actions.as_slice(), [IssueAction::Labeled { .. }, IssueAction::Unlabeled { .. }]), "once per change: {actions:?}");
    assert_eq!(fixture.events_of(&fixture.alice, number).await.len(), 2);

    let other = CreateRepository { name: "other".into(), description: String::new(), visibility: Visibility::Public };
    fixture.services.repositories.create(&fixture.alice, other).await.unwrap();
    let foreign = {
        let other_repo = fixture.services.repositories.open(Some(&fixture.alice), "alice", "other").await.unwrap();
        let input = LabelInput { name: "theirs".into(), color: "#000000".into(), description: String::new() };
        issues.create_label(&fixture.alice, &other_repo, &input).await.unwrap().id
    };
    assert!(
        matches!(issues.change_labels(&fixture.alice, &ctx, &[foreign], &[]).await, Err(ServiceError::Invalid(_))),
        "labels of other repositories"
    );

    let mut many = Vec::new();
    for i in 0..26 {
        many.push(fixture.label(&format!("l{i}")).await);
    }
    assert!(matches!(issues.set_labels(&fixture.alice, &ctx, &many).await, Err(ServiceError::Invalid(_))), "25 per issue");
    issues.set_labels(&fixture.alice, &ctx, &many[..25]).await.unwrap();
    issues.set_labels(&fixture.alice, &ctx, &many[1..3]).await.unwrap();
    assert_eq!(issues.metadata(&[ctx.issue.id]).await.unwrap()[&ctx.issue.id].labels.len(), 2);

    issues.delete_label(&repo, many[1]).await.unwrap();
    assert_eq!(issues.metadata(&[ctx.issue.id]).await.unwrap()[&ctx.issue.id].labels.len(), 1, "deleting a label removes it");

    let empty = fixture.services.repositories.open(Some(&fixture.alice), "alice", "other").await.unwrap();
    assert_eq!(issues.create_default_labels(&fixture.alice, &repo).await.unwrap(), 0, "only when there are none");
    issues.delete_label(&empty, foreign).await.unwrap();
    assert_eq!(issues.create_default_labels(&fixture.alice, &empty).await.unwrap(), 9);
}

mod attachments;
