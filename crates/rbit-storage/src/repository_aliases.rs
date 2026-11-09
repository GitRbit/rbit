//! Current and former repository addresses over the existing lookups; migration 0016
//! triggers remain the only authority that records and retires old addresses.
use crate::{RepositoryStore, StorageError};
use rbit_repositories::{OwnedRepository, aliases::RepositoryAddresses};
use std::future::Future;

impl RepositoryAddresses for RepositoryStore {
    type Error = StorageError;
    fn find_current(&self, owner: &str, name: &str) -> impl Future<Output = Result<Option<OwnedRepository>, StorageError>> + Send {
        self.find_by_path(owner, name)
    }
    fn find_old(&self, owner: &str, name: &str) -> impl Future<Output = Result<Option<OwnedRepository>, StorageError>> + Send {
        self.find_by_old_path(owner, name)
    }
}
