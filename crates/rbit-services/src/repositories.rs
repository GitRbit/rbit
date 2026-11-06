//! Repository lifecycle and permission-checked access.
//!
//! Access follows [`rbit_auth::repo_access`]: this service looks up what the viewer was
//! granted (organisation role and base permission, team grants, collaborator permission)
//! in one query and lets the pure policy decide. Listings filter with the store's shared
//! readable-by predicate, which expresses the same rules in SQL.
//!
//! Renamed and transferred repositories remain reachable under their old names (see
//! migration `0016_repo_redirects.sql`): [`RepositoryService::moved_to`] tells web pages
//! where to redirect, [`RepositoryService::open_following_renames`] serves Git there.

use std::{path::PathBuf, sync::Arc};

use rbit_auth::{Grants, repo_access};
use rbit_core::{Account, AccountId, Config, Permission, RepoAccess, RepoId, User, mirror::Mirror, repository::OwnedRepository};
use rbit_git::{RepositoryReader, RepositoryStorage};
use rbit_storage::{
    RepositoryStore,
    sqlite::{ForkParent, RepositoryPage, collaborators::ViewerGrants},
};

use crate::{EventBus, ServiceContext, ServiceError, StarSummary, timeline::OpenCounts};

/// Upper bound on the dashboard list; it is a jumping-off point, not an archive.
const DASHBOARD_LIMIT: u32 = 100;
pub use rbit_repositories::discovery::{EXPLORE_PAGE_SIZE, RepoSearch, RepoSearchResults};

pub use rbit_repositories::creation::CreateRepository;
mod creation;

/// A repository the viewer is allowed to see, with what they may do to it.
#[derive(Debug, Clone)]
pub struct RepoContext {
    pub repo: OwnedRepository,
    pub access: RepoAccess,
    /// The signed-in user `access` is exactly the access of, so it can be reused for
    /// them within the request. `None` for anonymous viewers and for contexts whose
    /// access is not a user's own (system work, deploy keys, capped LFS tokens).
    pub viewer: Option<AccountId>,
    /// `viewer`'s collaborator permission on the repository, read with their other grants
    /// in this request (`None` without one, and for owners and site administrators, whose
    /// grants are not looked up).
    pub collaborator: Option<Permission>,
    pub reader: RepositoryReader,
    pub disk_path: PathBuf,
}

impl RepoContext {
    /// The archive guard: [`ServiceError::Archived`] when the repository is archived.
    ///
    /// Every write to a repository's content asks this, at the same chokepoints branch
    /// protection uses — the push gate (`ProtectionService::check_push`, for HTTP, SSH and
    /// browser edits), merges, issues, pull requests, comments, reviews, labels,
    /// milestones, releases, commit statuses and LFS uploads and locks — after the
    /// access check, so only people who could otherwise write learn why they may not.
    /// Reading, cloning, forking and starring never ask; neither do the settings that
    /// archive, unarchive, transfer or delete the repository.
    pub fn ensure_writable(&self) -> Result<(), ServiceError> {
        if self.repo.repo.is_archived() { Err(ServiceError::Archived) } else { Ok(()) }
    }

    /// Whether the viewer may change the repository's content: write access to a
    /// repository that is not archived. What pages use to show write controls.
    pub fn can_write_content(&self) -> bool {
        self.access.can_write() && !self.repo.repo.is_archived()
    }

    /// Loads repository `id` for system work (background jobs), or `None` if it no
    /// longer exists. See [`Self::for_system`].
    pub(crate) async fn load_for_system(ctx: &ServiceContext, id: RepoId) -> Result<Option<Self>, ServiceError> {
        let repo = ctx.database.admin().find_repository(id).await?;
        Ok(repo.map(|repo| Self::for_system(repo, &ctx.repositories)))
    }

    /// A read-only context for system work on an already loaded repository.
    ///
    /// There is no viewer, so the context grants exactly [`RepoAccess::Read`]: jobs may
    /// read Git data to warm caches, but anything that checks `access` for writing or
    /// administration refuses. It is crate-private so no request handler can obtain one.
    pub(crate) fn for_system(repo: OwnedRepository, storage: &RepositoryStorage) -> Self {
        let disk_path = storage.path_for(repo.repo.owner_id, repo.repo.id);
        let reader = storage.reader(repo.repo.owner_id, repo.repo.id);
        Self { repo, access: RepoAccess::Read, viewer: None, collaborator: None, reader, disk_path }
    }
}

/// What every repository page shows in its header besides the repository itself, read
/// together with it by [`RepositoryService::open_page`] and shown only once the viewer
/// may read the repository.
#[derive(Debug, Clone)]
pub struct RepoHeaderData {
    pub stars: StarSummary,
    /// Forks the viewer may read.
    pub fork_count: u64,
    /// The repository this one was forked from, when the viewer may read it.
    pub parent: Option<ForkParent>,
    pub mirror: Option<Mirror>,
    pub open_counts: OpenCounts,
}

#[derive(Debug, Clone)]
pub struct RepositoryService {
    config: Arc<Config>,
    store: RepositoryStore,
    storage: RepositoryStorage,
    events: EventBus,
}

impl RepositoryService {
    pub(crate) fn new(config: Arc<Config>, store: RepositoryStore, storage: RepositoryStorage, events: EventBus) -> Self {
        Self { config, store, storage, events }
    }

    /// Resolves and authorises the owner of a new repository (also used for forks).
    pub(crate) async fn owner_for_new_repository(&self, actor: &User, owner: &str) -> Result<Account, ServiceError> {
        authorised_owner(&self.store, actor, owner).await
    }

    /// Loads `owner/name` for `viewer`. Repositories the viewer cannot read are reported
    /// as [`ServiceError::NotFound`] so their existence is not leaked.
    ///
    /// Only the current name matches; web pages answer an old name with a redirect
    /// ([`Self::moved_to`]), the Git transports use [`Self::open_following_renames`].
    pub async fn open(&self, viewer: Option<&User>, owner: &str, name: &str) -> Result<RepoContext, ServiceError> {
        let name = name.strip_suffix(".git").unwrap_or(name);
        let repo = self.store.find_by_path(owner, name).await?.ok_or(ServiceError::NotFound)?;
        self.open_found(viewer, repo).await
    }

    /// Like [`Self::open`], together with the page header's data ([`RepoHeaderData`]) in
    /// the same statement. Access is decided from that row exactly as [`Self::open`]
    /// decides it, before anything is returned: a repository the viewer may not read is
    /// [`ServiceError::NotFound`]. Only a private parent of a fork needs a second lookup,
    /// to check that the viewer may read it too.
    pub async fn open_page(&self, viewer: Option<&User>, owner: &str, name: &str) -> Result<(RepoContext, RepoHeaderData), ServiceError> {
        let name = name.strip_suffix(".git").unwrap_or(name);
        let viewer_is_admin = viewer.is_some_and(|user| user.is_admin);
        let page = self.store.find_page(owner, name, viewer.map(User::id), viewer_is_admin).await?.ok_or(ServiceError::NotFound)?;
        let RepositoryPage { repo, grants, star_count, starred_by_viewer, readable_forks, parent, mirror, open_issues, open_pulls } = page;
        let grants = page_grants(viewer, &repo, grants);
        let ctx = self.context(viewer, repo, grants)?;
        let parent = match parent {
            Some(parent) if !parent.visibility.is_public() => match self.open(viewer, &parent.owner, &parent.name).await {
                Ok(opened) if opened.repo.repo.id == parent.id => Some(parent),
                Ok(_) | Err(ServiceError::NotFound) => None,
                Err(error) => return Err(error),
            },
            parent => parent,
        };
        let header = RepoHeaderData {
            stars: StarSummary { count: i64::try_from(star_count).unwrap_or(i64::MAX), starred_by_viewer },
            fork_count: readable_forks,
            parent,
            mirror,
            open_counts: OpenCounts { issues: open_issues, pulls: open_pulls },
        };
        Ok((ctx, header))
    }

    /// Like [`Self::open`], but `owner/name` may also be a name the repository had before
    /// it was renamed or transferred, so existing clones keep working over Git. A
    /// repository currently called `owner/name` always wins over an old name.
    pub async fn open_following_renames(&self, viewer: Option<&User>, owner: &str, name: &str) -> Result<RepoContext, ServiceError> {
        let name = name.strip_suffix(".git").unwrap_or(name);
        let repo = match self.store.find_by_path(owner, name).await? {
            Some(repo) => repo,
            None => self.store.find_by_old_path(owner, name).await?.ok_or(ServiceError::NotFound)?,
        };
        self.open_found(viewer, repo).await
    }

    /// Where the repository formerly called `owner/name` lives now, if `viewer` may read
    /// it. `None` for names that were never moved away from, are taken again, or belong to
    /// a repository the viewer cannot see (so its new name does not leak).
    pub async fn moved_to(&self, viewer: Option<&User>, owner: &str, name: &str) -> Result<Option<OwnedRepository>, ServiceError> {
        let Some(repo) = self.store.find_by_old_path(owner, name).await? else {
            return Ok(None);
        };
        let grants = viewer_grants(&self.store, viewer, &repo).await?;
        Ok(repo_access(viewer, &repo, &grants).can_read().then_some(repo))
    }

    async fn open_found(&self, viewer: Option<&User>, repo: OwnedRepository) -> Result<RepoContext, ServiceError> {
        let grants = viewer_grants(&self.store, viewer, &repo).await?;
        self.context(viewer, repo, grants)
    }

    /// The context of `repo` for `viewer` with `grants`, or [`ServiceError::NotFound`]
    /// when they may not read it.
    fn context(&self, viewer: Option<&User>, repo: OwnedRepository, grants: Grants) -> Result<RepoContext, ServiceError> {
        let access = repo_access(viewer, &repo, &grants);
        if !access.can_read() {
            return Err(ServiceError::NotFound);
        }
        let disk_path = self.storage.path_for(repo.repo.owner_id, repo.repo.id);
        let reader = self.storage.reader(repo.repo.owner_id, repo.repo.id);
        Ok(RepoContext { repo, access, viewer: viewer.map(User::id), collaborator: grants.collaborator, reader, disk_path })
    }

    /// Repositories of `owner` that `viewer` may see.
    pub async fn list_for_owner(&self, viewer: Option<&User>, owner: &Account) -> Result<Vec<OwnedRepository>, ServiceError> {
        let is_admin = viewer.is_some_and(|user| user.is_admin);
        Ok(self.store.list_by_owner(owner.id, viewer.map(User::id), is_admin).await?)
    }

    /// The dashboard list: `user`'s own repositories, those of their organisations and
    /// those shared with them (each only if they may read it), most recently updated first.
    pub async fn list_for_dashboard(&self, user: &User) -> Result<Vec<OwnedRepository>, ServiceError> {
        Ok(self.store.list_for_member(user.id(), DASHBOARD_LIMIT).await?)
    }

    /// One page (`limit` repositories after skipping `offset`) of the same list as
    /// [`Self::list_for_dashboard`], uncapped, with the total across all pages.
    pub async fn page_for_member(&self, user: &User, limit: u32, offset: u32) -> Result<(Vec<OwnedRepository>, u64), ServiceError> {
        let total = self.store.count_for_member(user.id()).await?;
        Ok((self.store.page_for_member(user.id(), limit, offset).await?, total))
    }

    pub async fn recent_public(&self, limit: u32) -> Result<Vec<OwnedRepository>, ServiceError> {
        Ok(self.store.list_recent_public(limit).await?)
    }

    /// One page of public repositories matching `search` (name or description).
    /// Pages past the end are clamped to the last page.
    pub async fn explore(&self, search: &RepoSearch) -> Result<RepoSearchResults, ServiceError> {
        Ok(rbit_repositories::discovery::RepositoryDiscovery::new(&self.store).explore(search).await?)
    }

    /// The largest file size rendered inline in the browser.
    pub fn max_render_bytes(&self) -> usize {
        self.config.repository.max_render_bytes
    }
}

/// The account called `owner` if `actor` may put repositories there: themselves (also when
/// `owner` is empty), or an organisation they belong to (site administrators: any
/// organisation). Shared by repository creation, forks and transfers.
pub(crate) async fn authorised_owner(store: &RepositoryStore, actor: &User, owner: &str) -> Result<Account, ServiceError> {
    rbit_repositories::creation::RepositoryOwnership::resolve(store, actor, owner).await.map_err(creation::owner_error)
}

/// What `viewer` was granted on `repo`, for [`repo_access`]. Skips the query when the
/// answer cannot change the outcome (anonymous viewers, site admins, the personal owner).
pub(crate) async fn viewer_grants(store: &RepositoryStore, viewer: Option<&User>, repo: &OwnedRepository) -> Result<Grants, ServiceError> {
    let Some(user) = grant_holder(viewer, repo) else {
        return Ok(Grants::default());
    };
    Ok(policy_grants(store.grants(repo.repo.id, repo.repo.owner_id, user.id()).await?))
}

/// The viewer whose grants on `repo` matter: signed in, neither a site administrator nor
/// the owner (who have full access whatever they were granted).
fn grant_holder<'u>(viewer: Option<&'u User>, repo: &OwnedRepository) -> Option<&'u User> {
    viewer.filter(|user| !user.is_admin && user.id() != repo.repo.owner_id)
}

/// `stored` grants of `viewer` read together with `repo` (already for this viewer),
/// as [`viewer_grants`] would have looked them up.
fn page_grants(viewer: Option<&User>, repo: &OwnedRepository, stored: ViewerGrants) -> Grants {
    grant_holder(viewer, repo).map_or_else(Grants::default, |_| policy_grants(stored))
}

fn policy_grants(stored: ViewerGrants) -> Grants {
    Grants { org_role: stored.org_role, org_base: stored.org_base.permission(), team: stored.team, collaborator: stored.collaborator }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn results(total: u64, page: u32) -> RepoSearchResults {
        RepoSearchResults { repos: Vec::new(), total, page, per_page: 30 }
    }

    #[test]
    fn page_arithmetic() {
        assert_eq!(results(0, 1).page_count(), 1);
        assert_eq!(results(30, 1).page_count(), 1);
        assert_eq!(results(31, 1).page_count(), 2);
        assert!(!results(31, 1).has_previous() && results(31, 1).has_next());
        assert!(results(31, 2).has_previous() && !results(31, 2).has_next());
    }
}
