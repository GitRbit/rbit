//! Dedicated immutable wiki objects; no code refs, hooks, processes or publisher.
mod exact_object;
mod layout;
mod managed_writer;
mod objects;
mod writer_lock;
pub use managed_writer::{PreparedWikiObjects, WikiObjectWriter};

use rbit_core::{AccountId, RepoId, Timestamp};
use rbit_kernel::{AdmissionError, BlockingBudget};
use rbit_wikis::{WikiContent, WikiId, WikiPage, WikiPageId, WikiRevisionId, WikiStoreId, reading::WikiRevisionStore};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Debug, thiserror::Error)]
pub enum WikiObjectError {
    #[error("wiki catalog and object-root binding do not match")]
    Binding,
    #[error("wiki object format or resource limit refused")]
    Format,
    #[error(transparent)]
    Admission(#[from] AdmissionError),
    #[error("wiki object worker did not complete: {0}")]
    Worker(#[from] tokio::task::JoinError),
    #[error(transparent)]
    Git(#[from] crate::GitError),
}

/// Concrete private object root with an explicitly injected work budget. Clones share
/// admission. Dropped awaiters do not release a queued/running worker's permit early.
/// Composition must close/drain its owner before root teardown; no deadline lies here.
#[derive(Debug, Clone)]
pub struct WikiObjects {
    root: Arc<PathBuf>,
    identity: WikiStoreId,
    budget: BlockingBudget,
}

/// Prepare an immutable tree/commit at a stable numeric page path. This is not an ACL
/// capability, SQL intent, published head, or completed/recoverable wiki mutation.
#[derive(Debug)]
pub struct PrepareWikiPage {
    pub repository: RepoId,
    pub wiki: WikiId,
    pub page: WikiPageId,
    pub parent: Option<WikiRevisionId>,
    pub content: WikiContent,
    pub actor: AccountId,
    pub at: Timestamp,
}

impl WikiObjects {
    /// Startup only: creates a marker-bound root without selecting a wiki ref.
    /// Requires an owned local directory with stable topology. Link checks reject
    /// observed symlinks/reparse points, not hostile concurrent filesystem replacement.
    pub fn open(root: &Path, identity: WikiStoreId, budget: BlockingBudget) -> Result<Self, WikiObjectError> {
        let root = layout::bind_root(root, identity)?;
        Ok(Self { root: Arc::new(root), identity, budget })
    }

    pub async fn prepare(&self, request: PrepareWikiPage) -> Result<WikiRevisionId, WikiObjectError> {
        let reservation = self.budget.try_reserve()?;
        let root = Arc::clone(&self.root);
        let identity = self.identity;
        reservation
            .spawn(move || {
                layout::verify_root(&root, identity)?;
                let _writer = writer_lock::acquire(&root)?;
                objects::prepare(&root, identity, &request)
            })
            .await?
    }
}

impl WikiRevisionStore for WikiObjects {
    type Error = WikiObjectError;
    async fn read_page(&self, identity: WikiStoreId, page: &WikiPage) -> Result<WikiContent, Self::Error> {
        if identity != self.identity {
            return Err(WikiObjectError::Binding);
        }
        let reservation = self.budget.try_reserve()?;
        let root = Arc::clone(&self.root);
        let repository = page.repository;
        let page_id = page.id;
        let wiki = page.wiki;
        let revision = page.head.revision;
        reservation
            .spawn(move || {
                layout::verify_root(&root, identity)?;
                objects::read(&root, identity, repository, wiki, page_id, revision)
            })
            .await?
    }
}
