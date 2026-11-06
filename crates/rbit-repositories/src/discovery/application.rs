//! Search normalization and bounded public discovery pagination.

use super::{PublicRepositoryCatalog, RepoSearch, RepoSearchResults};

/// Explore results per page.
pub const EXPLORE_PAGE_SIZE: u32 = 30;
/// Longer search text is truncated; nothing useful is that long.
const MAX_SEARCH_LEN: usize = 100;

/// Borrows the concrete catalog; no runtime, service registry or cached grants.
#[derive(Debug)]
pub struct RepositoryDiscovery<'a, S> {
    catalog: &'a S,
}

impl<'a, S: PublicRepositoryCatalog> RepositoryDiscovery<'a, S> {
    pub fn new(catalog: &'a S) -> Self {
        Self { catalog }
    }

    /// Counts, clamps and loads one page in the same order as the legacy service.
    /// Concurrent mutations can make totals stale; storage filters rows again.
    pub async fn explore(&self, search: &RepoSearch) -> Result<RepoSearchResults, S::Error> {
        let text: String = search.text.trim().chars().take(MAX_SEARCH_LEN).collect();
        let total = u64::try_from(self.catalog.count_public(&text, search.archived).await?).unwrap_or_default();
        let mut results = RepoSearchResults { repos: Vec::new(), total, page: search.page.max(1), per_page: EXPLORE_PAGE_SIZE };
        results.page = results.page.min(results.page_count());

        let offset = (results.page - 1).saturating_mul(EXPLORE_PAGE_SIZE);
        results.repos = self.catalog.search_public(&text, search.sort, search.archived, EXPLORE_PAGE_SIZE, offset).await?;
        Ok(results)
    }
}
