//! Borrowed read composition retains neither the service context nor background controllers.
use super::{DashboardRelation, IssueMetadata, IssueSearchResults, IssueService, MetadataNeeds, SearchFilter};
use crate::{RepoContext, ServiceError};
use rbit_collaboration::issues::{IssueListing, IssueMetadataReader, IssueReadAccess, IssueReadError, IssueReadPort};
use rbit_core::{Account, AccountId, IssueId, IssueKind, RepoId, User, issue_query::IssueQuery, repository::OwnedRepository};
use rbit_storage::{
    Database, StorageError,
    sqlite::{issue_meta::IssueMetaStore, issue_search::IssueSearchStore},
};
use std::collections::HashMap;
#[derive(Debug, Clone, Copy)]
pub struct IssueReader<'a> {
    database: &'a Database,
}
fn error(error: IssueReadError<StorageError>) -> ServiceError {
    match error {
        IssueReadError::NotFound => ServiceError::NotFound,
        IssueReadError::Port(error) => error.into(),
    }
}
fn access(repo: &RepoContext) -> IssueReadAccess {
    IssueReadAccess { repo: repo.repo.repo.id, can_read: repo.access.can_read() }
}
impl IssueReadPort for IssueReader<'_> {
    type Error = StorageError;
    type Search = IssueSearchStore;
    type Metadata = IssueMetaStore;
    fn search_store(&self) -> Self::Search {
        self.database.issue_search()
    }
    fn metadata_store(&self) -> Self::Metadata {
        self.database.issue_meta()
    }
    async fn account_named(&self, name: &str) -> Result<Option<Account>, Self::Error> {
        self.database.accounts().find_account_by_name(name).await
    }
    async fn active_user(&self, id: AccountId) -> Result<Option<User>, Self::Error> {
        self.database.accounts().find_active_user(id).await
    }
    async fn accounts(&self, ids: &[AccountId]) -> Result<Vec<Account>, Self::Error> {
        self.database.accounts().find_accounts(ids).await
    }
    async fn hydrate(
        &self,
        issues: &[IssueId],
        authors: &[AccountId],
        needs: MetadataNeeds,
    ) -> Result<(Vec<Account>, HashMap<IssueId, IssueMetadata>), Self::Error> {
        tokio::try_join!(self.accounts(authors), async { IssueMetadataReader::new(&self.metadata_store()).read_for(issues, needs).await })
    }
    async fn readable_repositories(&self, ids: &[RepoId], viewer: &User) -> Result<HashMap<RepoId, OwnedRepository>, Self::Error> {
        self.database.repositories().readable_among(ids, viewer.id(), viewer.is_admin).await
    }
}
impl IssueReader<'_> {
    /// Every supplied ID receives metadata; single-thread reads use one joined statement.
    pub async fn metadata(&self, issues: &[IssueId]) -> Result<HashMap<IssueId, IssueMetadata>, ServiceError> {
        Ok(IssueMetadataReader::new(&self.metadata_store()).read(issues).await?)
    }
    /// HTML pagination clamps to the last matching page.
    pub async fn search(
        &self,
        viewer: Option<&User>,
        repo: &RepoContext,
        query: &IssueQuery,
        page: u32,
    ) -> Result<IssueSearchResults, ServiceError> {
        IssueListing::new(self).search(viewer, access(repo), query, page).await.map_err(error)
    }
    /// Read only metadata displayed by this page.
    pub async fn search_with(
        &self,
        viewer: Option<&User>,
        repo: &RepoContext,
        query: &IssueQuery,
        page: u32,
        needs: MetadataNeeds,
    ) -> Result<IssueSearchResults, ServiceError> {
        IssueListing::new(self).search_with(viewer, access(repo), query, page, needs).await.map_err(error)
    }
    /// REST pagination preserves an out-of-range requested page.
    pub async fn list(
        &self,
        repo: &RepoContext,
        filter: SearchFilter,
        page: u32,
        per_page: u32,
    ) -> Result<IssueSearchResults, ServiceError> {
        IssueListing::new(self).list(access(repo), filter, page, per_page).await.map_err(error)
    }
    /// Active-user authority is reloaded before filter resolution; repository visibility is batched after hydration.
    pub async fn dashboard(
        &self,
        viewer: &User,
        kind: IssueKind,
        relation: DashboardRelation,
        query: &IssueQuery,
        page: u32,
    ) -> Result<IssueSearchResults, ServiceError> {
        IssueListing::new(self).dashboard(viewer, kind, relation, query, page).await.map_err(error)
    }
}
// Compatibility methods preserve public Rust entry points while the concrete reader owns new reads.
impl IssueService {
    pub fn reader(&self) -> IssueReader<'_> {
        IssueReader { database: &self.ctx.database }
    }
    pub async fn metadata(&self, issues: &[IssueId]) -> Result<HashMap<IssueId, IssueMetadata>, ServiceError> {
        self.reader().metadata(issues).await
    }
    pub async fn search(
        &self,
        viewer: Option<&User>,
        repo: &RepoContext,
        query: &IssueQuery,
        page: u32,
    ) -> Result<IssueSearchResults, ServiceError> {
        self.reader().search(viewer, repo, query, page).await
    }
    pub async fn search_with(
        &self,
        viewer: Option<&User>,
        repo: &RepoContext,
        query: &IssueQuery,
        page: u32,
        needs: MetadataNeeds,
    ) -> Result<IssueSearchResults, ServiceError> {
        self.reader().search_with(viewer, repo, query, page, needs).await
    }
    pub async fn list(
        &self,
        repo: &RepoContext,
        filter: SearchFilter,
        page: u32,
        per_page: u32,
    ) -> Result<IssueSearchResults, ServiceError> {
        self.reader().list(repo, filter, page, per_page).await
    }
    pub async fn dashboard(
        &self,
        viewer: &User,
        kind: IssueKind,
        relation: DashboardRelation,
        query: &IssueQuery,
        page: u32,
    ) -> Result<IssueSearchResults, ServiceError> {
        self.reader().dashboard(viewer, kind, relation, query, page).await
    }
}
