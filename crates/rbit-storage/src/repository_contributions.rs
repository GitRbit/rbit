//! Contribution scans list repositories with the same visibility SQL as repository pages.
use crate::{RepositoryStore, StorageError};
use rbit_core::{AccountId, repository::OwnedRepository};
use rbit_repositories::contributions::ContributionRepository;
use std::future::Future;

impl ContributionRepository for RepositoryStore {
    type Error = StorageError;
    fn owned_readable(
        &self,
        owner: AccountId,
        viewer: Option<AccountId>,
        viewer_is_admin: bool,
    ) -> impl Future<Output = Result<Vec<OwnedRepository>, StorageError>> + Send {
        self.list_by_owner(owner, viewer, viewer_is_admin)
    }
    fn readable(
        &self,
        viewer: Option<AccountId>,
        viewer_is_admin: bool,
    ) -> impl Future<Output = Result<Vec<OwnedRepository>, StorageError>> + Send {
        self.contribution_repositories(viewer, viewer_is_admin)
    }
}
