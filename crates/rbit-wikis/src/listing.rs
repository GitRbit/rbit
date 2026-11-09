//! Content-free page listings at a finalized head; adapters authorize before listing.
use rbit_contracts::Timestamp;

use crate::{WikiGeneration, WikiPageId, WikiPath, WikiTitle};

/// One current page, without content.
#[derive(Debug, Clone)]
pub struct WikiPageEntry {
    pub id: WikiPageId,
    pub path: WikiPath,
    pub title: WikiTitle,
    pub updated_at: Timestamp,
}

/// One keyset page of the current pages, ordered by path bytes.
#[derive(Debug, Clone, Default)]
pub struct WikiPageList {
    pub entries: Vec<WikiPageEntry>,
    /// The last path of this page when more pages follow.
    pub next: Option<WikiPath>,
}

/// A deleted page: its last names and the generation that deleted it. Its history
/// stays readable and the page can be restored under the same stable ID.
#[derive(Debug, Clone)]
pub struct WikiDeletedPage {
    pub id: WikiPageId,
    pub path: WikiPath,
    pub title: WikiTitle,
    pub deleted: WikiGeneration,
    pub deleted_at: Timestamp,
    /// Display name recorded at deletion; not an account reference or ACL fact.
    pub deleted_by: Option<String>,
}

/// One keyset page of deleted pages, most recently deleted first.
#[derive(Debug, Clone, Default)]
pub struct WikiDeletedList {
    pub entries: Vec<WikiDeletedPage>,
    /// The deleting generation of the last entry when more deleted pages follow.
    pub next: Option<WikiGeneration>,
}
