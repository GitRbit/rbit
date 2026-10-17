//! Repository collaborators: adding people to a repository with a permission level.
//!
//! Milestone 2 workstream A (access) owns this file.
//!
//! Rules:
//! * Repository administrators add, change and remove collaborators. People are added
//!   directly (no invitations in wave A); organisations and suspended users cannot be
//!   added, nor can the personal owner of the repository.
//! * A collaborator may always remove themselves.
//! * Every change is recorded in the audit log ("Repository access").
//! * Administrators may ask what access anyone has ([`CollaboratorService::access_of`]),
//!   from every grant at once — the same policy every check uses.
//!
//! Every method takes a [`RepoContext`] from [`crate::RepositoryService::open`] for the
//! acting user, so the caller has already proven the repository is visible to them.

use rbit_auth::repo_access;
use rbit_core::{
    Account, AccountKind, RepoAccess, User,
    membership::{Collaborator, Permission, TeamGrant},
};
use rbit_storage::{StorageError, sqlite::collaborators::CollaboratorStore};

use crate::{
    AuditService, RepoContext, ServiceContext, ServiceError,
    audit::{AuditAction, AuditEntry},
    repositories::viewer_grants,
};

/// Everyone with direct or team access to a repository, for its collaborators page.
#[derive(Debug, Clone, Default)]
pub struct RepoCollaborators {
    pub collaborators: Vec<Collaborator>,
    /// Teams of the owning organisation granted the repository (read-only here; managed
    /// on the team's page).
    pub teams: Vec<TeamGrant>,
}

#[derive(Debug, Clone)]
pub struct CollaboratorService {
    ctx: ServiceContext,
    audit: AuditService,
}

impl CollaboratorService {
    pub(crate) fn new(ctx: ServiceContext) -> Self {
        Self { audit: AuditService::new(ctx.clone()), ctx }
    }

    fn store(&self) -> CollaboratorStore {
        self.ctx.database.collaborators()
    }

    /// Collaborators and team grants of `repo`. Administrators only.
    pub async fn list(&self, repo: &RepoContext) -> Result<RepoCollaborators, ServiceError> {
        require_admin(repo)?;
        let id = repo.repo.repo.id;
        let teams = if repo.repo.owner.is_organization() { self.ctx.database.teams().grants_for_repository(id).await? } else { Vec::new() };
        Ok(RepoCollaborators { collaborators: self.store().list(id).await?, teams })
    }

    /// Adds the user called `username` with `permission`.
    pub async fn add(&self, actor: &User, repo: &RepoContext, username: &str, permission: Permission) -> Result<(), ServiceError> {
        require_admin(repo)?;
        let user = self.find_user(username).await?;
        if user.id == repo.repo.repo.owner_id {
            return Err(ServiceError::Invalid(format!("{} owns this repository", user.name)));
        }
        if self.ctx.database.accounts().is_suspended(user.id).await? {
            return Err(ServiceError::Invalid(format!("{} is suspended and cannot be added", user.name)));
        }
        match self.store().add(repo.repo.repo.id, user.id, permission, actor.id()).await {
            Ok(()) => {}
            Err(StorageError::Conflict(_)) => return Err(ServiceError::Invalid(format!("{} is already a collaborator", user.name))),
            Err(other) => return Err(other.into()),
        }
        tracing::info!(repo = %repo.repo.full_name(), collaborator = %user.name, permission = permission.as_str(), by = %actor.name(), "collaborator added");
        self.record(AuditAction::CollaboratorAdded, actor, repo, format!("{} ({permission})", user.name)).await;
        Ok(())
    }

    /// Gives the collaborator called `username` a new permission.
    pub async fn change(&self, actor: &User, repo: &RepoContext, username: &str, permission: Permission) -> Result<(), ServiceError> {
        require_admin(repo)?;
        let user = self.find_user(username).await?;
        if !self.store().set_permission(repo.repo.repo.id, user.id, permission).await? {
            return Err(ServiceError::Invalid(format!("{} is not a collaborator", user.name)));
        }
        tracing::info!(repo = %repo.repo.full_name(), collaborator = %user.name, permission = permission.as_str(), by = %actor.name(), "collaborator changed");
        self.record(AuditAction::CollaboratorChanged, actor, repo, format!("{} ({permission})", user.name)).await;
        Ok(())
    }

    /// Removes the collaborator called `username`: administrators may remove anyone,
    /// collaborators themselves.
    pub async fn remove(&self, actor: &User, repo: &RepoContext, username: &str) -> Result<(), ServiceError> {
        let user = self.find_user(username).await?;
        if user.id != actor.id() {
            require_admin(repo)?;
        }
        if !self.store().remove(repo.repo.repo.id, user.id).await? {
            return Err(ServiceError::Invalid(format!("{} is not a collaborator", user.name)));
        }
        tracing::info!(repo = %repo.repo.full_name(), collaborator = %user.name, by = %actor.name(), "collaborator removed");
        self.record(AuditAction::CollaboratorRemoved, actor, repo, user.name.to_string()).await;
        Ok(())
    }

    /// Adds the user called `username` with `permission`, or gives them `permission` if
    /// they already collaborate (nothing changes, and nothing is audited, when they have
    /// it already). Returns whether they were added.
    pub async fn set(&self, actor: &User, repo: &RepoContext, username: &str, permission: Permission) -> Result<bool, ServiceError> {
        require_admin(repo)?;
        let user = self.find_user(username).await?;
        match self.store().permission(repo.repo.repo.id, user.id).await? {
            None => self.add(actor, repo, user.name.as_str(), permission).await.map(|()| true),
            Some(current) if current == permission => Ok(false),
            Some(_) => self.change(actor, repo, user.name.as_str(), permission).await.map(|()| false),
        }
    }

    /// The access the user called `username` has to `repo`, from every grant at once
    /// (ownership, site administration, organisation role and base permission, teams,
    /// their collaborator entry), and their account. Administrators only.
    pub async fn access_of(&self, repo: &RepoContext, username: &str) -> Result<(Account, RepoAccess), ServiceError> {
        require_admin(repo)?;
        let account = self.find_user(username).await?;
        let user = self.ctx.database.accounts().find_user(account.id).await?.ok_or(ServiceError::NotFound)?;
        let grants = viewer_grants(&self.ctx.database.repositories(), Some(&user), &repo.repo).await?;
        Ok((account, repo_access(Some(&user), &repo.repo, &grants)))
    }

    /// `actor`'s own collaborator permission on `repo`, if any (to offer "leave").
    pub async fn permission_of(&self, actor: &User, repo: &RepoContext) -> Result<Option<Permission>, ServiceError> {
        Ok(self.store().permission(repo.repo.repo.id, actor.id()).await?)
    }

    /// The user account called `username`; organisations cannot be collaborators.
    async fn find_user(&self, username: &str) -> Result<Account, ServiceError> {
        let username = username.trim();
        if username.is_empty() {
            return Err(ServiceError::Invalid("enter a username".to_owned()));
        }
        match self.ctx.database.accounts().find_account_by_name(username).await? {
            Some(account) if account.kind == AccountKind::User => Ok(account),
            _ => Err(ServiceError::Invalid(format!("there is no user called {username}"))),
        }
    }

    async fn record(&self, action: AuditAction, actor: &User, repo: &RepoContext, detail: String) {
        self.audit.record(AuditEntry::new(action).by(actor).on(repo.repo.full_name()).detail(detail)).await;
    }
}

fn require_admin(repo: &RepoContext) -> Result<(), ServiceError> {
    if repo.access.can_admin() { Ok(()) } else { Err(ServiceError::Forbidden) }
}
