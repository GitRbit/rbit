//! Static batched resolution and cross-reference storage/visibility boundaries.
use super::RemoteRef;
use crate::{
    threads::{Issue, IssueId, IssueNumber},
    timeline::IssueRef,
};
use rbit_contracts::{AccountId, RepoId, Timestamp};
use rbit_identity_models::{Account, User};
use rbit_repositories::OwnedRepository;
use std::future::Future;
pub trait ReferenceReadPort: Sync {
    type Error;
    fn resolve_batch(
        &self,
        repo: RepoId,
        numbers: &[IssueNumber],
        names: &[String],
    ) -> impl Future<Output = Result<(Vec<IssueRef>, Vec<Account>), Self::Error>> + Send;
}
pub trait RemoteReferencePort: Sync {
    type Error;
    fn find(&self, remote: &RemoteRef) -> impl Future<Output = Result<Option<OwnedRepository>, Self::Error>> + Send;
    fn can_read(&self, user: &User, repo: &OwnedRepository) -> impl Future<Output = Result<bool, Self::Error>> + Send;
}
pub trait CrossReferencePort: Sync {
    type Error;
    type Remote: RemoteReferencePort<Error = Self::Error>;
    fn numbers(&self, repo: RepoId, numbers: &[IssueNumber]) -> impl Future<Output = Result<Vec<IssueRef>, Self::Error>> + Send;
    fn issue(&self, id: IssueId) -> impl Future<Output = Result<Option<Issue>, Self::Error>> + Send;
    fn active_user(&self, id: AccountId) -> impl Future<Output = Result<Option<User>, Self::Error>> + Send;
    fn repositories(&self) -> Self::Remote;
    fn by_number(&self, repo: RepoId, number: IssueNumber) -> impl Future<Output = Result<Option<Issue>, Self::Error>> + Send;
    fn link(&self, target: IssueId, source: IssueId, at: Timestamp) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    fn record(&self, target: IssueId, source: IssueId, actor: Option<AccountId>) -> impl Future<Output = Result<(), Self::Error>> + Send;
}
