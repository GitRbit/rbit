//! Repository wiki pages for people: authorized reads, history and comparisons.
//!
//! Every read authorizes the CURRENT viewer in SQL before any Git object is opened
//! (the catalog's shared repository read predicate, archived repositories included).
//! A disabled wiki reads as missing. Content comes only from finalized catalog heads.
//! Changes live in [`super::publish`].
use std::{
    num::NonZeroUsize,
    path::{Path, PathBuf},
    sync::Arc,
};

use rbit_core::RepoId;
use rbit_git::{
    diff::{TextChanges, diff_bytes},
    wikis::WikiObjects,
};
use rbit_identity_models::account::User;
use rbit_kernel::BlockingBudget;
use rbit_storage::{
    Database,
    sqlite::{wiki_mutations::WikiMutationStore, wikis::WikiCatalogStore},
};
use rbit_wikis::{
    WikiContent, WikiGeneration, WikiPage, WikiPageId, WikiPath,
    listing::{WikiPageEntry, WikiPageList},
    reading::{
        WikiCatalog, WikiCatalogPage, WikiHistory, WikiHistoryLimit, WikiHistoryRequest, WikiPageRequest, WikiRevisionRequest,
        WikiRevisionStore,
    },
};
use tokio::sync::{Mutex, OnceCell};

use super::errors::WikiError;
use crate::RepoContext;

/// Revisions per history page.
pub const WIKI_HISTORY_PAGE: u8 = 30;
/// Pages per index page; the sidebar shows the first page of the same listing.
pub const WIKI_INDEX_PAGE: u8 = 50;
/// Concurrent Git object reads and preparations across all wikis.
const OBJECT_WORK: NonZeroUsize = NonZeroUsize::new(8).unwrap();

/// One page with its content at a finalized head.
#[derive(Debug)]
pub struct WikiPageView {
    pub page: WikiPage,
    pub content: WikiContent,
}

/// The result of opening a path in an enabled, readable wiki.
#[derive(Debug)]
pub enum WikiLookup {
    Page(WikiPageView),
    /// No current page has this path; `empty` when the wiki has no pages at all.
    Missing {
        empty: bool,
    },
}

/// Two revisions of one page and their line differences.
#[derive(Debug)]
pub struct WikiComparison {
    pub base: WikiPageView,
    pub head: WikiPageView,
    pub changes: TextChanges,
}

/// Whether a repository has its wiki switched on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WikiStatus {
    Off,
    On,
}

/// Wiki pages over the catalog and the dedicated object store. Cheap to clone.
#[derive(Debug, Clone)]
pub struct WikiService {
    pub(super) catalog: WikiCatalogStore,
    pub(super) mutations: WikiMutationStore,
    root: Arc<PathBuf>,
    objects: Arc<OnceCell<WikiObjects>>,
    work: BlockingBudget,
    /// One change at a time per process: preparation and publication of one change
    /// never interleave with another, so a head moved underneath is rare.
    pub(super) writes: Arc<Mutex<()>>,
}

fn viewer_id(viewer: Option<&User>) -> Option<rbit_core::AccountId> {
    viewer.map(User::id)
}

impl WikiService {
    /// `root` is the dedicated object directory (created and bound on first use).
    pub fn new(database: &Database, root: &Path) -> Self {
        let root = std::path::absolute(root).unwrap_or_else(|_| root.to_path_buf());
        Self {
            catalog: database.wikis(),
            mutations: database.wiki_mutations(),
            root: Arc::new(root),
            objects: Arc::new(OnceCell::new()),
            work: BlockingBudget::new(OBJECT_WORK),
            writes: Arc::new(Mutex::new(())),
        }
    }

    /// The object store bound to this catalog, opened once on first use.
    pub(super) async fn objects(&self) -> Result<&WikiObjects, WikiError> {
        self.objects
            .get_or_try_init(|| async {
                let identity = self.catalog.identity().await?;
                Ok(WikiObjects::open(&self.root, identity, self.work.clone())?)
            })
            .await
    }

    /// Whether `repo` has an enabled wiki. The caller already authorized reading `repo`.
    pub async fn status(&self, repo: &RepoContext) -> Result<WikiStatus, WikiError> {
        Ok(if self.catalog.enabled(repo.repo.repo.id).await? == Some(true) { WikiStatus::On } else { WikiStatus::Off })
    }

    pub(super) async fn content(&self, selection: WikiCatalogPage) -> Result<Option<WikiPageView>, WikiError> {
        let Some(page) = selection.page else { return Ok(None) };
        let content = self.objects().await?.read_page(selection.store, &page).await?;
        Ok(Some(WikiPageView { page, content }))
    }

    /// The current page at `path`.
    pub async fn page(&self, repo: &RepoContext, viewer: Option<&User>, path: &WikiPath) -> Result<WikiLookup, WikiError> {
        let request = WikiPageRequest { repository: repo.repo.repo.id, viewer: viewer_id(viewer), path };
        let selection = self.catalog.page(request).await?.ok_or(WikiError::NotFound)?;
        let empty = selection.head.is_none();
        Ok(match self.content(selection).await? {
            Some(view) => WikiLookup::Page(view),
            None => WikiLookup::Missing { empty },
        })
    }

    /// The current page with the stable `id`, or `NotFound`.
    pub async fn page_by_id(&self, repo: &RepoContext, viewer: Option<&User>, id: WikiPageId) -> Result<WikiPageView, WikiError> {
        let selection = self.catalog.page_by_id(repo.repo.repo.id, viewer_id(viewer), id).await?.ok_or(WikiError::NotFound)?;
        self.content(selection).await?.ok_or(WikiError::NotFound)
    }

    /// Current pages by path after `after`, one index page at a time.
    pub async fn pages(&self, repo: &RepoContext, viewer: Option<&User>, after: Option<&WikiPath>) -> Result<WikiPageList, WikiError> {
        self.catalog.pages(repo.repo.repo.id, viewer_id(viewer), after, WIKI_INDEX_PAGE).await?.ok_or(WikiError::NotFound)
    }

    /// Pages whose title or path contains `query` (titles only; content is not indexed).
    pub async fn search(&self, repo: &RepoContext, viewer: Option<&User>, query: &str) -> Result<Vec<WikiPageEntry>, WikiError> {
        self.catalog.search(repo.repo.repo.id, viewer_id(viewer), query, WIKI_INDEX_PAGE).await?.ok_or(WikiError::NotFound)
    }

    /// Revisions of a page, newest first, strictly before `before` when given.
    pub async fn history(
        &self,
        repo: &RepoContext,
        viewer: Option<&User>,
        page: WikiPageId,
        before: Option<WikiGeneration>,
    ) -> Result<WikiHistory, WikiError> {
        self.history_limited(repo.repo.repo.id, viewer, page, before, WIKI_HISTORY_PAGE).await
    }

    async fn history_limited(
        &self,
        repository: RepoId,
        viewer: Option<&User>,
        page: WikiPageId,
        before: Option<WikiGeneration>,
        limit: u8,
    ) -> Result<WikiHistory, WikiError> {
        let limit = WikiHistoryLimit::new(limit).ok_or(WikiError::NotFound)?;
        let request = WikiHistoryRequest { repository, viewer: viewer_id(viewer), page, before, limit };
        self.catalog.history(request).await?.ok_or(WikiError::NotFound)
    }

    /// The page as it was at its revision `generation`.
    pub async fn revision(
        &self,
        repo: &RepoContext,
        viewer: Option<&User>,
        page: WikiPageId,
        generation: WikiGeneration,
    ) -> Result<WikiPageView, WikiError> {
        let request = WikiRevisionRequest { repository: repo.repo.repo.id, viewer: viewer_id(viewer), page, generation };
        let selection = self.catalog.revision(request).await?.ok_or(WikiError::NotFound)?;
        self.content(selection).await?.ok_or(WikiError::NotFound)
    }

    /// The page's revision before `generation`, if any.
    pub async fn previous(
        &self,
        repo: &RepoContext,
        viewer: Option<&User>,
        page: WikiPageId,
        generation: WikiGeneration,
    ) -> Result<Option<WikiGeneration>, WikiError> {
        let history = self.history_limited(repo.repo.repo.id, viewer, page, Some(generation), 1).await?;
        Ok(history.entries.first().map(|entry| entry.head.generation))
    }

    /// Line differences between two revisions of one page (`base` is usually older).
    pub async fn compare(
        &self,
        repo: &RepoContext,
        viewer: Option<&User>,
        page: WikiPageId,
        base: WikiGeneration,
        head: WikiGeneration,
    ) -> Result<WikiComparison, WikiError> {
        let (base, head) = tokio::try_join!(self.revision(repo, viewer, page, base), self.revision(repo, viewer, page, head))?;
        let changes = diff_bytes(base.content.as_str().as_bytes(), head.content.as_str().as_bytes());
        Ok(WikiComparison { base, head, changes })
    }
}
