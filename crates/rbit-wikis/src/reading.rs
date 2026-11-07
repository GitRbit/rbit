//! Real read-side ports; no publication or caller-asserted write authorization API.
mod application;

use std::future::Future;

use rbit_contracts::{AccountId, RepoId, Timestamp};

use crate::{WikiContent, WikiGeneration, WikiId, WikiPage, WikiPageId, WikiPath, WikiStoreId, policy::WikiHead};

pub use application::{WikiReadError, WikiReadView, WikiReader};

#[derive(Debug, Clone, Copy)]
pub struct WikiPageRequest<'a> {
    pub repository: RepoId,
    pub viewer: Option<AccountId>,
    pub path: &'a WikiPath,
}

#[derive(Debug, Clone, Copy)]
pub struct WikiRevisionRequest {
    pub repository: RepoId,
    pub viewer: Option<AccountId>,
    pub page: WikiPageId,
    pub generation: WikiGeneration,
}

/// None head means an authorized, enabled, genuinely uninitialized wiki. None page
/// with a nonempty head is a missing target, never permission to read raw Git HEAD.
#[derive(Debug)]
pub struct WikiCatalogPage {
    pub wiki: WikiId,
    pub repository: RepoId,
    pub store: WikiStoreId,
    pub head: Option<WikiHead>,
    pub page: Option<WikiPage>,
}

#[derive(Debug, Clone, Copy)]
pub struct WikiHistoryRequest {
    pub repository: RepoId,
    pub viewer: Option<AccountId>,
    pub page: WikiPageId,
    pub before: Option<WikiGeneration>,
    pub limit: WikiHistoryLimit,
}

#[derive(Debug, Clone, Copy)]
pub struct WikiHistoryLimit(u8);
impl WikiHistoryLimit {
    pub const MAX: u8 = 50;
    pub const fn new(value: u8) -> Option<Self> {
        if value > 0 && value <= Self::MAX { Some(Self(value)) } else { None }
    }
    pub const fn get(self) -> u8 {
        self.0
    }
}

#[derive(Debug)]
pub struct WikiHistoryEntry {
    pub head: WikiHead,
    pub title: crate::WikiTitle,
    pub path: WikiPath,
    pub at: Timestamp,
}

#[derive(Debug)]
pub struct WikiHistory {
    pub entries: Vec<WikiHistoryEntry>,
    pub next: Option<WikiGeneration>,
}

/// Some results MUST follow a CURRENT active-viewer/repository ACL check in the same
/// read snapshot. NotFound/disabled/denied produce None before page/history fetching.
/// Viewer ID is an authenticated composition fact; no caller-provided admin flag.
pub trait WikiCatalog: Sync {
    type Error: Send;
    fn page(&self, request: WikiPageRequest<'_>) -> impl Future<Output = Result<Option<WikiCatalogPage>, Self::Error>> + Send;
    fn revision(&self, request: WikiRevisionRequest) -> impl Future<Output = Result<Option<WikiCatalogPage>, Self::Error>> + Send;
    fn history(&self, request: WikiHistoryRequest) -> impl Future<Output = Result<Option<WikiHistory>, Self::Error>> + Send;
}

/// Immutable finalized selection only. The implementation binds store/repository and
/// loads exactly that commit/page, with bounded object headers and content; no refs.
pub trait WikiRevisionStore: Sync {
    type Error: Send;
    fn read_page(&self, store: WikiStoreId, page: &WikiPage) -> impl Future<Output = Result<WikiContent, Self::Error>> + Send;
}
