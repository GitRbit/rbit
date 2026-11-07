//! Concrete persistence and Git/event adapters preserve conditional SQL and resource ownership.
use super::RepoGeneralSettings;
use crate::RepoName;
use rbit_contracts::{AccountId, RepoId, Timestamp};
use rbit_identity_models::User;
use std::future::Future;
pub trait RepositorySettingsRepository: Sync {
    type Error;
    fn update_general(&self, id: RepoId, settings: &RepoGeneralSettings) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn rename(&self, id: RepoId, name: &RepoName) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn archive(&self, id: RepoId, actor: AccountId, at: Timestamp) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    fn unarchive(&self, id: RepoId) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    fn delete(&self, id: RepoId) -> impl Future<Output = Result<(), Self::Error>> + Send;
}
pub trait RepositorySettingsPort: Sync {
    type Error;
    type StoreError;
    type Store: RepositorySettingsRepository<Error = Self::StoreError>;
    fn store(&self) -> Self::Store;
    fn has_private_ancestor(&self) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    fn branch_exists(&self, branch: &str) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    fn set_head(&self, branch: &str) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn restore_failed(&self, error: &Self::Error);
    fn updated(&self, settings: &RepoGeneralSettings);
    fn renamed(&self, name: &RepoName);
    /// Existing best-effort audit precedes the transient event; neither is a new outbox.
    fn archive_committed(&self, actor: &User, archived: bool) -> impl Future<Output = ()> + Send;
    fn close_reader(&self);
    fn delete_disk(&self) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn deleted(&self, actor: &User, full_name: String);
}
