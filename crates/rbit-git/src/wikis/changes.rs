//! Requests to prepare an unpublished whole-wiki commit that changes one page.
use rbit_core::{AccountId, RepoId, Timestamp};
use rbit_wikis::{WikiContent, WikiId, WikiPageId, WikiRevisionId};

/// The commit's effect on the page's blob in the flat stable-ID tree.
#[derive(Debug)]
pub enum WikiTreeChange {
    /// Create or replace the page's content.
    Upsert(WikiContent),
    /// Drop the page from the snapshot (it must exist in the parent).
    Remove,
    /// Same tree as the parent: a metadata-only change such as a rename.
    Keep,
}

/// Like [`super::PrepareWikiPage`], for any [`WikiTreeChange`]. Preparing objects is
/// not publication, authorization or a durable receipt.
#[derive(Debug)]
pub struct PrepareWikiChange {
    pub repository: RepoId,
    pub wiki: WikiId,
    pub page: WikiPageId,
    pub parent: Option<WikiRevisionId>,
    pub change: WikiTreeChange,
    pub actor: AccountId,
    pub at: Timestamp,
}
