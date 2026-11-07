//! A finalized page projection, not an ACL capability or persistence row decoder.
use rbit_contracts::{RepoId, Timestamp};

use crate::{
    WikiId, WikiPageId, WikiPath, WikiTitle,
    policy::{WikiExpected, WikiHead},
};

/// Stable page identity survives renaming. `head` is the visible WHOLE-WIKI snapshot
/// used for this projection, not an independently mutable per-page Git ref.
#[derive(Debug, Clone)]
pub struct WikiPage {
    pub repository: RepoId,
    pub wiki: WikiId,
    pub id: WikiPageId,
    pub path: WikiPath,
    pub title: WikiTitle,
    pub head: WikiHead,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl WikiPage {
    /// Encode the read-time edit form's exact identity/head. A submitted command must
    /// preserve the client's original expectation, never regenerate it from a fresh
    /// page read to make stale input appear current.
    pub const fn expected_change(&self) -> WikiExpected {
        WikiExpected::Change { repository: self.repository, wiki: self.wiki, page: self.id, head: self.head }
    }
}
