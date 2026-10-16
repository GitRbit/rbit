//! Forks: copying a repository to another owner and remembering where it came from.
//!
//! Milestone 2 workstream R (repository essentials) owns this file.
//!
//! # Rules
//!
//! * Anyone who can read a repository may fork it, into their own account or an
//!   organisation where they may create repositories (the same rule as for new
//!   repositories, [`RepositoryService::owner_for_new_repository`]).
//! * The fork keeps the parent's visibility (a private parent yields a private fork),
//!   description and default branch; its branches and tags are copied on disk with
//!   `git clone --bare --local` (see [`rustit_git::fork`]).
//! * The row is created first; if the disk step (or recording the relationship) fails,
//!   the row is removed again so metadata and disk never drift apart.
//! * Forking publishes [`DomainEvent::RepositoryForked`] — not `RepositoryCreated`.
//! * Deleting the parent leaves its forks intact; they simply lose their parent.

use rustit_core::{RepoName, User, repository::OwnedRepository};
use rustit_git::fork::fork_repository;
use rustit_storage::NewRepository;

use crate::{DomainEvent, RepoContext, RepositoryService, ServiceContext, ServiceError};

/// Forks listed on the forks page.
pub const MAX_LISTED_FORKS: u32 = 100;

/// Raw fork form input.
#[derive(Debug, Clone, Default)]
pub struct ForkInput {
    /// The new owner's name; empty means the actor.
    pub owner: String,
    /// The fork's name; empty means the parent's name.
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct ForkService {
    ctx: ServiceContext,
    repositories: RepositoryService,
}

impl ForkService {
    pub(crate) fn new(ctx: ServiceContext, repositories: RepositoryService) -> Self {
        Self { ctx, repositories }
    }

    /// Forks `parent` (opened for `actor`, so `actor` can read it).
    pub async fn fork(&self, actor: &User, parent: &RepoContext, input: ForkInput) -> Result<OwnedRepository, ServiceError> {
        if !parent.access.can_read() {
            return Err(ServiceError::NotFound);
        }
        let owner = self.repositories.owner_for_new_repository(actor, input.owner.trim()).await?;
        let name: RepoName = match input.name.trim() {
            "" => parent.repo.repo.name.clone(),
            name => name.parse()?,
        };
        let source = &parent.repo.repo;
        let store = self.ctx.database.repositories();
        let repo = store
            .create(NewRepository {
                owner_id: owner.id,
                name,
                description: source.description.clone(),
                visibility: source.visibility,
                default_branch: source.default_branch.clone(),
            })
            .await?;

        let target = self.ctx.repositories.path_for(owner.id, repo.id);
        let copied = async {
            fork_repository(&self.ctx.config.repository.git_binary, &parent.disk_path, &target).await?;
            self.ctx.database.forks().record(repo.id, source.id).await?;
            Ok::<_, ServiceError>(())
        };
        if let Err(error) = copied.await {
            tracing::error!(%error, parent = %parent.repo.full_name(), "forking failed; rolling back");
            if let Err(error) = self.ctx.repositories.delete(owner.id, repo.id).await {
                tracing::error!(%error, "cannot remove the partial fork from disk");
            }
            store.delete(repo.id).await?;
            return Err(error);
        }
        if source.pushed_at.is_some()
            && let Err(error) = store.mark_pushed(repo.id).await
        {
            tracing::warn!(%error, "cannot record the fork's content");
        }

        tracing::info!(actor = %actor.name(), parent = %parent.repo.full_name(), fork = %format!("{}/{}", owner.name, repo.name), "repository forked");
        self.ctx.events.publish(DomainEvent::RepositoryForked { repo: repo.id, parent: source.id, actor: actor.id() });
        Ok(OwnedRepository { owner, repo })
    }

    /// The repository `repo` was forked from, when it still exists and `viewer` may read it.
    pub async fn parent(&self, viewer: Option<&User>, repo: &RepoContext) -> Result<Option<OwnedRepository>, ServiceError> {
        let Some(parent) = self.ctx.database.forks().parent_of(repo.repo.repo.id).await? else { return Ok(None) };
        if parent.repo.visibility.is_public() {
            return Ok(Some(parent));
        }
        match self.repositories.open(viewer, parent.owner.name.as_str(), parent.repo.name.as_str()).await {
            Ok(_) => Ok(Some(parent)),
            Err(ServiceError::NotFound) => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// How many forks of `repo` the viewer may read.
    pub async fn count(&self, viewer: Option<&User>, repo: &RepoContext) -> Result<u64, ServiceError> {
        Ok(self.ctx.database.forks().count_readable(repo.repo.repo.id, viewer.map(User::id), viewer.is_some_and(|v| v.is_admin)).await?)
    }

    /// Forks of `repo` the viewer may read, newest first.
    pub async fn list(&self, viewer: Option<&User>, repo: &RepoContext) -> Result<Vec<OwnedRepository>, ServiceError> {
        Ok(self
            .ctx
            .database
            .forks()
            .list_readable(repo.repo.repo.id, viewer.map(User::id), viewer.is_some_and(|v| v.is_admin), MAX_LISTED_FORKS)
            .await?)
    }
}
