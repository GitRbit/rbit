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
//! Secrets and variables are managed through [`super::secrets`]. Runners and registration
//! tokens are added with the server side of the runner protocol.
//!
//! Workstream U (interface and secrets) owns this file.

use rbit_core::{
    Account, AccountId, RepoId, Timestamp, User,
    actions::{DefaultTokenPermission, SecretScope},
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

impl ActionsService {
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
        if store.is_enabled(repo_id).await? == enabled {
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
fn require_admin(repo: &RepoContext) -> Result<(), ServiceError> {
    if repo.access.can_admin() {
        Ok(())
    } else if repo.access.can_read() {
        Err(ServiceError::Forbidden)
    } else {
        Err(ServiceError::NotFound)
    }
}
