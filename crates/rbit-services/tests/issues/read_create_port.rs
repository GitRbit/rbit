//! Production issue read/create behavior against actual stores and legacy event adapters.

use super::{Fixture, issue_actions};
use rbit_core::{IssueKind, IssueNumber, RepoAccess, Timestamp, issue::IssueAction, membership::Permission};
use rbit_services::{
    ServiceError,
    issues::{MilestoneInput, NewIssueInput},
};
use rbit_storage::count_work;
use tokio::sync::broadcast::error::TryRecvError;

#[test]
fn original_core_and_storage_thread_paths_are_identical_feature_types() {
    use rbit_collaboration::{issues, threads};
    use std::any::TypeId;
    assert_eq!(TypeId::of::<rbit_core::Issue>(), TypeId::of::<threads::Issue>());
    assert_eq!(TypeId::of::<rbit_core::IssueId>(), TypeId::of::<threads::IssueId>());
    assert_eq!(TypeId::of::<IssueNumber>(), TypeId::of::<threads::IssueNumber>());
    assert_eq!(TypeId::of::<IssueKind>(), TypeId::of::<threads::IssueKind>());
    assert_eq!(TypeId::of::<rbit_core::IssueTitle>(), TypeId::of::<threads::IssueTitle>());
    assert_eq!(TypeId::of::<rbit_core::IssueText>(), TypeId::of::<threads::IssueText>());
    assert_eq!(TypeId::of::<rbit_storage::sqlite::issues::NewIssue>(), TypeId::of::<threads::NewIssue>());
    assert_eq!(TypeId::of::<NewIssueInput>(), TypeId::of::<issues::NewIssueInput>());
    assert_eq!(TypeId::of::<rbit_services::timeline::OpenCounts>(), TypeId::of::<threads::OpenCounts>());
}

#[tokio::test]
async fn empty_metadata_create_joined_open_and_counts_keep_exact_sql_work_and_events() {
    let fixture = Fixture::new().await;
    let repo = fixture.repo(&fixture.alice).await;
    let mut events = fixture.services.events.subscribe();
    let input = NewIssueInput { title: " title ".into(), body: "a\r\nb\rc".into(), ..NewIssueInput::default() };
    let (created, work) = count_work(fixture.services.issues.create(&fixture.alice, &repo, input)).await;
    let created = created.unwrap();
    assert_eq!((work.acquisitions, work.statements), (1, 2));
    assert_eq!((created.title.as_str(), created.body.as_str()), ("title", "a\nb\nc"));
    assert_eq!(issue_actions(&mut events), [IssueAction::Opened]);
    let mut archived = repo.clone();
    archived.repo.repo.archived_at = Some(Timestamp(1));
    let (opened, work) = count_work(fixture.services.timeline.open(Some(&fixture.alice), archived, created.number)).await;
    let opened = opened.unwrap();
    assert_eq!((work.acquisitions, work.statements), (1, 1));
    assert_eq!(opened.author.unwrap().id, fixture.alice.id());
    assert!(opened.permissions.can_edit);
    let (counts, work) = count_work(fixture.services.timeline.open_counts(repo.repo.repo.id)).await;
    assert_eq!(counts.unwrap().issues, 1);
    assert_eq!(work.statements, 1);
}

#[tokio::test]
async fn private_archive_metadata_and_text_error_order_retains_original_query_counts() {
    let fixture = Fixture::new().await;
    let mut repo = fixture.repo(&fixture.alice).await;
    let mut input = NewIssueInput { title: String::new(), labels: vec![rbit_core::LabelId(i64::MAX)], ..NewIssueInput::default() };
    repo.access = RepoAccess::None;
    repo.repo.repo.archived_at = Some(Timestamp(1));
    let (result, work) = count_work(fixture.services.issues.create(&fixture.alice, &repo, input.clone())).await;
    assert!(matches!(result, Err(ServiceError::NotFound)));
    assert_eq!(work.statements, 0);
    let (opened, work) = count_work(fixture.services.timeline.open(Some(&fixture.alice), repo, IssueNumber::new(1).unwrap())).await;
    assert!(matches!(opened, Err(ServiceError::NotFound)));
    assert_eq!(work.statements, 0);
    repo = fixture.repo(&fixture.alice).await;
    repo.repo.repo.archived_at = Some(Timestamp(1));
    let (result, work) = count_work(fixture.services.issues.create(&fixture.alice, &repo, input.clone())).await;
    assert!(matches!(result, Err(ServiceError::Archived)));
    assert_eq!(work.statements, 0);
    repo.repo.repo.archived_at = None;
    let (result, work) = count_work(fixture.services.issues.create(&fixture.alice, &repo, input.clone())).await;
    assert!(matches!(result,Err(ServiceError::Invalid(message)) if message=="unknown label"));
    assert_eq!(work.statements, 1);
    let reader = fixture.repo(&fixture.bob).await;
    let (result, work) = count_work(fixture.services.issues.create(&fixture.bob, &reader, input.clone())).await;
    assert!(matches!(result,Err(ServiceError::Invalid(message)) if message=="title must not be empty"));
    assert_eq!(work.statements, 0);
    input.title = "reader issue".into();
    let (result, work) = count_work(fixture.services.issues.create(&fixture.bob, &reader, input)).await;
    assert!(result.is_ok());
    assert_eq!(work.statements, 2);
}

#[tokio::test]
async fn racing_creates_use_existing_counter_transaction_and_publish_once_per_committed_row() {
    let fixture = Fixture::new().await;
    let repo = fixture.repo(&fixture.alice).await;
    let input = NewIssueInput { title: "race".into(), ..NewIssueInput::default() };
    let mut events = fixture.services.events.subscribe();
    let (first, second) = tokio::join!(
        fixture.services.issues.create(&fixture.alice, &repo, input.clone()),
        fixture.services.issues.create(&fixture.alice, &repo, input)
    );
    let mut numbers = [first.unwrap().number.get(), second.unwrap().number.get()];
    numbers.sort_unstable();
    assert_eq!(numbers, [1, 2]);
    assert_eq!(issue_actions(&mut events), [IssueAction::Opened, IssueAction::Opened]);
}

#[tokio::test]
async fn metadata_transaction_failure_keeps_existing_committed_row_gap_without_opened_event() {
    use sqlx::Connection as _;
    let fixture = Fixture::new().await;
    let repo = fixture.repo(&fixture.alice).await;
    let label = fixture.label("bug").await;
    let options = sqlx::sqlite::SqliteConnectOptions::new().filename(fixture._dir.path().join("rbit.db"));
    let mut connection = sqlx::SqliteConnection::connect_with(&options).await.unwrap();
    sqlx::query(
        "CREATE TRIGGER fixture_reject_metadata BEFORE INSERT ON issue_labels BEGIN SELECT RAISE(ABORT, 'fixture metadata failure'); END",
    )
    .execute(&mut connection)
    .await
    .unwrap();
    let mut events = fixture.services.events.subscribe();
    let input = NewIssueInput { title: "committed before metadata".into(), labels: vec![label], ..NewIssueInput::default() };
    let (result, work) = count_work(fixture.services.issues.create(&fixture.alice, &repo, input)).await;
    assert!(matches!(result, Err(ServiceError::Internal(_))));
    assert_eq!(work.statements, 4);
    let row = fixture.database.issues().find_by_number(repo.repo.repo.id, IssueNumber::new(1).unwrap()).await.unwrap().unwrap();
    assert_eq!(row.title.as_str(), "committed before metadata");
    assert!(fixture.database.issue_meta().labels_of(&[row.id]).await.unwrap().is_empty());
    assert!(fixture.events_of(&fixture.alice, row.number).await.is_empty());
    assert!(matches!(events.try_recv(), Err(TryRecvError::Empty)));
    connection.close().await.unwrap();
}

#[tokio::test]
async fn writers_open_issues_with_metadata_and_opened_is_published_once() {
    let fixture = Fixture::new().await;
    let repo = fixture.repo(&fixture.alice).await;
    fixture.services.collaborators.add(&fixture.alice, &repo, "bob", Permission::Write).await.unwrap();
    let bug = fixture.label("bug").await;
    let milestone = fixture
        .services
        .issues
        .create_milestone(&fixture.alice, &repo, &MilestoneInput { title: "v1".into(), ..MilestoneInput::default() })
        .await
        .unwrap();
    let mut events = fixture.services.events.subscribe();

    let input = NewIssueInput {
        title: "Crash".into(),
        body: "Boom".into(),
        labels: vec![bug, bug],
        assignees: vec![fixture.bob.id()],
        milestone: Some(milestone.id),
    };
    let issue = fixture.services.issues.create(&fixture.alice, &repo, input).await.unwrap();
    assert_eq!(issue_actions(&mut events), [IssueAction::Opened], "metadata changes are not published separately");
    let metadata = fixture.services.issues.metadata(&[issue.id]).await.unwrap().remove(&issue.id).unwrap();
    assert_eq!(metadata.labels.len(), 1);
    assert_eq!(metadata.assignees[0].id, fixture.bob.id());
    assert_eq!(metadata.milestone.unwrap().title, "v1");
    let recorded = fixture.events_of(&fixture.alice, issue.number).await;
    assert_eq!(recorded.len(), 3, "labeled, assigned and milestoned: {recorded:?}");
    // A list that shows only labels reads only labels.
    let query = rbit_core::issue_query::IssueQuery::parse("is:open");
    let labels_only = rbit_services::issues::MetadataNeeds::Labels;
    let listed = fixture.services.issues.search_with(Some(&fixture.alice), &repo, &query, 1, labels_only).await.unwrap();
    let listed = &listed.metadata[&issue.id];
    assert_eq!(listed.labels.len(), 1);
    assert!(listed.assignees.is_empty() && listed.milestone.is_none());

    // A reader's metadata is ignored; a carol (not a writer) cannot be assigned.
    let reader_repo = fixture.repo(&fixture.carol).await;
    let input = NewIssueInput { title: "Question".into(), labels: vec![bug], ..NewIssueInput::default() };
    let theirs = fixture.services.issues.create(&fixture.carol, &reader_repo, input).await.unwrap();
    assert_eq!(fixture.services.issues.metadata(&[theirs.id]).await.unwrap()[&theirs.id].labels, []);
    let input = NewIssueInput { title: "x".into(), assignees: vec![fixture.carol.id()], ..NewIssueInput::default() };
    assert!(matches!(fixture.services.issues.create(&fixture.alice, &repo, input).await, Err(ServiceError::Invalid(_))));
    let input = NewIssueInput { title: " ".into(), ..NewIssueInput::default() };
    assert!(matches!(fixture.services.issues.create(&fixture.alice, &repo, input).await, Err(ServiceError::Invalid(_))));
}
