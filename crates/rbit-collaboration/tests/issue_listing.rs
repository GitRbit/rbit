#![cfg(feature = "applications")]
//! Independent listing authority, pagination, construction and projection ordering.
#[path = "support/thread_fixtures.rs"]
mod fixtures;
#[path = "support/listing_port.rs"]
mod port;
use port::{Port, user};
use rbit_collaboration::{
    issues::{
        DashboardRelation, DashboardScope, IssueListing, IssueMetadataReader, IssueReadAccess, IssueReadError, IssueReadPort,
        MetadataNeeds, SearchFilter, query::IssueQuery,
    },
    threads::{IssueId, IssueKind},
};
use rbit_contracts::RepoId;
fn access(can_read: bool) -> IssueReadAccess {
    IssueReadAccess { repo: RepoId(3), can_read }
}
#[tokio::test]
async fn denied_and_unpolled_reads_do_not_construct_or_resolve_anything() {
    let port = Port::default();
    let binding = &port;
    let app = IssueListing::new(&binding);
    let query = IssueQuery::parse("author:alice assignee:bob");
    assert!(matches!(app.search(None, access(false), &query, 1).await, Err(IssueReadError::NotFound)));
    assert!(matches!(app.list(access(false), SearchFilter::new(RepoId(9)), 1, 25).await, Err(IssueReadError::NotFound)));
    drop(app.search(None, access(true), &query, 1));
    assert_eq!(port.calls(), Vec::<String>::new());
}
#[tokio::test]
async fn html_clamps_after_counts_and_uses_joined_hydration_with_ordered_qualifiers() {
    let port = Port::default();
    let binding = &port;
    let query = IssueQuery::parse("is:open author:missing assignee:@me unknown:x");
    let result = IssueListing::new(&binding).search_with(None, access(true), &query, u32::MAX, MetadataNeeds::Labels).await.unwrap();
    assert_eq!((result.page, result.total, result.open, result.closed), (2, 26, 26, 3));
    assert_eq!(result.hints, ["“unknown:x” is not understood and was ignored", "Sign in to use @me"]);
    assert_eq!(
        port.calls(),
        ["name:missing", "search_store", "counts:3:false", "rows:25:25", "hydrate:[IssueId(1)]:[AccountId(4)]:Labels"]
    );
    assert_eq!(port.filters.lock().unwrap()[0].author, Some(rbit_contracts::AccountId(0)));
}
#[tokio::test]
async fn api_scrubs_dashboard_and_keeps_requested_page_with_sequential_author_then_metadata() {
    let port = Port::default();
    let binding = &port;
    let mut filter = SearchFilter::new(RepoId(9));
    filter.dashboard = Some(DashboardScope { viewer: user().id(), is_admin: true, relation: DashboardRelation::Created });
    let result = IssueListing::new(&binding).list(access(true), filter, 100, 0).await.unwrap();
    assert_eq!((result.page, result.per_page, result.total), (100, 1, 29));
    assert_eq!(port.calls(), ["search_store", "counts:3:false", "rows:1:99", "accounts:[AccountId(4)]", "metadata_store", "one:1"]);
}
#[tokio::test]
async fn dashboard_reloads_before_names_and_uses_fresh_admin_for_scope_and_visibility() {
    let port = Port::default();
    let binding = &port;
    let mut stale = user();
    stale.is_admin = true;
    let result = IssueListing::new(&binding)
        .dashboard(&stale, IssueKind::Pull, DashboardRelation::Created, &IssueQuery::parse("author:missing"), 1)
        .await
        .unwrap();
    assert_eq!(result.issues.len(), 1);
    assert_eq!(
        port.calls(),
        [
            "active",
            "name:missing",
            "search_store",
            "counts:0:true",
            "rows:25:0",
            "hydrate:[IssueId(1)]:[AccountId(4)]:All",
            "readable:[RepoId(3)]:false"
        ]
    );
    assert!(!port.filters.lock().unwrap()[0].dashboard.unwrap().is_admin);
    let port = Port { inactive: true, ..Port::default() };
    let binding = &port;
    assert!(matches!(
        IssueListing::new(&binding)
            .dashboard(&stale, IssueKind::Issue, DashboardRelation::Created, &IssueQuery::parse("author:missing"), 1)
            .await,
        Err(IssueReadError::NotFound)
    ));
    assert_eq!(port.calls(), ["active"]);
}
#[tokio::test]
async fn metadata_single_batch_empty_and_labels_keep_distinct_raw_read_paths() {
    let port = Port::default();
    let binding = &port;
    let store = binding.metadata_store();
    let app = IssueMetadataReader::new(&store);
    assert_eq!(app.read(&[]).await.unwrap().len(), 0);
    assert_eq!(app.read(&[IssueId(1)]).await.unwrap().len(), 1);
    assert_eq!(app.read(&[IssueId(1), IssueId(2)]).await.unwrap().len(), 2);
    assert_eq!(app.read_for(&[IssueId(1)], MetadataNeeds::Labels).await.unwrap().len(), 1);
    assert_eq!(port.calls(), ["metadata_store", "batch:[]", "one:1", "batch:[IssueId(1), IssueId(2)]", "labels:[IssueId(1)]"]);
}
#[tokio::test]
async fn failed_author_lookup_precedes_assignee_and_no_page_store_is_constructed() {
    let port = Port { fail_name: true, ..Port::default() };
    let binding = &port;
    assert!(matches!(
        IssueListing::new(&binding).search(None, access(true), &IssueQuery::parse("author:a assignee:b"), 1).await,
        Err(IssueReadError::Port("name"))
    ));
    assert_eq!(port.calls(), ["name:a"]);
}
