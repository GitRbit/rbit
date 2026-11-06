//! Persistence retains SQL filtering, ordering, row validation and concurrency authority.

use std::future::Future;

use crate::{ArchivedFilter, OwnedRepository, RepoSort};

/// Both operations must filter public visibility and the requested archive state.
/// Text is literal search text, not a LIKE pattern; escaping belongs to storage.
/// Count and rows are separate operations, preserving the existing non-snapshot
/// behavior when repositories change between them. Do not imply an atomic page.
pub trait PublicRepositoryCatalog: Sync {
    type Error: std::error::Error + Send + Sync + 'static;

    fn count_public(&self, text: &str, archived: ArchivedFilter) -> impl Future<Output = Result<i64, Self::Error>> + Send;

    fn search_public(
        &self,
        text: &str,
        sort: RepoSort,
        archived: ArchivedFilter,
        limit: u32,
        offset: u32,
    ) -> impl Future<Output = Result<Vec<OwnedRepository>, Self::Error>> + Send;
}
