//! Transferring a repository to another user or organisation.
//!
//! Milestone 2 workstream A (access) owns this file.
//!
//! Rules:
//! * Repository administrators may move a repository to their own account or to an
//!   organisation where they may create repositories (the rule of
//!   `RepositoryService::owner_for_new_repository`); site administrators may move it to
//!   any user or organisation. Suspended users cannot receive repositories.
//! * The full `owner/name` must be typed to confirm. A repository of the same name at the
//!   new owner is a validation error and leaves everything as it was.
//! * The bare repository is moved first (one same-filesystem rename), then the owner is
//!   changed in a transaction that also drops the old organisation's team grants. If the
//!   database step fails the directory is moved back, so disk and database never disagree.
//! * Collaborators, stars and everything else keyed by the repository id stay.
//!   [`DomainEvent::RepositoryTransferred`] is published and the transfer is audited.

use rustit_core::{Account, Repository, User, repository::OwnedRepository};
use rustit_git::relocate::relocate;
use rustit_storage::StorageError;

use crate::{
    AuditService, DomainEvent, RepoContext, ServiceContext, ServiceError,
    audit::{AuditAction, AuditEntry},
    repositories::authorised_owner,
};

#[derive(Debug, Clone)]
pub struct TransferService {
    ctx: ServiceContext,
    audit: AuditService,
}

impl TransferService {
    pub(crate) fn new(ctx: ServiceContext) -> Self {
        Self { audit: AuditService::new(ctx.clone()), ctx }
    }

    /// Accounts `actor` may move `repo` to without typing a name (their own account and
    /// their organisations), excluding the current owner. Site administrators may also
    /// enter any other name.
    pub async fn suggested_owners(&self, actor: &User, repo: &RepoContext) -> Result<Vec<Account>, ServiceError> {
        require_admin(repo)?;
        let memberships = self.ctx.database.orgs().memberships_of(actor.id()).await?;
        let candidates = std::iter::once(actor.account.clone()).chain(memberships.into_iter().map(|m| m.org));
        Ok(candidates.filter(|account| account.id != repo.repo.repo.owner_id).collect())
    }

    /// Moves `repo` to the account called `new_owner` and returns it as now owned.
    pub async fn transfer(
        &self,
        actor: &User,
        repo: &RepoContext,
        new_owner: &str,
        confirmation: &str,
    ) -> Result<OwnedRepository, ServiceError> {
        require_admin(repo)?;
        let full_name = repo.repo.full_name();
        let new_owner = new_owner.trim();
        if new_owner.is_empty() {
            return Err(ServiceError::Invalid("choose the new owner".to_owned()));
        }
        if confirmation.trim() != full_name {
            return Err(ServiceError::Invalid(format!("type \"{full_name}\" to confirm")));
        }

        let target = self.target(actor, new_owner).await?;
        let current = &repo.repo.repo;
        if target.id == current.owner_id {
            return Err(ServiceError::Invalid(format!("{full_name} already belongs to {}", target.name)));
        }

        let storage = &self.ctx.repositories;
        relocate(storage, current.id, current.owner_id, target.id).await?;
        if let Err(error) = self.ctx.database.repositories().transfer(current.id, target.id).await {
            if let Err(restore) = relocate(storage, current.id, target.id, current.owner_id).await {
                tracing::error!(%restore, repo = %full_name, "failed to move the repository back after a failed transfer");
            }
            return Err(match error {
                StorageError::Conflict(_) => {
                    ServiceError::Invalid(format!("{}/{} already exists; rename one of them first", target.name, current.name))
                }
                other => other.into(),
            });
        }

        let moved = OwnedRepository { repo: Repository { owner_id: target.id, ..current.clone() }, owner: target };
        tracing::info!(repo = %full_name, to = %moved.full_name(), by = %actor.name(), "repository transferred");
        self.ctx.events.publish(DomainEvent::RepositoryTransferred {
            repo: current.id,
            from: current.owner_id,
            to: moved.owner.id,
            actor: actor.id(),
        });
        self.audit
            .record(AuditEntry::new(AuditAction::RepositoryTransferred).by(actor).on(full_name).detail(format!("to {}", moved.full_name())))
            .await;
        Ok(moved)
    }

    /// The account `actor` may move a repository to.
    async fn target(&self, actor: &User, name: &str) -> Result<Account, ServiceError> {
        let store = self.ctx.database.repositories();
        let target = if actor.is_admin {
            store.find_owner(name).await?.ok_or_else(|| ServiceError::Invalid(format!("there is no user or organization called {name}")))?
        } else {
            authorised_owner(&store, actor, name).await.map_err(|error| match error {
                ServiceError::Invalid(_) => ServiceError::Invalid(format!("you cannot transfer repositories to {name}")),
                other => other,
            })?
        };
        if !target.is_organization() && self.ctx.database.accounts().is_suspended(target.id).await? {
            return Err(ServiceError::Invalid(format!("{} is suspended", target.name)));
        }
        Ok(target)
    }
}

fn require_admin(repo: &RepoContext) -> Result<(), ServiceError> {
    if repo.access.can_admin() { Ok(()) } else { Err(ServiceError::Forbidden) }
}
