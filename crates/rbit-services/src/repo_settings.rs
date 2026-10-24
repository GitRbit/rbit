//! Repository settings: description, visibility, default branch, rename, archive, delete.
//!
//! Workstream D owns this file. Every method takes a [`RepoContext`] obtained from
//! [`crate::RepositoryService::open`] for the acting user, and requires admin access.
//!
//! Archiving makes a repository read-only ([`RepoContext::ensure_writable`] refuses every
//! change to its content) until it is unarchived; both are audited ("Repository
//! content") and publish [`DomainEvent::RepositoryArchived`], which `repository` webhooks
//! report as `archived` / `unarchived`. The settings on this page keep working, so an
//! archived repository can still be renamed, transferred, unarchived or deleted.

use rbit_core::{RepoName, Timestamp, User, Visibility};
use rbit_git::RefKind;
use rbit_storage::sqlite::repo_settings::{RepoGeneralSettings, RepoSettingsStore};

use crate::{
    AuditService, DomainEvent, RepoContext, ServiceContext, ServiceError,
    audit::{AuditAction, AuditEntry},
};

const MAX_DESCRIPTION_LEN: usize = 350;

/// Raw "general settings" input, exactly as submitted.
#[derive(Debug, Clone, Default)]
pub struct UpdateRepository {
    pub description: String,
    pub visibility: Visibility,
    /// `None` keeps the current default branch (e.g. an empty repository has no branches to choose from).
    pub default_branch: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RepoSettingsService {
    ctx: ServiceContext,
    audit: AuditService,
}

impl RepoSettingsService {
    pub(crate) fn new(ctx: ServiceContext) -> Self {
        Self { audit: AuditService::new(ctx.clone()), ctx }
    }

    fn store(&self) -> RepoSettingsStore {
        self.ctx.database.repo_settings()
    }

    /// Updates description, visibility and default branch together.
    ///
    /// A new default branch must exist; the bare repository's `HEAD` is pointed at it so
    /// that `git clone` checks it out.
    pub async fn update(&self, repo: &RepoContext, input: UpdateRepository) -> Result<(), ServiceError> {
        require_admin(repo)?;
        let description = input.description.trim();
        if description.chars().count() > MAX_DESCRIPTION_LEN {
            return Err(ServiceError::Invalid(format!("description must be at most {MAX_DESCRIPTION_LEN} characters")));
        }
        if description.chars().any(char::is_control) {
            return Err(ServiceError::Invalid("description must be a single line of text".to_owned()));
        }

        let current = &repo.repo.repo;
        let default_branch = match input.default_branch.map(|b| b.trim().to_owned()) {
            Some(branch) if branch != current.default_branch => {
                let refs = repo.reader.refs().await?;
                if !refs.iter().any(|r| r.kind == RefKind::Branch && r.name == branch) {
                    return Err(ServiceError::Invalid(format!("branch \"{branch}\" does not exist")));
                }
                Some(branch)
            }
            _ => None,
        };

        let settings = RepoGeneralSettings {
            description: (!description.is_empty()).then(|| description.to_owned()),
            visibility: input.visibility,
            default_branch: default_branch.clone().unwrap_or_else(|| current.default_branch.clone()),
        };

        let storage = &self.ctx.repositories;
        if let Some(branch) = &default_branch {
            storage.set_head(current.owner_id, current.id, branch).await?;
        }
        if let Err(error) = self.store().update_general(current.id, &settings).await {
            if default_branch.is_some()
                && let Err(restore) = storage.set_head(current.owner_id, current.id, &current.default_branch).await
            {
                tracing::error!(%restore, repo = %repo.repo.full_name(), "failed to restore HEAD after a failed settings update");
            }
            return Err(error.into());
        }

        tracing::info!(repo = %repo.repo.full_name(), visibility = settings.visibility.as_str(), default_branch = %settings.default_branch, "repository settings updated");
        Ok(())
    }

    /// Renames the repository and returns the new name. On-disk paths use ids, so nothing moves.
    pub async fn rename(&self, repo: &RepoContext, new_name: &str) -> Result<RepoName, ServiceError> {
        require_admin(repo)?;
        let name: RepoName = new_name.trim().parse()?;
        if name != repo.repo.repo.name {
            self.store().rename(repo.repo.repo.id, &name).await?;
            tracing::info!(repo = %repo.repo.full_name(), new_name = %name, "repository renamed");
        }
        Ok(name)
    }

    /// Archives the repository (`archived`) or unarchives it, as `actor`. `false` when it
    /// already was in that state: nothing is recorded or published then.
    pub async fn set_archived(&self, actor: &User, repo: &RepoContext, archived: bool) -> Result<bool, ServiceError> {
        require_admin(repo)?;
        let id = repo.repo.repo.id;
        let changed =
            if archived { self.store().archive(id, actor.id(), Timestamp::now()).await? } else { self.store().unarchive(id).await? };
        if !changed {
            return Ok(false);
        }
        let full_name = repo.repo.full_name();
        let action = if archived { AuditAction::RepositoryArchived } else { AuditAction::RepositoryUnarchived };
        tracing::info!(repo = %full_name, archived, by = %actor.name(), "repository archive state changed");
        self.audit.record(AuditEntry::new(action).by(actor).on(full_name)).await;
        self.ctx.events.publish(DomainEvent::RepositoryArchived { repo: id, archived, actor: actor.id() });
        Ok(true)
    }

    /// Permanently deletes the repository: its bare repository on disk, then its metadata
    /// (stars cascade). `confirmation` must be the full `owner/name`; `actor` is the
    /// administrator doing it (the access check itself is `repo`'s).
    ///
    /// Disk first: if that fails nothing is lost from the database and the deletion can
    /// simply be retried; if the database step fails afterwards, a retry still succeeds
    /// because removing an already-missing directory is not an error.
    pub async fn delete(&self, actor: &User, repo: &RepoContext, confirmation: &str) -> Result<(), ServiceError> {
        require_admin(repo)?;
        let full_name = repo.repo.full_name();
        if confirmation.trim() != full_name {
            return Err(ServiceError::Invalid(format!("type \"{full_name}\" to confirm")));
        }
        let current = &repo.repo.repo;
        self.ctx.repositories.delete(current.owner_id, current.id).await?;
        self.store().delete(current.id).await?;
        tracing::info!(repo = %full_name, "repository deleted");
        self.ctx.events.publish(DomainEvent::RepositoryDeleted { repo: current.id, owner: current.owner_id, full_name, actor: actor.id() });
        Ok(())
    }
}

fn require_admin(repo: &RepoContext) -> Result<(), ServiceError> {
    if repo.access.can_admin() { Ok(()) } else { Err(ServiceError::Forbidden) }
}
