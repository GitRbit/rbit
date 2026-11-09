//! Repository collaborators: adding people to a repository with a permission level.
//!
//! Management rules (administrators only, self-removal, who cannot be added, idempotent
//! `set`) live in [`rbit_access::collaborators`]; the storage adapter keeps one entry per
//! user and repository atomic. This adapter adds team grants to the listing, records the
//! audit log, publishes events and answers [`CollaboratorService::access_of`].
//!
//! Rules:
//! * Repository administrators add, change and remove collaborators. People are added
//!   directly (no invitations in wave A); organisations and suspended users cannot be
//!   added, nor can the personal owner of the repository.
//! * A collaborator may always remove themselves.
//! * Every change is recorded in the audit log ("Repository access") and published as
//!   [`DomainEvent::CollaboratorChanged`] (the `member` webhook).
//! * Administrators may ask what access anyone has ([`CollaboratorService::access_of`]),
//!   from every grant at once — the same policy every check uses.
//!
//! Every method takes a [`RepoContext`] from [`crate::RepositoryService::open`] for the
//! acting user, so the caller has already proven the repository is visible to them.

use rbit_access::collaborators::{CollaboratorRepository, CollaboratorScope, Collaborators, SetOutcome};
use rbit_core::access::repo_access;
use rbit_core::{
    Account, AccountId, RepoAccess, RepoId, User,
    membership::{Collaborator, Permission, TeamGrant},
};
use rbit_storage::CollaboratorAccessStore;

use crate::{
    AuditService, DomainEvent, RepoContext, ServiceContext, ServiceError,
    audit::{AuditAction, AuditEntry},
    events::CollaboratorChange,
    orgs::service_error,
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
    store: CollaboratorAccessStore,
    audit: AuditService,
}

type Rules<'a> = Collaborators<'a, CollaboratorAccessStore>;

fn scope(repo: &RepoContext) -> CollaboratorScope<RepoId, AccountId> {
    CollaboratorScope { repo: repo.repo.repo.id, owner: repo.repo.repo.owner_id, can_admin: repo.access.can_admin() }
}

impl CollaboratorService {
    pub(crate) fn new(ctx: ServiceContext) -> Self {
        Self { store: CollaboratorAccessStore::new(&ctx.database), audit: AuditService::new(ctx.clone()), ctx }
    }

    fn rules(&self) -> Rules<'_> {
        Collaborators::new(&self.store)
    }

    /// Collaborators and team grants of `repo`. Administrators only.
    pub async fn list(&self, repo: &RepoContext) -> Result<RepoCollaborators, ServiceError> {
        Rules::require_admin(scope(repo)).map_err(service_error)?;
        let id = repo.repo.repo.id;
        let teams = if repo.repo.owner.is_organization() { self.ctx.database.teams().grants_for_repository(id).await? } else { Vec::new() };
        Ok(RepoCollaborators { collaborators: self.rules().list(scope(repo)).await.map_err(service_error)?, teams })
    }

    /// Adds the user called `username` with `permission`.
    pub async fn add(&self, actor: &User, repo: &RepoContext, username: &str, permission: Permission) -> Result<(), ServiceError> {
        let user = self.rules().add(scope(repo), actor.id(), username, permission).await.map_err(service_error)?;
        self.added(actor, repo, user.id, &user.name, permission).await;
        Ok(())
    }

    /// Gives the collaborator called `username` a new permission.
    pub async fn change(&self, actor: &User, repo: &RepoContext, username: &str, permission: Permission) -> Result<(), ServiceError> {
        let (user, from) = self.rules().change(scope(repo), username, permission).await.map_err(service_error)?;
        self.changed(actor, repo, user.id, &user.name, from, permission).await;
        Ok(())
    }

    /// Removes the collaborator called `username`: administrators may remove anyone,
    /// collaborators themselves.
    pub async fn remove(&self, actor: &User, repo: &RepoContext, username: &str) -> Result<(), ServiceError> {
        let user = self.rules().remove(scope(repo), actor.id(), username).await.map_err(service_error)?;
        tracing::info!(repo = %repo.repo.full_name(), collaborator = %user.name, by = %actor.name(), "collaborator removed");
        self.record(AuditAction::CollaboratorRemoved, actor, repo, user.name.clone()).await;
        self.publish(actor, repo, user.id, CollaboratorChange::Removed);
        Ok(())
    }

    /// Adds the user called `username` with `permission`, or gives them `permission` if
    /// they already collaborate (nothing changes, and nothing is audited, when they have
    /// it already). Returns whether they were added.
    pub async fn set(&self, actor: &User, repo: &RepoContext, username: &str, permission: Permission) -> Result<bool, ServiceError> {
        Ok(match self.rules().set(scope(repo), actor.id(), username, permission).await.map_err(service_error)? {
            SetOutcome::Added(user) => {
                self.added(actor, repo, user.id, &user.name, permission).await;
                true
            }
            SetOutcome::Changed { user, from } => {
                self.changed(actor, repo, user.id, &user.name, from, permission).await;
                false
            }
            SetOutcome::Unchanged => false,
        })
    }

    /// The access the user called `username` has to `repo`, from every grant at once
    /// (ownership, site administration, organisation role and base permission, teams,
    /// their collaborator entry), and their account. Administrators only.
    pub async fn access_of(&self, repo: &RepoContext, username: &str) -> Result<(Account, RepoAccess), ServiceError> {
        Rules::require_admin(scope(repo)).map_err(service_error)?;
        let account = self.rules().find_user(username).await.map_err(service_error)?;
        let user = self.ctx.database.accounts().find_user(account.id).await?.ok_or(ServiceError::NotFound)?;
        let grants = viewer_grants(&self.ctx.database.repositories(), Some(&user), &repo.repo).await?;
        let access = repo_access(Some(&user), &repo.repo, &grants);
        Ok((user.account, access))
    }

    /// `actor`'s own collaborator permission on `repo`, if any (to offer "leave").
    pub async fn permission_of(&self, actor: &User, repo: &RepoContext) -> Result<Option<Permission>, ServiceError> {
        Ok(self.store.permission(repo.repo.repo.id, actor.id()).await?)
    }

    async fn added(&self, actor: &User, repo: &RepoContext, user: AccountId, name: &str, permission: Permission) {
        tracing::info!(repo = %repo.repo.full_name(), collaborator = %name, permission = permission.as_str(), by = %actor.name(), "collaborator added");
        self.record(AuditAction::CollaboratorAdded, actor, repo, format!("{name} ({permission})")).await;
        self.publish(actor, repo, user, CollaboratorChange::Added(permission));
    }

    async fn changed(&self, actor: &User, repo: &RepoContext, user: AccountId, name: &str, from: Permission, to: Permission) {
        tracing::info!(repo = %repo.repo.full_name(), collaborator = %name, permission = to.as_str(), by = %actor.name(), "collaborator changed");
        self.record(AuditAction::CollaboratorChanged, actor, repo, format!("{name} ({to})")).await;
        if from != to {
            self.publish(actor, repo, user, CollaboratorChange::Changed { from, to });
        }
    }

    fn publish(&self, actor: &User, repo: &RepoContext, user: AccountId, change: CollaboratorChange) {
        self.ctx.events.publish(DomainEvent::CollaboratorChanged { repo: repo.repo.repo.id, user, actor: actor.id(), change });
    }

    async fn record(&self, action: AuditAction, actor: &User, repo: &RepoContext, detail: String) {
        self.audit.record(AuditEntry::new(action).by(actor).on(repo.repo.full_name()).detail(detail)).await;
    }
}
