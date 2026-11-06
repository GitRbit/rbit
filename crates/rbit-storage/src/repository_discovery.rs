//! Static public-discovery port adapter; existing SQL remains the filtering authority.

use rbit_repositories::{ArchivedFilter, OwnedRepository, RepoSort, discovery::PublicRepositoryCatalog};

use crate::{RepositoryStore, StorageError};

impl PublicRepositoryCatalog for RepositoryStore {
    type Error = StorageError;

    async fn count_public(&self, text: &str, archived: ArchivedFilter) -> Result<i64, Self::Error> {
        Self::count_public(self, text, archived).await
    }

    async fn search_public(
        &self,
        text: &str,
        sort: RepoSort,
        archived: ArchivedFilter,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<OwnedRepository>, Self::Error> {
        Self::search_public(self, text, sort, archived, limit, offset).await
    }
}
