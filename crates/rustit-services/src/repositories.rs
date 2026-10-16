//! Repository lifecycle and permission-checked access.
//!
//! Access follows [`rustit_auth::repo_access`]: this service looks up the viewer's role in
//! the owning organisation (if any) and lets the pure policy decide.

use std::{path::PathBuf, sync::Arc};

use rustit_auth::repo_access;
use rustit_core::{Account, Config, OrgRole, RepoAccess, RepoId, RepoName, RepoSort, User, Visibility, repository::OwnedRepository};
use rustit_git::{RepositoryReader, RepositoryStorage};
use rustit_storage::{NewRepository, RepositoryStore};

use crate::{ServiceContext, ServiceError};

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
        let reader = RepositoryReader::new(disk_path.clone());
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
}

impl RepositoryService {
    pub(crate) fn new(config: Arc<Config>, store: RepositoryStore, storage: RepositoryStorage) -> Self {
        Self { config, store, storage }
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

        tracing::info!(actor = %actor.name(), owner = %owner.name, repo = %repo.name, "repository created");
        Ok(OwnedRepository { owner, repo })
    }

    /// Resolves and authorises the owner of a new repository.
    async fn owner_for_new_repository(&self, actor: &User, owner: &str) -> Result<Account, ServiceError> {
        if owner.is_empty() || owner.eq_ignore_ascii_case(actor.name().as_str()) {
            return Ok(actor.account.clone());
        }
        let not_allowed = || ServiceError::Invalid(format!("you cannot create repositories for {owner}"));
        let account = self.store.find_owner(owner).await?.ok_or_else(not_allowed)?;
        if !account.is_organization() {
            return Err(not_allowed());
        }
        if actor.is_admin || self.store.org_role(account.id, actor.id()).await?.is_some() { Ok(account) } else { Err(not_allowed()) }
    }

    /// Loads `owner/name` for `viewer`. Repositories the viewer cannot read are reported
    /// as [`ServiceError::NotFound`] so their existence is not leaked.
    pub async fn open(&self, viewer: Option<&User>, owner: &str, name: &str) -> Result<RepoContext, ServiceError> {
        let name = name.strip_suffix(".git").unwrap_or(name);
        let repo = self.store.find_by_path(owner, name).await?.ok_or(ServiceError::NotFound)?;
        let role = self.viewer_role(viewer, &repo.owner).await?;
        let access = repo_access(viewer, &repo, role);
        if !access.can_read() {
            return Err(ServiceError::NotFound);
        }
        let disk_path = self.storage.path_for(repo.repo.owner_id, repo.repo.id);
        let reader = RepositoryReader::new(disk_path.clone());
        Ok(RepoContext { repo, access, reader, disk_path })
    }

    /// The viewer's role in `owner` when it is an organisation. Skips the query when the
    /// answer cannot change the outcome (anonymous viewers, site admins, user owners).
    async fn viewer_role(&self, viewer: Option<&User>, owner: &Account) -> Result<Option<OrgRole>, ServiceError> {
        match viewer {
            Some(user) if owner.is_organization() && !user.is_admin => Ok(self.store.org_role(owner.id, user.id()).await?),
            _ => Ok(None),
        }
    }

    /// Repositories of `owner` that `viewer` may see: private ones only for the owner
    /// themselves, members of an owning organisation, and site admins.
    pub async fn list_for_owner(&self, viewer: Option<&User>, owner: &Account) -> Result<Vec<OwnedRepository>, ServiceError> {
        let include_private = match viewer {
            None => false,
            Some(user) => user.is_admin || user.id() == owner.id || self.viewer_role(viewer, owner).await?.is_some(),
        };
        Ok(self.store.list_by_owner(owner.id, include_private).await?)
    }

    /// The dashboard list: `user`'s own repositories and those of their organisations,
    /// most recently updated first.
    pub async fn list_for_dashboard(&self, user: &User) -> Result<Vec<OwnedRepository>, ServiceError> {
        Ok(self.store.list_for_member(user.id(), DASHBOARD_LIMIT).await?)
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
