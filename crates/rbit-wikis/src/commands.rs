//! Exact create/edit input. IDs and expectations are data, never publish authority.
use std::fmt;

use rbit_contracts::{AccountId, Timestamp};

use crate::{WikiContent, WikiGeneration, WikiId, WikiPageId, WikiPath, WikiRevisionId, WikiStoreId, WikiTitle, policy::WikiExpected};

/// Fixed request identity scoped to one wiki and actor incarnation. Transport must preserve it
/// after acceptance or an uncertain response. It is not a CSRF/authentication token.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct WikiCommandId([u8; 16]);
impl WikiCommandId {
    pub const fn new(bytes: [u8; 16]) -> Option<Self> {
        if u128::from_be_bytes(bytes) == 0 { None } else { Some(Self(bytes)) }
    }
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}
impl fmt::Debug for WikiCommandId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("WikiCommandId(..)")
    }
}

/// Nonreusable wiki-side binding to an account incarnation. Numeric account IDs
/// can be reused after deletion. This data is not authentication evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WikiActorIncarnation(i64);
impl WikiActorIncarnation {
    pub const fn new(value: i64) -> Option<Self> {
        if value > 0 { Some(Self(value)) } else { None }
    }
    pub const fn get(self) -> i64 {
        self.0
    }
}

/// The actor is an authenticated composition fact. HTTP forms never supply actor,
/// native object root, target OID, publication state or a freshened expected head.
pub struct WikiWriteCommand {
    pub id: WikiCommandId,
    pub actor: AccountId,
    pub expected: WikiExpected,
    pub path: WikiPath,
    pub title: WikiTitle,
    pub content: WikiContent,
}
impl fmt::Debug for WikiWriteCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("WikiWriteCommand(..)")
    }
}

/// A committed, bounded recovery record, not a completed mutation or permission grant.
/// Acceptance precedes native preparation; the same exact payload is retained until
/// visible completion. Recovery must recheck current permission/archive/incarnation.
pub struct WikiIntent {
    pub store: WikiStoreId,
    pub actor_incarnation: WikiActorIncarnation,
    pub command: WikiWriteCommand,
    pub page: WikiPageId,
    pub generation: WikiGeneration,
    pub accepted_at: Timestamp,
    pub target: Option<WikiRevisionId>,
}
impl fmt::Debug for WikiIntent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("WikiIntent").field("page", &self.page).field("generation", &self.generation).finish_non_exhaustive()
    }
}

impl WikiWriteCommand {
    pub const fn wiki(&self) -> WikiId {
        match self.expected {
            WikiExpected::Create { wiki, .. } | WikiExpected::Change { wiki, .. } => wiki,
        }
    }
}
