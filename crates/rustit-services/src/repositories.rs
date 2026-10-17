//! Repository lifecycle and permission-checked access.
//!
//! Access follows [`rustit_auth::repo_access`]: this service looks up what the viewer was
//! granted (organisation role and base permission, team grants, collaborator permission)
//! in one query and lets the pure policy decide. Listings filter with the store's shared
//! readable-by predicate, which expresses the same rules in SQL.
//!
//! Renamed and transferred repositories remain reachable under their old names (see
//! migration `0016_repo_redirects.sql`): [`RepositoryService::moved_to`] tells web pages
//! where to redirect, [`RepositoryService::open_following_renames`] serves Git there.

use std::{path::PathBuf, sync::Arc};

use rustit_auth::{Grants, repo_access};
use rustit_core::{
    Account, Config, OrgRole, Permission, RepoAccess, RepoId, RepoName, RepoSort, User, Visibility, repository::OwnedRepository,
};
use rustit_git::{RepositoryReader, RepositoryStorage};
use rustit_storage::{NewRepository, RepositoryStore};

use crate::{DomainEvent, EventBus, ServiceContext, ServiceError};

const MAX_DESCRIPTION_LEN: usize = 350;
/// Upper bound on the dashboard list; it is a jumping-off point, not an archive.
const DASHBOARD_LIMIT: u32 = 100;
/// Explore results per page.
pub const EXPLORE_PAGE_SIZE: u32 = 30;
/// Longer search text is truncated; nothing useful is that long.
const MAX_SEARCH_LEN: usize = 100;

/// Raw "new repository" input, exactly as submitted.
#[derive(Debug, Clone, Default)]
pub struct CreateRepository {
    pub name: String,
    pub description: String,
    pub visibility: Visibility,
}

/// A repository the viewer is allowed to see, with what they may do to it.
#[derive(Debug, Clone)]
pub struct RepoContext {
    pub repo: OwnedRepository,
    pub access: RepoAccess,
    pub reader: RepositoryReader,
    pub disk_path: PathBuf,
}

impl RepoContext {
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
        Self { repo, access: RepoAccess::Read, reader, disk_path }
    }
}

/// Explore page input, exactly as requested (`?q=…&sort=…&page=…`).
#[derive(Debug, Clone, Default)]
pub struct RepoSearch {
    pub text: String,
    pub sort: RepoSort,
    /// 1-based; `0` is treated as `1`.
    pub page: u32,
}

/// One page of explore results.
#[derive(Debug, Clone)]
pub struct RepoSearchResults {
    pub repos: Vec<OwnedRepository>,
    /// Matches across all pages.
    pub total: u64,
    /// 1-based page number actually served.
    pub page: u32,
    pub per_page: u32,
}

impl RepoSearchResults {
    /// Number of pages (at least 1, so "page 1 of 1" holds for empty results).
    pub fn page_count(&self) -> u32 {
        let pages = self.total.div_ceil(u64::from(self.per_page)).max(1);
        u32::try_from(pages).unwrap_or(u32::MAX)
    }

    pub fn has_previous(&self) -> bool {
        self.page > 1
    }

    pub fn has_next(&self) -> bool {
        self.page < self.page_count()
    }
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

    /// Creates a repository owned by `actor` personally. See [`Self::create_in`].
    pub async fn create(&self, actor: &User, input: CreateRepository) -> Result<OwnedRepository, ServiceError> {
        self.create_in(actor, actor.name().as_str(), input).await
    }

    /// Creates a repository owned by the account called `owner`: `actor` themselves, or
    /// an organisation `actor` belongs to (site administrators may use any organisation).
    ///
    /// Creates the metadata row and the bare repository. If the on-disk step fails the
    /// row is removed again, so the two never drift apart.
    pub async fn create_in(&self, actor: &User, owner: &str, input: CreateRepository) -> Result<OwnedRepository, ServiceError> {
        let owner = self.owner_for_new_repository(actor, owner.trim()).await?;
        let name: RepoName = input.name.trim().parse()?;
        let description = input.description.trim();
        if description.chars().count() > MAX_DESCRIPTION_LEN {
            return Err(ServiceError::Invalid(format!("description must be at most {MAX_DESCRIPTION_LEN} characters")));
        }

        let default_branch = self.config.repository.default_branch.clone();
        let repo = self
            .store
            .create(NewRepository {
                owner_id: owner.id,
                name,
                description: (!description.is_empty()).then(|| description.to_owned()),
                visibility: input.visibility,
                default_branch: default_branch.clone(),
            })
            .await?;

        if let Err(error) = self.storage.create(owner.id, repo.id, &default_branch).await {
            tracing::error!(%error, repo = %repo.name, "failed to initialise repository on disk; rolling back");
            self.store.delete(repo.id).await?;
            return Err(error.into());
        }

        // A member who is not an owner would otherwise depend on the organisation's base
        // permission to reach the repository they just created (with `none`, not at all),
        // so the creator administers it as a collaborator, as on other forges.
        if owner.is_organization() && !actor.is_admin && self.store.org_role(owner.id, actor.id()).await? == Some(OrgRole::Member) {
            self.store.grant_collaborator(repo.id, actor.id(), Permission::Admin).await?;
        }

        tracing::info!(actor = %actor.name(), owner = %owner.name, repo = %repo.name, "repository created");
        self.events.publish(DomainEvent::RepositoryCreated { repo: repo.id, actor: actor.id() });
        Ok(OwnedRepository { owner, repo })
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
        let access = repo_access(viewer, &repo, &grants);
        if !access.can_read() {
            return Err(ServiceError::NotFound);
        }
        let disk_path = self.storage.path_for(repo.repo.owner_id, repo.repo.id);
        let reader = self.storage.reader(repo.repo.owner_id, repo.repo.id);
        Ok(RepoContext { repo, access, reader, disk_path })
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
        let text: String = search.text.trim().chars().take(MAX_SEARCH_LEN).collect();
        let total = u64::try_from(self.store.count_public(&text).await?).unwrap_or_default();
        let mut results = RepoSearchResults { repos: Vec::new(), total, page: search.page.max(1), per_page: EXPLORE_PAGE_SIZE };
        results.page = results.page.min(results.page_count());

        let offset = (results.page - 1).saturating_mul(EXPLORE_PAGE_SIZE);
        results.repos = self.store.search_public(&text, search.sort, EXPLORE_PAGE_SIZE, offset).await?;
        Ok(results)
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
    if owner.is_empty() || owner.eq_ignore_ascii_case(actor.name().as_str()) {
        return Ok(actor.account.clone());
    }
    let not_allowed = || ServiceError::Invalid(format!("you cannot create repositories for {owner}"));
    let account = store.find_owner(owner).await?.ok_or_else(not_allowed)?;
    if !account.is_organization() {
        return Err(not_allowed());
    }
    if actor.is_admin || store.org_role(account.id, actor.id()).await?.is_some() { Ok(account) } else { Err(not_allowed()) }
}

/// What `viewer` was granted on `repo`, for [`repo_access`]. Skips the query when the
/// answer cannot change the outcome (anonymous viewers, site admins, the personal owner).
pub(crate) async fn viewer_grants(store: &RepositoryStore, viewer: Option<&User>, repo: &OwnedRepository) -> Result<Grants, ServiceError> {
    let Some(user) = viewer.filter(|user| !user.is_admin && user.id() != repo.repo.owner_id) else {
        return Ok(Grants::default());
    };
    let stored = store.grants(repo.repo.id, repo.repo.owner_id, user.id()).await?;
    Ok(Grants { org_role: stored.org_role, org_base: stored.org_base.permission(), team: stored.team, collaborator: stored.collaborator })
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
