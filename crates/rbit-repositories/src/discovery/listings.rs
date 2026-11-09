//! Account, dashboard and recent listings, and the explore query's lenient parsing.
//!
//! Rules:
//! * Every listing for a viewer is filtered by the storage visibility predicate with
//!   the viewer's id and site-administrator flag; nothing is filtered after loading.
//! * The dashboard shows at most [`DASHBOARD_LIMIT`] repositories: the member's own,
//!   their organizations' and those shared with them, most recently updated first. Its
//!   paged form is uncapped and counts before it loads, as two separate statements.
//! * Recent public repositories are public only, newest activity first.
//! * Explore query values are lenient: unknown sort or archive values fall back to
//!   their defaults, an unparsable page is page 1, and search text is trimmed.

use std::future::Future;

use rbit_contracts::AccountId;
use rbit_identity_models::{Account, User};

use super::page::RepoSearch;
use crate::{ArchivedFilter, OwnedRepository, RepoSort};

/// Upper bound on the dashboard list; it is a jumping-off point, not an archive.
pub const DASHBOARD_LIMIT: u32 = 100;

/// The existing visibility-filtered listing statements.
pub trait RepositoryListingCatalog: Sync {
    type Error: Send;
    fn owned_by(
        &self,
        owner: AccountId,
        viewer: Option<AccountId>,
        viewer_is_admin: bool,
    ) -> impl Future<Output = Result<Vec<OwnedRepository>, Self::Error>> + Send;
    fn for_member(&self, member: AccountId, limit: u32) -> impl Future<Output = Result<Vec<OwnedRepository>, Self::Error>> + Send;
    fn count_for_member(&self, member: AccountId) -> impl Future<Output = Result<u64, Self::Error>> + Send;
    fn page_for_member(
        &self,
        member: AccountId,
        limit: u32,
        offset: u32,
    ) -> impl Future<Output = Result<Vec<OwnedRepository>, Self::Error>> + Send;
    fn recent_public(&self, limit: u32) -> impl Future<Output = Result<Vec<OwnedRepository>, Self::Error>> + Send;
}

/// Listing queries over one borrowed catalog.
#[derive(Debug)]
pub struct RepositoryListings<'a, S> {
    catalog: &'a S,
}

impl<'a, S: RepositoryListingCatalog> RepositoryListings<'a, S> {
    pub fn new(catalog: &'a S) -> Self {
        Self { catalog }
    }

    /// Repositories of `owner` that `viewer` may see.
    pub async fn for_owner(&self, viewer: Option<&User>, owner: &Account) -> Result<Vec<OwnedRepository>, S::Error> {
        let is_admin = viewer.is_some_and(|user| user.is_admin);
        self.catalog.owned_by(owner.id, viewer.map(User::id), is_admin).await
    }

    /// Every repository of `owner`, whoever may read it, for owner-level management.
    pub async fn all_for_owner(&self, owner: AccountId) -> Result<Vec<OwnedRepository>, S::Error> {
        self.catalog.owned_by(owner, None, true).await
    }

    pub async fn dashboard(&self, user: &User) -> Result<Vec<OwnedRepository>, S::Error> {
        self.catalog.for_member(user.id(), DASHBOARD_LIMIT).await
    }

    /// One page of the dashboard list, uncapped, with the total across all pages.
    pub async fn member_page(&self, user: &User, limit: u32, offset: u32) -> Result<(Vec<OwnedRepository>, u64), S::Error> {
        let total = self.catalog.count_for_member(user.id()).await?;
        Ok((self.catalog.page_for_member(user.id(), limit, offset).await?, total))
    }

    pub async fn recent_public(&self, limit: u32) -> Result<Vec<OwnedRepository>, S::Error> {
        self.catalog.recent_public(limit).await
    }
}

impl RepoSearch {
    /// The explore query as requested, with lenient defaults for bad values.
    pub fn from_query(text: Option<&str>, sort: Option<&str>, archived: Option<&str>, page: Option<&str>) -> Self {
        Self {
            text: text.unwrap_or_default().trim().to_owned(),
            sort: sort.and_then(RepoSort::parse).unwrap_or_default(),
            archived: archived.and_then(ArchivedFilter::parse).unwrap_or_default(),
            page: page.and_then(|page| page.parse().ok()).unwrap_or(1),
        }
    }
}
