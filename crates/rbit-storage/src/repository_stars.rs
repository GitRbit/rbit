//! Star persistence over the existing star SQL; idempotence stays in the statements.
use crate::{StorageError, sqlite::stars::StarStore};
use rbit_core::{AccountId, RepoId, repository::OwnedRepository};
use rbit_repositories::stars::{StarRepository, StarSummary, Stargazer};
use std::{collections::HashMap, future::Future};

impl StarRepository for StarStore {
    type Error = StorageError;
    async fn summary(&self, repo: RepoId, viewer: Option<AccountId>) -> Result<StarSummary, StorageError> {
        let counted = Self::count(self, repo, viewer).await?;
        Ok(StarSummary { count: counted.count, starred_by_viewer: counted.starred })
    }
    fn counts(&self, repos: &[RepoId]) -> impl Future<Output = Result<HashMap<RepoId, i64>, StorageError>> + Send {
        Self::counts(self, repos)
    }
    fn star(&self, account: AccountId, repo: RepoId) -> impl Future<Output = Result<(), StorageError>> + Send {
        Self::add(self, account, repo)
    }
    fn unstar(&self, account: AccountId, repo: RepoId) -> impl Future<Output = Result<(), StorageError>> + Send {
        Self::remove(self, account, repo)
    }
    fn stargazers(&self, repo: RepoId, limit: u32) -> impl Future<Output = Result<Vec<Stargazer>, StorageError>> + Send {
        Self::stargazers(self, repo, limit)
    }
    fn starred_by(
        &self,
        account: AccountId,
        viewer: Option<AccountId>,
        viewer_is_admin: bool,
        limit: u32,
    ) -> impl Future<Output = Result<Vec<OwnedRepository>, StorageError>> + Send {
        Self::starred_by(self, account, viewer, viewer_is_admin, limit)
    }
}
