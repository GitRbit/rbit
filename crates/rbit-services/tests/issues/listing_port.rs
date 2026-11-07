//! Production filtered reads retain original SQL work, pagination and current authority.
use super::Fixture;
use rbit_core::{IssueId, IssueKind, RepoAccess, issue_query::IssueQuery};
use rbit_services::{
    ServiceError,
    issues::{DashboardRelation, MetadataNeeds, SearchFilter},
};
use rbit_storage::count_work;
use sqlx::Connection as _;
#[test]
fn query_filters_and_results_keep_original_public_type_identity() {
    use rbit_collaboration::issues as owned;
    use std::any::TypeId;
    assert_eq!(TypeId::of::<IssueQuery>(), TypeId::of::<owned::query::IssueQuery>());
    assert_eq!(TypeId::of::<SearchFilter>(), TypeId::of::<owned::SearchFilter>());
    assert_eq!(TypeId::of::<rbit_services::issues::IssueSearchResults>(), TypeId::of::<owned::IssueSearchResults>());
    assert_eq!(TypeId::of::<rbit_services::issues::IssueMetadata>(), TypeId::of::<owned::IssueMetadata>());
    assert_eq!(TypeId::of::<rbit_storage::sqlite::issue_meta::OneIssueMetadata>(), TypeId::of::<owned::OneIssueMetadata>());
}
#[tokio::test]
async fn selected_page_single_multi_labels_and_empty_paths_keep_exact_work() {
    let fixture = Fixture::new().await;
    let repo = fixture.repo(&fixture.alice).await;
    let first = fixture.create(&fixture.alice, "first", "").await;
    let reader = fixture.services.issues.reader();
    let (result, work) = count_work(reader.search(Some(&fixture.alice), &repo, &IssueQuery::parse("is:open"), u32::MAX)).await;
    let result = result.unwrap();
    let first_id = result.issues[0].id;
    assert_eq!((result.page, result.total), (1, 1));
    assert_eq!((work.acquisitions, work.statements), (4, 4));
    fixture.create(&fixture.alice, "second", "").await;
    let (result, work) = count_work(reader.search(Some(&fixture.alice), &repo, &IssueQuery::parse("is:open"), 1)).await;
    let result = result.unwrap();
    assert_eq!(result.issues.len(), 2);
    assert_eq!(result.authors.len(), 1);
    assert_eq!(result.metadata.len(), 2);
    assert_eq!((work.acquisitions, work.statements), (6, 6));
    let (result, work) = count_work(reader.search_with(None, &repo, &IssueQuery::parse("is:open"), 1, MetadataNeeds::Labels)).await;
    assert_eq!(result.unwrap().metadata.len(), 2);
    assert_eq!((work.acquisitions, work.statements), (4, 4));
    let (result, work) = count_work(reader.search(None, &repo, &IssueQuery::parse("label:absent"), 1)).await;
    assert_eq!(result.unwrap().issues.len(), 0);
    assert_eq!((work.acquisitions, work.statements), (2, 2));
    let (result, work) = count_work(reader.metadata(&[])).await;
    assert_eq!(result.unwrap().len(), 0);
    assert_eq!(work.statements, 0);
    let (result, work) = count_work(reader.metadata(&[first_id, IssueId(i64::MAX)])).await;
    assert_eq!(result.unwrap().len(), 2);
    assert_eq!((work.acquisitions, work.statements), (3, 3));
    assert_eq!(first.get(), 1);
}
#[tokio::test]
async fn api_does_not_clamp_and_denied_routes_do_not_read_even_malformed_qualifiers() {
    let fixture = Fixture::new().await;
    fixture.create(&fixture.alice, "first", "").await;
    let mut repo = fixture.repo(&fixture.alice).await;
    let reader = fixture.services.issues.reader();
    let (result, work) = count_work(reader.list(&repo, SearchFilter::new(rbit_core::RepoId(i64::MAX)), u32::MAX, 0)).await;
    let result = result.unwrap();
    assert_eq!((result.total, result.page, result.per_page), (1, u32::MAX, 1));
    assert_eq!(result.issues.len(), 0);
    assert_eq!((work.acquisitions, work.statements), (2, 2));
    repo.access = RepoAccess::None;
    let (result, work) = count_work(reader.search(None, &repo, &IssueQuery::parse("author:alice assignee:bob"), 1)).await;
    assert!(matches!(result, Err(ServiceError::NotFound)));
    assert_eq!(work.statements, 0);
    let (result, work) = count_work(reader.list(&repo, SearchFilter::new(repo.repo.repo.id), 1, 25)).await;
    assert!(matches!(result, Err(ServiceError::NotFound)));
    assert_eq!(work.statements, 0);
}
#[tokio::test]
async fn stale_dashboard_user_loses_suspension_and_admin_authority_before_any_search() {
    let fixture = Fixture::new().await;
    let reader = fixture.services.issues.reader();
    fixture.create(&fixture.bob, "private assigned relationship", "").await;
    // The caller still holds a once-admin user, but the database authority is newer.
    let mut stale = fixture.bob.clone();
    stale.is_admin = true;
    let options = sqlx::sqlite::SqliteConnectOptions::new().filename(fixture.directory.path().join("rbit.db"));
    let mut connection = sqlx::SqliteConnection::connect_with(&options).await.unwrap();
    sqlx::query("UPDATE repositories SET visibility='private' WHERE owner_id=?")
        .bind(fixture.alice.id().0)
        .execute(&mut connection)
        .await
        .unwrap();
    let (result, work) =
        count_work(reader.dashboard(&stale, IssueKind::Issue, DashboardRelation::Created, &IssueQuery::parse("is:open"), 1)).await;
    assert_eq!(result.unwrap().issues.len(), 0);
    // Active-user lookup, counts and rows; empty hydration/repository batches do no SQL.
    assert_eq!((work.acquisitions, work.statements), (3, 3));
    fixture.database.admin().set_suspended(fixture.bob.id(), true).await.unwrap();
    let (result, work) =
        count_work(reader.dashboard(&stale, IssueKind::Issue, DashboardRelation::Created, &IssueQuery::parse("author:alice"), 1)).await;
    assert!(matches!(result, Err(ServiceError::NotFound)));
    assert_eq!((work.acquisitions, work.statements), (1, 1));
    connection.close().await.unwrap();
}

#[path = "listing_race.rs"]
mod race;
