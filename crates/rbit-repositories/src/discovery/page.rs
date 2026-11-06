//! Public discovery inputs and page arithmetic, preserving legacy public fields.

use crate::{ArchivedFilter, OwnedRepository, RepoSort};

/// Explore page input, exactly as requested (`?q=…&sort=…&page=…`).
#[derive(Debug, Clone, Default)]
pub struct RepoSearch {
    pub text: String,
    pub sort: RepoSort,
    /// Archived repositories are listed unless this says otherwise.
    pub archived: ArchivedFilter,
    /// 1-based; `0` is treated as `1`.
    pub page: u32,
}

/// One page of explore results.
#[derive(Debug, Clone)]
pub struct RepoSearchResults {
    pub repos: Vec<OwnedRepository>,
    /// Matches across all pages.
    pub total: u64,
    /// 1-based page number actually served.
    pub page: u32,
    pub per_page: u32,
}

impl RepoSearchResults {
    /// Number of pages (at least 1, so "page 1 of 1" holds for empty results).
    pub fn page_count(&self) -> u32 {
        let pages = self.total.div_ceil(u64::from(self.per_page)).max(1);
        u32::try_from(pages).unwrap_or(u32::MAX)
    }

    pub fn has_previous(&self) -> bool {
        self.page > 1
    }

    pub fn has_next(&self) -> bool {
        self.page < self.page_count()
    }
}
