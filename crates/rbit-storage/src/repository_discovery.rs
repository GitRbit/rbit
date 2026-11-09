//! Static public-discovery port adapter; existing SQL remains the filtering authority.

use rbit_core::AccountId;
use rbit_repositories::{
    ArchivedFilter, OwnedRepository, RepoSort,
    discovery::{PublicRepositoryCatalog, RepositoryListingCatalog},
};
use std::future::Future;

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

impl RepositoryListingCatalog for RepositoryStore {
    type Error = StorageError;
    fn owned_by(
        &self,
        owner: AccountId,
        viewer: Option<AccountId>,
        viewer_is_admin: bool,
    ) -> impl Future<Output = Result<Vec<OwnedRepository>, StorageError>> + Send {
        self.list_by_owner(owner, viewer, viewer_is_admin)
    }
    fn for_member(&self, member: AccountId, limit: u32) -> impl Future<Output = Result<Vec<OwnedRepository>, StorageError>> + Send {
        self.list_for_member(member, limit)
    }
    fn count_for_member(&self, member: AccountId) -> impl Future<Output = Result<u64, StorageError>> + Send {
        Self::count_for_member(self, member)
    }
    fn page_for_member(
        &self,
        member: AccountId,
        limit: u32,
        offset: u32,
    ) -> impl Future<Output = Result<Vec<OwnedRepository>, StorageError>> + Send {
        Self::page_for_member(self, member, limit, offset)
    }
    fn recent_public(&self, limit: u32) -> impl Future<Output = Result<Vec<OwnedRepository>, StorageError>> + Send {
        self.list_recent_public(limit)
    }
}
