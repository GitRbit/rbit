//! Filtered thread reads own pagination, ordered policy and selected-page hydration.
use super::{
    DashboardRelation, DashboardScope, ISSUES_PER_PAGE, IssueMetadataReader, IssueReadAccess, IssueReadError, IssueReadPort,
    IssueSearchPort, IssueSearchResults, MetadataNeeds, SearchFilter, query::IssueQuery,
};
use crate::{
    IssueState,
    threads::{IssueId, IssueKind},
};
use rbit_contracts::{AccountId, RepoId};
use rbit_identity_models::User;
use std::collections::{BTreeSet, HashMap};
/// Distinct items in their first order, retaining the original temporary vector and set.
fn distinct<T: Ord + Copy>(items: &[T]) -> Vec<T> {
    let mut seen = BTreeSet::new();
    items.iter().copied().filter(|item| seen.insert(*item)).collect()
}
#[derive(Debug)]
pub struct IssueListing<'a, P> {
    pub(super) port: &'a P,
}
impl<'a, P: IssueReadPort> IssueListing<'a, P> {
    pub fn new(port: &'a P) -> Self {
        Self { port }
    }
    /// One page (1-based; clamped to the last) of `repo`'s threads matching `query` for
    /// `viewer`, with tab counts, authors and metadata.
    pub async fn search(
        &self,
        viewer: Option<&User>,
        repo: IssueReadAccess,
        query: &IssueQuery,
        page: u32,
    ) -> Result<IssueSearchResults, IssueReadError<P::Error>> {
        self.search_with(viewer, repo, query, page, MetadataNeeds::All).await
    }

    /// [`Self::search`], reading only the metadata the page shows (`needs`); the rest of
    /// each entry of [`IssueSearchResults::metadata`] stays empty.
    pub async fn search_with(
        &self,
        viewer: Option<&User>,
        repo: IssueReadAccess,
        query: &IssueQuery,
        page: u32,
        needs: MetadataNeeds,
    ) -> Result<IssueSearchResults, IssueReadError<P::Error>> {
        if !repo.can_read {
            return Err(IssueReadError::NotFound);
        }
        let (filter, hints) = self.search_filter(viewer, repo.repo, query).await?;
        self.search_results(&filter, hints, page, needs).await
    }

    /// Personal threads across every repository the current active viewer can read.
    pub async fn dashboard(
        &self,
        viewer: &User,
        kind: IssueKind,
        relation: DashboardRelation,
        query: &IssueQuery,
        page: u32,
    ) -> Result<IssueSearchResults, IssueReadError<P::Error>> {
        let viewer = self.port.active_user(viewer.id()).await.map_err(IssueReadError::Port)?.ok_or(IssueReadError::NotFound)?;
        let (mut filter, hints) = self.search_filter(Some(&viewer), RepoId(0), query).await?;
        filter.kind = Some(kind);
        filter.dashboard = Some(DashboardScope { viewer: viewer.id(), is_admin: viewer.is_admin, relation });
        let mut results = self.search_results(&filter, hints, page, MetadataNeeds::All).await?;
        // Each row's repository, access rechecked for the viewer just loaded: one batch
        // over the page's distinct repositories.
        let ids: Vec<RepoId> = distinct(&results.issues.iter().map(|issue| issue.repo_id).collect::<Vec<_>>());
        results.repositories = self.port.readable_repositories(&ids, &viewer).await.map_err(IssueReadError::Port)?;
        Ok(results)
    }

    async fn search_results(
        &self,
        filter: &SearchFilter,
        hints: Vec<String>,
        page: u32,
        needs: MetadataNeeds,
    ) -> Result<IssueSearchResults, IssueReadError<P::Error>> {
        let store = self.port.search_store();
        let (open, closed) = store.state_counts(filter).await.map_err(IssueReadError::Port)?;
        let total = match filter.state {
            Some(IssueState::Open) => open,
            Some(IssueState::Closed) => closed,
            None => open + closed,
        };
        let mut results = IssueSearchResults {
            issues: Vec::new(),
            authors: HashMap::new(),
            metadata: HashMap::new(),
            total,
            open,
            closed,
            page: page.max(1),
            per_page: ISSUES_PER_PAGE,
            hints,
            repositories: HashMap::new(),
        };
        results.page = results.page.min(results.page_count());
        let offset = u64::from(results.page - 1) * u64::from(ISSUES_PER_PAGE);
        results.issues = store.search(filter, ISSUES_PER_PAGE, offset).await.map_err(IssueReadError::Port)?;
        let ids: Vec<IssueId> = results.issues.iter().map(|issue| issue.id).collect();
        let author_ids: Vec<AccountId> = distinct(&results.issues.iter().filter_map(|issue| issue.author).collect::<Vec<_>>());
        let (authors, metadata) = self.port.hydrate(&ids, &author_ids, needs).await.map_err(IssueReadError::Port)?;
        results.authors = authors.into_iter().map(|account| (account.id, account)).collect();
        results.metadata = metadata;
        Ok(results)
    }

    /// One page (1-based, not clamped) of `per_page` threads of `repo` matching `filter`
    /// (its `repo` is replaced by `repo`'s), with authors and metadata: the REST API's list.
    pub async fn list(
        &self,
        repo: IssueReadAccess,
        mut filter: SearchFilter,
        page: u32,
        per_page: u32,
    ) -> Result<IssueSearchResults, IssueReadError<P::Error>> {
        if !repo.can_read {
            return Err(IssueReadError::NotFound);
        }
        filter.repo = repo.repo;
        filter.dashboard = None;
        let store = self.port.search_store();
        let (open, closed) = store.state_counts(&filter).await.map_err(IssueReadError::Port)?;
        let total = match filter.state {
            Some(IssueState::Open) => open,
            Some(IssueState::Closed) => closed,
            None => open + closed,
        };
        let per_page = per_page.max(1);
        let offset = u64::from(page.max(1) - 1) * u64::from(per_page);
        let issues = store.search(&filter, per_page, offset).await.map_err(IssueReadError::Port)?;
        let ids: Vec<IssueId> = issues.iter().map(|issue| issue.id).collect();
        let author_ids: Vec<AccountId> = distinct(&issues.iter().filter_map(|issue| issue.author).collect::<Vec<_>>());
        let authors = self.port.accounts(&author_ids).await.map_err(IssueReadError::Port)?;
        Ok(IssueSearchResults {
            metadata: IssueMetadataReader::new(&self.port.metadata_store()).read(&ids).await.map_err(IssueReadError::Port)?,
            authors: authors.into_iter().map(|account| (account.id, account)).collect(),
            issues,
            total,
            open,
            closed,
            page: page.max(1),
            per_page,
            hints: Vec::new(),
            repositories: HashMap::new(),
        })
    }
}
