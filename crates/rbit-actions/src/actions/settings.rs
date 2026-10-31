//! A repository's Actions settings as its administrators manage them: the switch, the
//! default workflow token permission (§11.4 of `docs/design/actions.md`) and the pull
//! request authors it trusts (§11.1), plus the owner's repositories an owner secret can be
//! given to (§11.10).
//!
//! * Only repository administrators read or change these settings; anyone else is refused
//!   with `Forbidden`, and the pages answer 404 before asking.
//! * Every change is audited, and a request that changes nothing writes nothing and records
//!   nothing.
//! * Turning Actions off stops new runs (triggers, schedules and dispatch read the switch)
//!   and hides the tab; runs already stored stay as they are. Nothing else is touched here:
//!   no run is cancelled and no runner is told.
//!
//! The policies of GitHub's `actions/permissions` endpoints, a repository's and its
//! owner's ([`rbit_core::actions::permissions`]), are read and changed here too, by the
//! same people: repository administrators, organisation owners and users for their own
//! account. A repository can only narrow what its owner allows: a change past the owner's
//! ceiling is refused ([`PermissionsError::Capped`]) and a setting stored before the owner
//! narrowed is capped where it applies ([`ActionsService::effective_policies`]).
//!
//! Secrets and variables are managed through [`super::secrets`]. Runners and registration
//! tokens are added with the server side of the runner protocol.
//!
//! Workstream U (interface and secrets) owns this file.

use rbit_core::{
    Account, AccountId, RepoId, Timestamp, User,
    actions::{
        DefaultTokenPermission, SecretScope,
        permissions::{
            ActionsPolicies, AllowedActions, ApprovalPolicy, EffectivePolicies, EnabledRepositories, MAX_ACTION_PATTERNS,
            OwnerActionsSettings, SelectedActions, effective, invalid_pattern,
        },
    },
};

use super::{ActionsService, secrets::ActionsTarget};
use crate::{
    RepoContext, ServiceError,
    audit::{AuditAction, AuditEntry},
};

/// A repository's Actions settings as its settings page shows them.
#[derive(Debug, Clone)]
pub struct RepoActionsSettings {
    /// Whether workflows run in the repository. Off until switched on, unless
    /// `[actions] enable_for_new_repositories` switched it on at creation.
    pub enabled: bool,
    pub token_permission: DefaultTokenPermission,
    /// Pull request authors whose runs need no approval, oldest first.
    pub trusted: Vec<TrustedAuthor>,
}

/// A pull request author the repository trusts.
#[derive(Debug, Clone)]
pub struct TrustedAuthor {
    pub account: Account,
    /// Who approved the run that trusted them, while that account exists.
    pub approved_by: Option<Account>,
    pub since: Timestamp,
}

/// One of the owner's repositories, for the "selected repositories" of an owner secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnerRepository {
    pub id: RepoId,
    pub name: String,
}

// Actions: settings API.
/// A change to a repository's or owner's `actions/permissions`; `None` leaves a setting as
/// it is. `enabled` is a repository's switch, `enabled_repositories` an owner's.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PermissionsChange {
    pub enabled: Option<bool>,
    pub enabled_repositories: Option<EnabledRepositories>,
    pub allowed_actions: Option<AllowedActions>,
    pub selected_actions: Option<SelectedActions>,
    pub token_permission: Option<DefaultTokenPermission>,
    pub can_approve_reviews: Option<bool>,
    pub approval_policy: Option<ApprovalPolicy>,
}

impl PermissionsChange {
    fn apply(&self, mut policies: ActionsPolicies) -> ActionsPolicies {
        if let Some(allowed) = self.allowed_actions {
            policies.allowed_actions = allowed;
        }
        if let Some(selected) = &self.selected_actions {
            policies.selected_actions = selected.clone();
        }
        if let Some(permission) = self.token_permission {
            policies.token_permission = permission;
        }
        if let Some(can) = self.can_approve_reviews {
            policies.can_approve_reviews = can;
        }
        if let Some(policy) = self.approval_policy {
            policies.approval_policy = policy;
        }
        policies
    }

    /// The fields this change sets, by GitHub's names.
    fn fields(&self) -> Vec<&'static str> {
        [
            (self.allowed_actions.is_some(), "allowed_actions"),
            (self.token_permission.is_some(), "default_workflow_permissions"),
            (self.can_approve_reviews.is_some(), "can_approve_pull_request_reviews"),
            (self.approval_policy.is_some(), "approval_policy"),
        ]
        .into_iter()
        .filter_map(|(set, field)| set.then_some(field))
        .collect()
    }

    fn validate(&self) -> Result<(), ServiceError> {
        if let Some(selected) = &self.selected_actions {
            if selected.patterns_allowed.len() > MAX_ACTION_PATTERNS {
                return Err(ServiceError::Invalid(format!("at most {MAX_ACTION_PATTERNS} patterns are allowed")));
            }
            if let Some(problem) = selected.patterns_allowed.iter().find_map(|pattern| invalid_pattern(pattern)) {
                return Err(ServiceError::Invalid(problem.to_owned()));
            }
        }
        Ok(())
    }
}

/// Why a permissions change was refused.
#[derive(Debug, thiserror::Error)]
pub enum PermissionsError {
    #[error(transparent)]
    Service(#[from] ServiceError),
    /// The owner's policy does not allow this field's new value.
    #[error("the owner's Actions policy does not allow this `{0}`")]
    Capped(&'static str),
}

impl From<rbit_storage::StorageError> for PermissionsError {
    fn from(error: rbit_storage::StorageError) -> Self {
        Self::Service(error.into())
    }
}

/// A repository's `actions/permissions` as its administrators read them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoPermissions {
    /// The repository's own switch; whether its owner lets it run is [`Self::owner_allows`].
    pub enabled: bool,
    pub owner_allows: bool,
    pub policies: ActionsPolicies,
}

impl ActionsService {
    // Actions: settings API.
    /// `repo`'s `actions/permissions`, for an administrator.
    pub async fn repo_permissions(&self, repo: &RepoContext) -> Result<RepoPermissions, ServiceError> {
        require_admin(repo)?;
        let store = self.ctx().database.actions_settings();
        let id = repo.repo.repo.id;
        let owner = store.owner_settings(repo.repo.repo.owner_id).await?;
        Ok(RepoPermissions {
            enabled: store.is_switched_on(id).await?,
            owner_allows: self.owner_allows(&owner, repo.repo.repo.owner_id, id).await?,
            policies: store.policies(id).await?,
        })
    }

    async fn owner_allows(&self, owner: &OwnerActionsSettings, owner_id: AccountId, repo: RepoId) -> Result<bool, ServiceError> {
        Ok(match owner.enabled_repositories {
            EnabledRepositories::All => true,
            EnabledRepositories::None => false,
            EnabledRepositories::Selected => {
                self.ctx().database.actions_settings().owner_enabled_repositories(owner_id).await?.contains(&repo)
            }
        })
    }

    /// Changes `repo`'s `actions/permissions` for an administrator. A value past the owner's
    /// ceiling, or turning Actions on where the owner does not let the repository run, is
    /// [`PermissionsError::Capped`]; nothing is changed then.
    pub async fn set_repo_permissions(&self, actor: &User, repo: &RepoContext, change: &PermissionsChange) -> Result<(), PermissionsError> {
        require_admin(repo)?;
        change.validate()?;
        let store = self.ctx().database.actions_settings();
        let id = repo.repo.repo.id;
        let owner = store.owner_settings(repo.repo.repo.owner_id).await?;
        let current = store.policies(id).await?;
        let next = change.apply(current.clone());
        let changed = change.fields();
        if let Some(field) = next.exceeding(&owner.policies).into_iter().find(|field| changed.contains(field)) {
            return Err(PermissionsError::Capped(field));
        }
        if change.enabled == Some(true) && !self.owner_allows(&owner, repo.repo.repo.owner_id, id).await? {
            return Err(PermissionsError::Capped("enabled"));
        }
        if next != current {
            store.set_policies(id, &next, Timestamp::now()).await?;
            let details = format!("Actions permissions of {}: {}", repo.repo.full_name(), describe(&next));
            self.audit()
                .record(AuditEntry::new(AuditAction::ActionsPermissionsChanged).by(actor).on(repo.repo.full_name()).detail(details))
                .await;
        }
        if let Some(enabled) = change.enabled {
            self.set_repo_enabled(actor, repo, enabled).await?;
        }
        Ok(())
    }

    /// The owner's (an organisation's or a user's) Actions settings, for who manages them.
    pub async fn owner_permissions(&self, target: ActionsTarget<'_>) -> Result<OwnerActionsSettings, ServiceError> {
        let owner = owner_of(&target)?;
        target.require_manager()?;
        Ok(self.ctx().database.actions_settings().owner_settings(owner).await?)
    }

    /// Changes the owner's Actions settings. `enabled` is ignored: owners choose
    /// `enabled_repositories`.
    pub async fn set_owner_permissions(
        &self,
        actor: &User,
        target: ActionsTarget<'_>,
        change: &PermissionsChange,
    ) -> Result<(), ServiceError> {
        let owner = owner_of(&target)?;
        target.require_manager()?;
        change.validate()?;
        let store = self.ctx().database.actions_settings();
        let current = store.owner_settings(owner).await?;
        let next = OwnerActionsSettings {
            enabled_repositories: change.enabled_repositories.unwrap_or(current.enabled_repositories),
            policies: change.apply(current.policies.clone()),
        };
        if next != current {
            store.set_owner_settings(owner, &next, Timestamp::now()).await?;
            let details = format!(
                "Actions permissions of {}: repositories {}, {}",
                target.name(),
                next.enabled_repositories.as_str(),
                describe(&next.policies)
            );
            self.audit().record(AuditEntry::new(AuditAction::ActionsPermissionsChanged).by(actor).on(target.name()).detail(details)).await;
        }
        Ok(())
    }

    /// The repositories the owner selected to run workflows, by id and name.
    pub async fn owner_selected_repositories(&self, target: ActionsTarget<'_>) -> Result<Vec<OwnerRepository>, ServiceError> {
        let owner = owner_of(&target)?;
        target.require_manager()?;
        let selected = self.ctx().database.actions_settings().owner_enabled_repositories(owner).await?;
        Ok(self.owner_repositories(target).await?.into_iter().filter(|repo| selected.contains(&repo.id)).collect())
    }

    /// Replaces the owner's selected repositories; each must be the owner's (`NotFound`
    /// otherwise, so no other repository's id is confirmed).
    pub async fn set_owner_selected_repositories(
        &self,
        actor: &User,
        target: ActionsTarget<'_>,
        repos: &[RepoId],
    ) -> Result<(), ServiceError> {
        let owner = owner_of(&target)?;
        target.require_manager()?;
        let theirs: Vec<RepoId> = self.owner_repositories(target).await?.into_iter().map(|repo| repo.id).collect();
        if repos.iter().any(|repo| !theirs.contains(repo)) {
            return Err(ServiceError::NotFound);
        }
        let mut repos = repos.to_vec();
        repos.sort_unstable_by_key(|repo| repo.0);
        repos.dedup();
        self.ctx().database.actions_settings().set_owner_enabled_repositories(owner, &repos).await?;
        let details = format!("Actions run in {} selected repositories of {}", repos.len(), target.name());
        self.audit().record(AuditEntry::new(AuditAction::ActionsPermissionsChanged).by(actor).on(target.name()).detail(details)).await;
        Ok(())
    }

    /// Adds `repo` to, or removes it from, the owner's selected repositories.
    pub async fn select_owner_repository(
        &self,
        actor: &User,
        target: ActionsTarget<'_>,
        repo: RepoId,
        selected: bool,
    ) -> Result<(), ServiceError> {
        let owner = owner_of(&target)?;
        target.require_manager()?;
        if !self.owner_repositories(target).await?.iter().any(|owned| owned.id == repo) {
            return Err(ServiceError::NotFound);
        }
        if self.ctx().database.actions_settings().select_owner_repository(owner, repo, selected).await? {
            let verb = if selected { "can run" } else { "no longer runs" };
            let details = format!("repository {} of {} {verb} Actions", repo.0, target.name());
            self.audit().record(AuditEntry::new(AuditAction::ActionsPermissionsChanged).by(actor).on(target.name()).detail(details)).await;
        }
        Ok(())
    }

    /// What applies to `repo`'s runs: its policies capped by its owner's.
    pub async fn effective_policies(&self, repo: RepoId) -> Result<EffectivePolicies, ServiceError> {
        let database = &self.ctx().database;
        let store = database.actions_settings();
        let policies = store.policies(repo).await?;
        let owner = match database.admin().find_repository(repo).await? {
            Some(found) => store.owner_settings(found.repo.owner_id).await?,
            None => OwnerActionsSettings::default(),
        };
        Ok(effective(&policies, &owner.policies))
    }

    /// The Actions settings of `repo`, for an administrator.
    pub async fn repo_settings(&self, repo: &RepoContext) -> Result<RepoActionsSettings, ServiceError> {
        require_admin(repo)?;
        let store = self.ctx().database.actions_settings();
        let repo_id = repo.repo.repo.id;
        let stored = store.get(repo_id).await?;
        let trusted = store.trusted_users(repo_id).await?;
        let people = self.people(trusted.iter().flat_map(|row| [Some(row.user_id), row.approved_by])).await?;
        let trusted = trusted
            .into_iter()
            .filter_map(|row| {
                let account = people.get(&row.user_id)?.clone();
                let approved_by = row.approved_by.and_then(|id| people.get(&id)).cloned();
                Some(TrustedAuthor { account, approved_by, since: row.created_at })
            })
            .collect();
        Ok(RepoActionsSettings {
            enabled: stored.is_some_and(|settings| settings.enabled),
            token_permission: stored.map(|settings| settings.token_permission).unwrap_or_default(),
            trusted,
        })
    }

    /// Turns Actions on or off for `repo` as `actor` (an administrator; audited). Returns
    /// whether anything changed.
    pub async fn set_repo_enabled(&self, actor: &User, repo: &RepoContext, enabled: bool) -> Result<bool, ServiceError> {
        require_admin(repo)?;
        let store = self.ctx().database.actions_settings();
        let repo_id = repo.repo.repo.id;
        if store.is_switched_on(repo_id).await? == enabled {
            return Ok(false);
        }
        store.set_enabled(repo_id, enabled, Timestamp::now()).await?;
        let action = if enabled { AuditAction::ActionsEnabled } else { AuditAction::ActionsDisabled };
        self.audit().record(AuditEntry::new(action).by(actor).on(repo.repo.full_name())).await;
        Ok(true)
    }

    /// Sets the default permission of `repo`'s workflow tokens as `actor` (an administrator;
    /// audited). Returns whether anything changed.
    pub async fn set_token_permission(
        &self,
        actor: &User,
        repo: &RepoContext,
        permission: DefaultTokenPermission,
    ) -> Result<bool, ServiceError> {
        require_admin(repo)?;
        let store = self.ctx().database.actions_settings();
        let repo_id = repo.repo.repo.id;
        let current = store.get(repo_id).await?.map(|settings| settings.token_permission).unwrap_or_default();
        if current == permission {
            return Ok(false);
        }
        store.set_token_permission(repo_id, permission, Timestamp::now()).await?;
        let entry = AuditEntry::new(AuditAction::ActionsTokenPermissionChanged).by(actor).on(repo.repo.full_name()).detail(format!(
            "{} to {}",
            current.as_str(),
            permission.as_str()
        ));
        self.audit().record(entry).await;
        Ok(true)
    }

    /// Stops trusting `user`'s pull requests in `repo` as `actor` (an administrator;
    /// audited): their next untrusted runs wait for approval again. `NotFound` when they
    /// were not trusted.
    pub async fn untrust_user(&self, actor: &User, repo: &RepoContext, user: AccountId) -> Result<(), ServiceError> {
        require_admin(repo)?;
        let repo_id = repo.repo.repo.id;
        if !self.ctx().database.actions_settings().untrust(repo_id, user).await? {
            return Err(ServiceError::NotFound);
        }
        let name = self
            .people([Some(user)])
            .await?
            .remove(&user)
            .map_or_else(|| format!("account {}", user.0), |account| account.name.to_string());
        self.audit().record(AuditEntry::new(AuditAction::ActionsUserUntrusted).by(actor).on(repo.repo.full_name()).detail(name)).await;
        Ok(())
    }

    /// Every repository of `target`'s owner by name, for choosing the repositories an owner
    /// secret is given to. Owners only; a repository has no such choice.
    pub async fn owner_repositories(&self, target: ActionsTarget<'_>) -> Result<Vec<OwnerRepository>, ServiceError> {
        target.require_manager()?;
        let SecretScope::Owner(owner) = target.scope() else {
            return Err(ServiceError::Invalid("only owner secrets are given to selected repositories".to_owned()));
        };
        // Every repository, whoever may read it: an owner manages all of them.
        let stored = self.ctx().database.repositories().list_by_owner(owner, None, true).await?;
        let mut repositories: Vec<OwnerRepository> =
            stored.into_iter().map(|owned| OwnerRepository { id: owned.repo.id, name: owned.repo.name.to_string() }).collect();
        repositories.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(repositories)
    }
}

/// Repository administrators manage its Actions settings; readers learn it exists.
// Actions: settings API.
/// The account whose settings `target` names: an organisation or a user.
fn owner_of(target: &ActionsTarget<'_>) -> Result<AccountId, ServiceError> {
    match target {
        ActionsTarget::Organization(org) => Ok(org.org.account.id),
        ActionsTarget::User(user) => Ok(user.id()),
        ActionsTarget::Repository(_) | ActionsTarget::Environment(..) => Err(ServiceError::NotFound),
    }
}

/// The policies, as audit entries record them.
fn describe(policies: &ActionsPolicies) -> String {
    format!(
        "actions {}, token {}, approve reviews {}, approval {}",
        policies.allowed_actions.as_str(),
        policies.token_permission.as_str(),
        policies.can_approve_reviews,
        policies.approval_policy.as_str()
    )
}

fn require_admin(repo: &RepoContext) -> Result<(), ServiceError> {
    if repo.access.can_admin() {
        Ok(())
    } else if repo.access.can_read() {
        Err(ServiceError::Forbidden)
    } else {
        Err(ServiceError::NotFound)
    }
}
