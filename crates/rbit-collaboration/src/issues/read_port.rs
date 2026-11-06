//! Static read ports keep storage selection and concurrent SQL in concrete adapters.
use super::{IssueHydration, MetadataNeeds, MetadataRows, OneIssueMetadata, SearchFilter};
use crate::{
    labels::LabelSnapshot,
    threads::{Issue, IssueId},
};
use rbit_contracts::{AccountId, RepoId};
use rbit_identity_models::{Account, User};
use rbit_repositories::OwnedRepository;
use std::{collections::HashMap, future::Future};
#[derive(Debug)]
pub enum IssueReadError<E> {
    NotFound,
    Port(E),
}
/// Authorization belongs to the caller's repository context; dashboards reload their viewer.
#[derive(Debug, Clone, Copy)]
pub struct IssueReadAccess {
    pub repo: RepoId,
    pub can_read: bool,
}
pub trait IssueSearchPort: Sync {
    type Error;
    fn state_counts(&self, filter: &SearchFilter) -> impl Future<Output = Result<(u64, u64), Self::Error>> + Send;
    fn search(&self, filter: &SearchFilter, limit: u32, offset: u64) -> impl Future<Output = Result<Vec<Issue>, Self::Error>> + Send;
}
pub trait IssueMetadataPort: Sync {
    type Error;
    fn one(&self, issue: IssueId) -> impl Future<Output = Result<OneIssueMetadata, Self::Error>> + Send;
    fn batch(&self, issues: &[IssueId]) -> impl Future<Output = Result<MetadataRows, Self::Error>> + Send;
    fn labels(&self, issues: &[IssueId]) -> impl Future<Output = Result<Vec<(IssueId, LabelSnapshot)>, Self::Error>> + Send;
}
pub trait IssueReadPort: Sync {
    type Error;
    type Search: IssueSearchPort<Error = Self::Error>;
    type Metadata: IssueMetadataPort<Error = Self::Error>;
    fn search_store(&self) -> Self::Search;
    fn metadata_store(&self) -> Self::Metadata;
    fn account_named(&self, name: &str) -> impl Future<Output = Result<Option<Account>, Self::Error>> + Send;
    fn active_user(&self, id: AccountId) -> impl Future<Output = Result<Option<User>, Self::Error>> + Send;
    fn accounts(&self, ids: &[AccountId]) -> impl Future<Output = Result<Vec<Account>, Self::Error>> + Send;
    /// The HTML path joins both existing futures; the API path deliberately remains sequential.
    fn hydrate(
        &self,
        issues: &[IssueId],
        authors: &[AccountId],
        needs: MetadataNeeds,
    ) -> impl Future<Output = Result<IssueHydration, Self::Error>> + Send;
    fn readable_repositories(
        &self,
        ids: &[RepoId],
        viewer: &User,
    ) -> impl Future<Output = Result<HashMap<RepoId, OwnedRepository>, Self::Error>> + Send;
}
