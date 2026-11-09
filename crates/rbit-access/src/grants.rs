//! Per-viewer grants: which stored grants count for a viewer, how they combine with
//! visibility and ownership, and the port that loads them.
//!
//! Site administrators and the repository's owner have full access whatever they were
//! granted, so their grants are never loaded. Organisation grants count only on
//! organisation repositories, team grants only for members of the owning organisation,
//! and nothing lowers access below the public read baseline. Identifiers and the stored
//! permission type are generic: these rules compare them, never interpret them.

use std::future::Future;

use crate::{OrgRole, OrganizationAccess, RepoAccess, RepositoryAccessFacts, ViewerAccessFacts, repo_access};

/// What one viewer was granted on one repository, as stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RepositoryGrants<P> {
    /// The viewer's role in the owning organisation.
    pub org_role: Option<OrgRole>,
    /// That organisation's base permission (`None` = no permission).
    pub org_base: Option<P>,
    /// The best grant among the viewer's teams for this repository.
    pub team: Option<P>,
    /// The viewer's collaborator permission.
    pub collaborator: Option<P>,
}

impl<P> Default for RepositoryGrants<P> {
    fn default() -> Self {
        Self { org_role: None, org_base: None, team: None, collaborator: None }
    }
}

/// An authenticated viewer; `site_admin` must come from the identity layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Viewer<I> {
    pub id: I,
    pub site_admin: bool,
}

/// The repository facts the decision needs, resolved from one repository record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Repository<I> {
    /// The owning account (a user or an organisation).
    pub owner: I,
    pub public: bool,
    /// The owner is an organisation.
    pub organization: bool,
}

/// Whether `viewer`'s stored grants matter on a repository owned by `owner`: neither a
/// site administrator nor the owner, who have full access whatever they were granted.
pub fn grants_apply<I: PartialEq>(viewer: &Viewer<I>, owner: &I) -> bool {
    !viewer.site_admin && viewer.id != *owner
}

/// What `viewer` (or an anonymous visitor) may do with `repo` given `grants`: the maximum
/// of every rule that applies (see [`repo_access`]).
pub fn access<I, P>(viewer: Option<&Viewer<I>>, repo: &Repository<I>, grants: &RepositoryGrants<P>) -> RepoAccess
where
    I: PartialEq,
    P: Copy + Into<RepoAccess>,
{
    let granted = |permission: Option<P>| permission.map_or(RepoAccess::None, Into::into);
    let viewer = viewer.map(|viewer| ViewerAccessFacts {
        site_admin: viewer.site_admin,
        repository_owner: viewer.id == repo.owner,
        organization: match grants.org_role {
            Some(OrgRole::Owner) => OrganizationAccess::Owner,
            Some(OrgRole::Member) => OrganizationAccess::Member { base: granted(grants.org_base), team: granted(grants.team) },
            None => OrganizationAccess::None,
        },
        collaborator: granted(grants.collaborator),
    });
    repo_access(viewer, RepositoryAccessFacts { public: repo.public, organization: repo.organization })
}

/// Loads one viewer's stored grants on one repository.
pub trait GrantRepository: Sync {
    type Error: Send;
    type Id: Copy + PartialEq + Send + Sync;
    type Repo: Copy + Send + Sync;
    type Permission: Copy + Send;

    fn grants(
        &self,
        repo: Self::Repo,
        owner: Self::Id,
        viewer: Self::Id,
    ) -> impl Future<Output = Result<RepositoryGrants<Self::Permission>, Self::Error>> + Send;
}

/// `viewer`'s grants on `repo` (owned by `owner`): loaded only when they matter, empty
/// for anonymous visitors, site administrators and the owner.
pub async fn viewer_grants<P: GrantRepository>(
    port: &P,
    viewer: Option<Viewer<P::Id>>,
    repo: P::Repo,
    owner: P::Id,
) -> Result<RepositoryGrants<P::Permission>, P::Error> {
    match viewer {
        Some(viewer) if grants_apply(&viewer, &owner) => port.grants(repo, owner, viewer.id).await,
        _ => Ok(RepositoryGrants::default()),
    }
}
