//! Repository visibility, ownership and grants combine by maximum access.

use crate::RepoAccess;

/// Facts about the repository being authorized. Resolve both from the same repository.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RepositoryAccessFacts {
    /// Public repositories provide a read baseline, even for anonymous visitors.
    pub public: bool,
    /// Organisation membership grants apply only to organisation-owned repositories.
    pub organization: bool,
}

/// Membership and grants resolved for this actor in the repository's owning organisation.
///
/// Member grants cannot be supplied without the membership variant. Stale team/base
/// rows belonging to a nonmember must be discarded when constructing these facts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OrganizationAccess {
    #[default]
    None,
    /// Current membership in the owning organisation, with its applicable grants.
    Member {
        /// The owning organisation's base permission for members.
        base: RepoAccess,
        /// The best grant from this member's teams on this repository.
        team: RepoAccess,
    },
    /// Current ownership of the repository's owning organisation.
    Owner,
}

/// Facts about an already authenticated viewer, scoped to this repository.
///
/// These are trusted inputs, not a credential or a capability issued by this crate.
/// `repository_owner` means the authenticated account ID equals the owner ID; the
/// adapter must compare IDs, never names supplied in a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ViewerAccessFacts {
    /// Site-wide administrator privilege established by the identity layer.
    pub site_admin: bool,
    /// The authenticated account ID matches the repository owner account ID.
    pub repository_owner: bool,
    /// Membership facts for the repository's owning organisation.
    pub organization: OrganizationAccess,
    /// Direct collaborator permission on this repository, or `RepoAccess::None`.
    pub collaborator: RepoAccess,
}

/// Effective access is the maximum of visibility, ownership and applicable grants.
///
/// `None` is anonymous and ignores all identity-dependent grants. Personal repositories
/// ignore organisation facts. Site admins and account owners retain administrative
/// access, while outside collaborators can access organisation repositories without
/// membership. No grant can lower the public read baseline.
///
/// This decision does not enforce archived content, branch protection, token scopes,
/// suspended accounts or operation-specific rules; callers retain those separate gates.
pub fn repo_access(viewer: Option<ViewerAccessFacts>, repo: RepositoryAccessFacts) -> RepoAccess {
    let baseline = if repo.public { RepoAccess::Read } else { RepoAccess::None };
    let Some(viewer) = viewer else {
        return baseline;
    };
    if viewer.site_admin || viewer.repository_owner {
        return RepoAccess::Admin;
    }
    let organization = match viewer.organization {
        _ if !repo.organization => RepoAccess::None,
        OrganizationAccess::None => RepoAccess::None,
        OrganizationAccess::Member { base, team } => base.max(team),
        OrganizationAccess::Owner => RepoAccess::Admin,
    };
    baseline.max(organization).max(viewer.collaborator)
}
