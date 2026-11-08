//! Content-free page listings at a finalized head; adapters authorize before listing.
use rbit_contracts::Timestamp;

use crate::{WikiPageId, WikiPath, WikiTitle};

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
