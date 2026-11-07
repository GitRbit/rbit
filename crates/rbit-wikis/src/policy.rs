//! Constant-space command checks. Results are data, not persistent write capabilities.
use rbit_access::RepoAccess;
use rbit_contracts::{AccountId, RepoId};

use crate::{WikiGeneration, WikiPageId, WikiRevisionId};

/// Trusted composition supplies CURRENT repository facts, not user-submitted grants.
/// A cached value alone is insufficient for a later write or conditional response.
#[derive(Debug, Clone, Copy)]
pub struct WikiAccess {
    pub repository: RepoId,
    pub actor: Option<AccountId>,
    pub access: RepoAccess,
    pub enabled: bool,
    pub archived: bool,
}

/// One finalized repository-wide catalog/Git head, shared by page read projections.
/// It must advance for every mutation; it is not merely the changed page's content OID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WikiHead {
    pub generation: WikiGeneration,
    pub revision: WikiRevisionId,
}

/// Exact optimistic expectation; None means an uninitialized wiki, never a wildcard.
/// Page creation also requires the target path/alias slot to remain unoccupied.
#[derive(Debug, Clone, Copy)]
pub enum WikiExpected {
    Create { repository: RepoId, head: Option<WikiHead> },
    Change { repository: RepoId, page: WikiPageId, head: WikiHead },
}

/// Authoritative projection of the addressed page/namespace slot at the current wiki
/// head. The SQL adapter must enforce matching facts and uniqueness in its write TX.
#[derive(Debug, Clone, Copy)]
pub struct WikiMutationState {
    pub repository: RepoId,
    pub head: Option<WikiHead>,
    pub page: Option<WikiPageId>,
    pub pending: bool,
}

/// Stable reasons for localized presentation; no page/title/content data is exposed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WikiPolicyError {
    NotFound,
    Forbidden,
    Archived,
    Pending,
    Conflict,
    VersionExhausted,
}

/// Must precede conditional response/cache access. Anonymous public readers are valid.
pub fn check_read(access: &WikiAccess, repository: RepoId) -> Result<(), WikiPolicyError> {
    if access.repository == repository && access.enabled && access.access.can_read() { Ok(()) } else { Err(WikiPolicyError::NotFound) }
}

/// Write access is required for page create/edit/restore/rename/tombstone mutations;
/// administrators do not bypass archive or CAS. Whole-wiki administrative settings
/// remain separate repository-admin commands, not implicit content write authority.
///
/// Order is privacy/read, authenticated writer, archive, scoped state, pending, exact
/// expected head/page, exhaustion. A next generation is PROVISIONAL. Persistence
/// must recheck authoritative ACL/archive state and enforce SQL/Git CAS under the
/// planned publication protocol. Calling this function cannot commit or reserve work.
pub fn check_write(access: &WikiAccess, expected: WikiExpected, current: &WikiMutationState) -> Result<WikiGeneration, WikiPolicyError> {
    check_read(access, access.repository)?;
    if access.actor.is_none() || !access.access.can_write() {
        return Err(WikiPolicyError::Forbidden);
    }
    if access.archived {
        return Err(WikiPolicyError::Archived);
    }
    let expected_repository = match expected {
        WikiExpected::Create { repository, .. } | WikiExpected::Change { repository, .. } => repository,
    };
    if access.repository != current.repository || access.repository != expected_repository {
        return Err(WikiPolicyError::Conflict);
    }
    if current.pending {
        return Err(WikiPolicyError::Pending);
    }
    let matches = match expected {
        WikiExpected::Create { head, .. } => head == current.head && current.page.is_none(),
        WikiExpected::Change { page, head, .. } => current.page == Some(page) && current.head == Some(head),
    };
    if !matches {
        return Err(WikiPolicyError::Conflict);
    }
    match current.head {
        None => Ok(WikiGeneration::INITIAL),
        Some(head) => head.generation.checked_next().ok_or(WikiPolicyError::VersionExhausted),
    }
}
