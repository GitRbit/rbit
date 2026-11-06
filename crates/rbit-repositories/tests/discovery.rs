//! Application authority and ordering without a runtime or concrete persistence.

use std::{
    fmt,
    future::Future,
    pin::pin,
    sync::Mutex,
    task::{Context, Poll, Waker},
};

use rbit_repositories::{
    ArchivedFilter, OwnedRepository, RepoSort,
    discovery::{PublicRepositoryCatalog, RepoSearch, RepoSearchResults, RepositoryDiscovery},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Failure {
    Count,
    Rows,
}
impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Failure {}

#[derive(Debug, PartialEq, Eq)]
enum Call {
    Count(String, ArchivedFilter),
    Rows(String, RepoSort, ArchivedFilter, u32, u32),
}

#[derive(Debug, Default)]
struct Catalog {
    total: i64,
    fail_count: bool,
    fail_rows: bool,
    calls: Mutex<Vec<Call>>,
}
impl PublicRepositoryCatalog for Catalog {
    type Error = Failure;

    async fn count_public(&self, text: &str, archived: ArchivedFilter) -> Result<i64, Failure> {
        self.calls.lock().unwrap().push(Call::Count(text.into(), archived));
        if self.fail_count { Err(Failure::Count) } else { Ok(self.total) }
    }

    async fn search_public(
        &self,
        text: &str,
        sort: RepoSort,
        archived: ArchivedFilter,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<OwnedRepository>, Failure> {
        self.calls.lock().unwrap().push(Call::Rows(text.into(), sort, archived, limit, offset));
        if self.fail_rows { Err(Failure::Rows) } else { Ok(Vec::new()) }
    }
}

/// Every fake port is immediately ready; pending indicates an accidental test change.
fn ready<T>(future: impl Future<Output = T>) -> T {
    match pin!(future).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("test port unexpectedly suspended"),
    }
}

#[test]
fn normalization_counts_unicode_characters_and_preserves_literal_search_text() {
    let text = format!("  %_\\{}TAIL  ", "é".repeat(100));
    let expected: String = text.trim().chars().take(100).collect();
    assert_eq!(expected.chars().count(), 100);
    assert!(expected.len() > 100);
    let catalog = Catalog { total: 31, ..Catalog::default() };
    let search = RepoSearch { text, sort: RepoSort::Name, archived: ArchivedFilter::Only, page: 99 };
    let result = ready(RepositoryDiscovery::new(&catalog).explore(&search)).unwrap();
    assert_eq!((result.total, result.page, result.per_page), (31, 2, 30));
    assert_eq!(
        *catalog.calls.lock().unwrap(),
        [Call::Count(expected.clone(), ArchivedFilter::Only), Call::Rows(expected, RepoSort::Name, ArchivedFilter::Only, 30, 30)]
    );
}

#[test]
fn all_selectors_forward_without_reinterpretation() {
    for sort in RepoSort::ALL {
        for archived in ArchivedFilter::ALL {
            let catalog = Catalog::default();
            let search = RepoSearch { sort, archived, ..RepoSearch::default() };
            let result = ready(RepositoryDiscovery::new(&catalog).explore(&search)).unwrap();
            assert_eq!((result.total, result.page, result.page_count()), (0, 1, 1));
            assert_eq!(
                *catalog.calls.lock().unwrap(),
                [Call::Count(String::new(), archived), Call::Rows(String::new(), sort, archived, 30, 0)]
            );
        }
    }
}

#[test]
fn page_zero_and_negative_counts_keep_legacy_fallbacks_and_still_fetch_rows() {
    let catalog = Catalog { total: -1, ..Catalog::default() };
    let result = ready(RepositoryDiscovery::new(&catalog).explore(&RepoSearch::default())).unwrap();
    assert_eq!((result.total, result.page, result.per_page), (0, 1, 30));
    assert_eq!(catalog.calls.lock().unwrap().len(), 2);
}

#[test]
fn count_error_short_circuits_rows_and_row_error_propagates_unchanged() {
    let count = Catalog { fail_count: true, ..Catalog::default() };
    assert!(matches!(ready(RepositoryDiscovery::new(&count).explore(&RepoSearch::default())), Err(Failure::Count)));
    assert_eq!(count.calls.lock().unwrap().len(), 1);
    let rows = Catalog { fail_rows: true, ..Catalog::default() };
    assert!(matches!(ready(RepositoryDiscovery::new(&rows).explore(&RepoSearch::default())), Err(Failure::Rows)));
    assert_eq!(rows.calls.lock().unwrap().len(), 2);
}

#[test]
fn separate_queries_allow_stale_totals_and_empty_rows_without_synthesizing_a_snapshot() {
    // Count can observe records that disappear or become private before the rows query.
    let catalog = Catalog { total: 31, ..Catalog::default() };
    let search = RepoSearch { page: 2, ..RepoSearch::default() };
    let result = ready(RepositoryDiscovery::new(&catalog).explore(&search)).unwrap();
    assert_eq!((result.total, result.page, result.repos.len()), (31, 2, 0));
    assert!(result.has_previous());
    assert!(!result.has_next());
    assert_eq!(catalog.calls.lock().unwrap().len(), 2);
}

#[test]
fn extreme_page_and_offset_remain_saturating() {
    let catalog = Catalog { total: i64::MAX, ..Catalog::default() };
    let search = RepoSearch { page: u32::MAX, ..RepoSearch::default() };
    let result = ready(RepositoryDiscovery::new(&catalog).explore(&search)).unwrap();
    assert_eq!(result.page, u32::MAX);
    assert_eq!(result.page_count(), u32::MAX);
    assert_eq!(
        catalog.calls.lock().unwrap()[1],
        Call::Rows(String::new(), RepoSort::RecentlyUpdated, ArchivedFilter::Include, 30, u32::MAX)
    );
    let arbitrary = RepoSearchResults { repos: Vec::new(), total: u64::MAX, page: 0, per_page: 1 };
    assert_eq!(arbitrary.page_count(), u32::MAX);
    assert!(!arbitrary.has_previous());
    assert!(arbitrary.has_next());
}

#[test]
#[should_panic(expected = "divide by zero")]
fn arbitrary_public_zero_per_page_retains_legacy_division_behavior() {
    let arbitrary = RepoSearchResults { repos: Vec::new(), total: 0, page: 1, per_page: 0 };
    let _ = arbitrary.page_count();
}
