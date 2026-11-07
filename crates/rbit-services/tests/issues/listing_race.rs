//! Deterministic actual-SQL visibility change between the original count and row statements.
use super::{DashboardRelation, Fixture, IssueKind, IssueQuery};
use rbit_collaboration::{
    issues::{IssueListing, IssueMetadata, IssueReadPort, IssueSearchPort, MetadataNeeds, SearchFilter},
    threads::{Issue, IssueId},
};
use rbit_core::{Account, AccountId, RepoId, User, repository::OwnedRepository};
use rbit_services::issues::IssueReader;
use rbit_storage::{
    StorageError,
    sqlite::{issue_meta::IssueMetaStore, issue_search::IssueSearchStore},
};
use sqlx::Connection as _;
use std::{collections::HashMap, path::Path};
struct View<'a> {
    inner: IssueReader<'a>,
    filename: &'a Path,
}
struct Rows<'a> {
    inner: IssueSearchStore,
    filename: &'a Path,
}
impl IssueSearchPort for Rows<'_> {
    type Error = StorageError;
    async fn state_counts(&self, filter: &SearchFilter) -> Result<(u64, u64), Self::Error> {
        let counts = self.inner.state_counts(filter).await?;
        let options = sqlx::sqlite::SqliteConnectOptions::new().filename(self.filename);
        let mut connection = sqlx::SqliteConnection::connect_with(&options).await?;
        sqlx::query("UPDATE repositories SET visibility='private'").execute(&mut connection).await?;
        connection.close().await?;
        Ok(counts)
    }
    async fn search(&self, filter: &SearchFilter, limit: u32, offset: u64) -> Result<Vec<Issue>, Self::Error> {
        self.inner.search(filter, limit, offset).await
    }
}
impl<'a> IssueReadPort for View<'a> {
    type Error = StorageError;
    type Search = Rows<'a>;
    type Metadata = IssueMetaStore;
    fn search_store(&self) -> Self::Search {
        Rows { inner: self.inner.search_store(), filename: self.filename }
    }
    fn metadata_store(&self) -> Self::Metadata {
        self.inner.metadata_store()
    }
    async fn account_named(&self, name: &str) -> Result<Option<Account>, Self::Error> {
        self.inner.account_named(name).await
    }
    async fn active_user(&self, id: AccountId) -> Result<Option<User>, Self::Error> {
        self.inner.active_user(id).await
    }
    async fn accounts(&self, ids: &[AccountId]) -> Result<Vec<Account>, Self::Error> {
        self.inner.accounts(ids).await
    }
    async fn hydrate(
        &self,
        ids: &[IssueId],
        authors: &[AccountId],
        needs: MetadataNeeds,
    ) -> Result<(Vec<Account>, HashMap<IssueId, IssueMetadata>), Self::Error> {
        self.inner.hydrate(ids, authors, needs).await
    }
    async fn readable_repositories(&self, ids: &[RepoId], viewer: &User) -> Result<HashMap<RepoId, OwnedRepository>, Self::Error> {
        self.inner.readable_repositories(ids, viewer).await
    }
}
#[tokio::test]
async fn visibility_revoked_after_counts_is_filtered_again_by_actual_row_sql() {
    let fixture = Fixture::new().await;
    fixture.create(&fixture.bob, "visible until count", "").await;
    let filename = fixture.directory.path().join("rbit.db");
    let view = View { inner: fixture.services.issues.reader(), filename: &filename };
    let result = IssueListing::new(&view)
        .dashboard(&fixture.bob, IssueKind::Issue, DashboardRelation::Created, &IssueQuery::parse("is:open"), 1)
        .await
        .unwrap();
    // Existing two-statement semantics deliberately allow a stale total; no stale row leaks.
    assert_eq!(result.total, 1);
    assert_eq!(result.issues.len(), 0);
    assert_eq!(result.authors.len(), 0);
    assert_eq!(result.metadata.len(), 0);
    assert_eq!(result.repositories.len(), 0);
}
